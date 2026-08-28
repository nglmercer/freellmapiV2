//! Legacy
//! `POST /v1/completions` endpoint that wraps chat completions behind the
//! classic text-completion interface.

use std::collections::HashSet;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::future::join_all;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::providers::base::{ChunkReceiver, ProviderError};
use crate::routes::middleware::{
    api_key_auth, estimate_input_tokens, malformed_json, validate_completion_body_value,
};
use crate::services::ratelimit::{record_request, record_tokens, set_cooldown};
use crate::services::router::{record_rate_limit_hit, record_success, route_request, RouteResult};
use crate::services::telemetry::{record_observation, ObservationOutcome, RuntimeObservation};
use crate::types::{ChatMessage, CompletionOptions, MessageContent};

const MAX_RETRIES: i64 = 30;

fn is_retryable_error(err: &ProviderError) -> bool {
    // NOTE: the legacy completions path has a SHORTER retryable list than
    // the chat proxy (no 401/403/unauthorized/forbidden entries) — matches
    // completions.ts exactly.
    let msg = err.message.to_lowercase();
    [
        "429",
        "rate limit",
        "too many requests",
        "quota",
        "resource_exhausted",
        "aborted",
        "timeout",
        "etimedout",
        "econnrefused",
        "econnreset",
        "503",
        "unavailable",
        "500",
        "internal server error",
        "invalid url",
        "invalid uri",
    ]
    .iter()
    .any(|p| msg.contains(p))
}

fn build_chat_messages(prompt: &str, suffix: Option<&str>) -> Vec<ChatMessage> {
    let content = match suffix {
        Some(s) => format!("{prompt} {s}"),
        None => prompt.to_string(),
    };
    vec![ChatMessage::text("user", &content)]
}

fn preview(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Route handler for `POST /v1/completions` with authentication and
/// validation.
pub async fn completions_handler(headers: HeaderMap, body: Bytes) -> Response {
    if let Err(resp) = api_key_auth(&headers).await {
        return resp;
    }
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_json(),
    };
    let data = match validate_completion_body_value(&raw) {
        Ok(d) => d,
        Err(e) => return e.json(),
    };
    handle_completion(data).await
}

/// `handleCompletion(c, data)`
async fn handle_completion(data: crate::types::CompletionRequest) -> Response {
    let start = chrono::Utc::now().timestamp_millis();
    let prompts: Vec<String> = match data.prompt.clone() {
        crate::types::PromptField::Text(s) => vec![s],
        crate::types::PromptField::List(l) => l,
    };
    let suffix = data.suffix.clone();
    let do_stream = data.stream.unwrap_or(false);

    let estimated_input_tokens: i64 = prompts
        .iter()
        .map(|p| estimate_input_tokens(&[ChatMessage::text("user", p)]))
        .sum();
    let estimated_total = estimated_input_tokens + data.max_tokens.unwrap_or(16);
    let n = (data.n.unwrap_or(1)).min(prompts.len() as i64);
    let echo = data.echo.unwrap_or(false);

    // Build the pass-through options once (rest of CompletionRequest fields).
    // The legacy route passes only temperature/max_tokens/top_p/
    // seed/frequency_penalty/presence_penalty/user/logprobs/top_logprobs —
    // `stop` is validated but never forwarded, and `stream_options` is only
    // forwarded on the streaming path.
    let base_options = CompletionOptions {
        model: None,
        temperature: data.temperature,
        max_tokens: data.max_tokens,
        top_p: data.top_p,
        seed: data.seed,
        frequency_penalty: data.frequency_penalty,
        presence_penalty: data.presence_penalty,
        user: data.user.clone(),
        response_format: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        stream_options: None,
        logprobs: data.logprobs.map(|_| true),
        top_logprobs: data.logprobs,
        stop: None,
    };

    if do_stream {
        let mut options = base_options.clone();
        options.stream_options = data.stream_options.clone();
        handle_completion_stream(
            prompts,
            suffix,
            echo,
            n,
            estimated_input_tokens,
            estimated_total,
            options,
            start,
        )
        .await
    } else {
        handle_completion_standard(
            prompts,
            suffix,
            echo,
            n,
            estimated_input_tokens,
            estimated_total,
            base_options,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_completion_standard(
    prompts: Vec<String>,
    suffix: Option<String>,
    echo: bool,
    n: i64,
    estimated_input_tokens: i64,
    estimated_total: i64,
    options: CompletionOptions,
) -> Response {
    let _ = estimated_input_tokens;
    let mut skip_keys: HashSet<String> = HashSet::new();
    let mut last_error: Option<String> = None;

    for attempt in 0..MAX_RETRIES {
        let route = {
            let conn = db().lock().await;
            route_request(
                &conn,
                estimated_total,
                if !skip_keys.is_empty() {
                    Some(&skip_keys)
                } else {
                    None
                },
                None,
            )
        };
        let route = match route {
            Ok(r) => r,
            Err(_) => {
                return rate_limited_response(last_error.as_deref().unwrap_or("null"), false);
            }
        };

        let call = async {
            let timed_results = join_all(prompts.iter().take(n as usize).map(|prompt| async {
                let started = Instant::now();
                let result =
                    run_single_completion(&route, prompt, suffix.as_deref(), &options).await;
                (
                    result,
                    started.elapsed().as_millis().min(i64::MAX as u128) as i64,
                )
            }))
            .await;
            let mut results: Vec<SingleResult> = Vec::with_capacity(timed_results.len());
            let mut first_error: Option<ProviderError> = None;
            for (result, latency_ms) in timed_results {
                match result {
                    Ok(result) => {
                        record_tokens(
                            &route.platform,
                            &route.model_id,
                            route.key_id,
                            result.usage.total_tokens,
                        );
                        record_success(route.model_db_id);
                        record_request(&route.platform, &route.model_id, route.key_id);
                        let _ = record_observation(
                            &route.platform,
                            &route.model_id,
                            RuntimeObservation {
                                outcome: ObservationOutcome::Success,
                                latency_ms: Some(latency_ms),
                                ttft_ms: None,
                                output_tokens: Some(result.usage.completion_tokens),
                                generation_duration_ms: None,
                            },
                        )
                        .await;
                        results.push(result);
                    }
                    Err(error) => {
                        let _ = record_observation(
                            &route.platform,
                            &route.model_id,
                            RuntimeObservation {
                                outcome: ObservationOutcome::Failure {
                                    status_code: error.status,
                                    message: error.message.clone(),
                                },
                                latency_ms: Some(latency_ms),
                                ttft_ms: None,
                                output_tokens: None,
                                generation_duration_ms: None,
                            },
                        )
                        .await;
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
            }
            if let Some(error) = first_error {
                return Err(error);
            }

            let mut final_texts: Vec<String> = Vec::new();
            let mut total_input_tokens = 0i64;
            let mut total_output_tokens = 0i64;
            let mut total_tokens = 0i64;
            for (i, r) in results.into_iter().enumerate() {
                let text = if echo {
                    format!("{}{}", prompts[i], r.text)
                } else {
                    r.text.clone()
                };
                final_texts.push(text);
                total_input_tokens += r.usage.prompt_tokens;
                total_output_tokens += r.usage.completion_tokens;
                total_tokens += r.usage.total_tokens;
            }

            let choices: Vec<Value> = final_texts
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    json!({
                        "text": text,
                        "index": i,
                        "logprobs": Value::Null,
                        "finish_reason": "stop",
                    })
                })
                .collect();

            let response = json!({
                "id": format!("cmpl-{}", chrono::Utc::now().timestamp_millis()),
                "object": "text_completion",
                "created": chrono::Utc::now().timestamp(),
                "model": route.model_id,
                "choices": choices,
                "usage": {
                    "prompt_tokens": total_input_tokens,
                    "completion_tokens": total_output_tokens,
                    "total_tokens": total_tokens,
                },
                "_routed_via": {
                    "platform": route.platform,
                    "model": route.model_id,
                },
            });

            let mut res = (StatusCode::OK, axum::Json(response)).into_response();
            {
                let h = res.headers_mut();
                h.insert(
                    "x-routed-via",
                    format!("{}/{}", route.platform, route.model_id)
                        .parse()
                        .unwrap(),
                );
                if attempt > 0 {
                    h.insert("x-fallback-attempts", attempt.to_string().parse().unwrap());
                }
            }
            Ok::<Response, ProviderError>(res)
        }
        .await;

        let Err(err) = call else {
            return call.unwrap();
        };

        last_error = Some(err.message.clone());
        if is_retryable_error(&err) {
            skip_keys.insert(format!(
                "{}:{}:{}",
                route.platform, route.model_id, route.key_id
            ));
            set_cooldown(&route.platform, &route.model_id, route.key_id, 600_000);
            record_rate_limit_hit(route.model_db_id);
            tracing::info!(
                "[Completions] {} from {}, falling back ({}/{})",
                preview(&err.message, 60),
                route.display_name,
                attempt + 1,
                MAX_RETRIES
            );
            continue;
        }

        return (
            StatusCode::BAD_GATEWAY,
            axum::Json(json!({
                "error": {
                    "message": format!("Provider error ({}): {}", route.display_name, err.message),
                    "type": "provider_error",
                }
            })),
        )
            .into_response();
    }

    rate_limited_response(last_error.as_deref().unwrap_or("null"), true)
}

fn rate_limited_response(last: &str, exhausted: bool) -> Response {
    let message = if exhausted {
        format!("All models rate-limited after {MAX_RETRIES} attempts. Last: {last}")
    } else {
        format!("All models rate-limited. Last error: {last}")
    };
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(json!({ "error": { "message": message, "type": "rate_limit_error" } })),
    )
        .into_response()
}

struct SingleResult {
    text: String,
    usage: crate::types::TokenUsage,
}

async fn run_single_completion(
    route: &RouteResult,
    prompt: &str,
    suffix: Option<&str>,
    options: &CompletionOptions,
) -> Result<SingleResult, ProviderError> {
    let messages = build_chat_messages(prompt, suffix);
    let result = route
        .provider
        .chat_completion(&route.api_key, &messages, &route.model_id, options)
        .await?;

    let raw_content = result
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or(MessageContent::Null(()));
    let text = match raw_content {
        MessageContent::Text(s) => s,
        _ => String::new(),
    };
    Ok(SingleResult {
        text,
        usage: result.usage,
    })
}

#[allow(clippy::too_many_arguments)]
async fn handle_completion_stream(
    prompts: Vec<String>,
    suffix: Option<String>,
    echo: bool,
    n: i64,
    estimated_input_tokens: i64,
    estimated_total: i64,
    options: CompletionOptions,
    start: i64,
) -> Response {
    let _ = n;
    let mut skip_keys: HashSet<String> = HashSet::new();
    let mut last_error: Option<String> = None;

    for attempt in 0..MAX_RETRIES {
        let route = {
            let conn = db().lock().await;
            route_request(
                &conn,
                estimated_total,
                if !skip_keys.is_empty() {
                    Some(&skip_keys)
                } else {
                    None
                },
                None,
            )
        };
        let route = match route {
            Ok(r) => r,
            Err(_) => {
                return rate_limited_response(last_error.as_deref().unwrap_or("null"), false);
            }
        };

        let prompt = prompts.first().cloned().unwrap_or_default();
        let messages = build_chat_messages(&prompt, suffix.as_deref());
        let rx_result: Result<ChunkReceiver, ProviderError> = route
            .provider
            .stream_chat_completion(&route.api_key, &messages, &route.model_id, &options)
            .await;

        let mut rx = match rx_result {
            Ok(rx) => rx,
            Err(err) => {
                last_error = Some(err.message.clone());
                let _ = record_observation(
                    &route.platform,
                    &route.model_id,
                    RuntimeObservation {
                        outcome: ObservationOutcome::Failure {
                            status_code: err.status,
                            message: err.message.clone(),
                        },
                        latency_ms: Some(chrono::Utc::now().timestamp_millis() - start),
                        ttft_ms: None,
                        output_tokens: None,
                        generation_duration_ms: None,
                    },
                )
                .await;
                if is_retryable_error(&err) {
                    skip_keys.insert(format!(
                        "{}:{}:{}",
                        route.platform, route.model_id, route.key_id
                    ));
                    set_cooldown(&route.platform, &route.model_id, route.key_id, 600_000);
                    record_rate_limit_hit(route.model_db_id);
                    continue;
                }
                // Provider errors on this path use the plain-text server error
                // response.
                return (StatusCode::INTERNAL_SERVER_ERROR, err.message).into_response();
            }
        };

        // Pre-stream handshake: first chunk.
        let mut first_chunk: Option<(String, String, Option<String>, Option<i64>)> = None;
        let mut first_chunk_ttft_ms: Option<i64> = None;
        match rx.recv().await {
            Some(Ok(chunk)) => {
                let content = chunk
                    .choices
                    .first()
                    .and_then(|c| c.delta.content.clone())
                    .unwrap_or_default();
                let finish = chunk.choices.first().and_then(|c| c.finish_reason.clone());
                let usage = chunk
                    .usage
                    .as_ref()
                    .map(|usage| usage.completion_tokens)
                    .filter(|tokens| *tokens > 0);
                first_chunk = Some((chunk.id, content, finish, usage));
                first_chunk_ttft_ms = Some(chrono::Utc::now().timestamp_millis() - start);
            }
            Some(Err(err)) => {
                last_error = Some(err.message.clone());
                let _ = record_observation(
                    &route.platform,
                    &route.model_id,
                    RuntimeObservation {
                        outcome: ObservationOutcome::Failure {
                            status_code: err.status,
                            message: err.message.clone(),
                        },
                        latency_ms: Some(chrono::Utc::now().timestamp_millis() - start),
                        ttft_ms: None,
                        output_tokens: None,
                        generation_duration_ms: None,
                    },
                )
                .await;
                if is_retryable_error(&err) {
                    skip_keys.insert(format!(
                        "{}:{}:{}",
                        route.platform, route.model_id, route.key_id
                    ));
                    set_cooldown(&route.platform, &route.model_id, route.key_id, 600_000);
                    record_rate_limit_hit(route.model_db_id);
                    continue;
                }
                return (StatusCode::INTERNAL_SERVER_ERROR, err.message).into_response();
            }
            None => {}
        }

        let include_usage = options
            .stream_options
            .as_ref()
            .and_then(|so| so.include_usage)
            .unwrap_or(false);

        let routed_via_header = format!("{}/{}", route.platform, route.model_id);
        let platform = route.platform.clone();
        let model_id = route.model_id.clone();
        let model_db_id = route.model_db_id;
        let key_id = route.key_id;
        let display_name = route.display_name.clone();

        let body = async_stream::stream! {
            let mut total_output_tokens: i64 = 0;
            // Keep provider-reported usage separate from the character-based
            // estimate used only for the legacy response shape.
            let mut observed_output_tokens = first_chunk.as_ref().and_then(|(_, _, _, usage)| *usage);
            let mut stream_started = false;

            if let Some((ref id, ref content, ref finish, _usage)) = first_chunk {
                stream_started = true;
                let text = if echo { format!("{prompt}{content}") } else { content.clone() };
                total_output_tokens += (content.chars().count() as f64 / 4.0).ceil() as i64;
                let cc = json!({
                    "id": id,
                    "object": "text_completion.chunk",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model_id,
                    "choices": [{ "text": text, "index": 0, "logprobs": Value::Null, "finish_reason": finish }],
                });
                yield Ok::<Bytes, std::io::Error>(Bytes::from(format!("data: {cc}\n\n")));
            }

            let mut mid_stream_error: Option<String> = None;
            while let Some(item) = rx.recv().await {
                match item {
                    Ok(chunk) => {
                        if let Some(tokens) = chunk
                            .usage
                            .as_ref()
                            .map(|usage| usage.completion_tokens)
                            .filter(|tokens| *tokens > 0)
                        {
                            observed_output_tokens = Some(tokens);
                        }
                        let content = chunk.choices.first()
                            .and_then(|c| c.delta.content.clone())
                            .unwrap_or_default();
                        total_output_tokens += (content.chars().count() as f64 / 4.0).ceil() as i64;
                        let cc = json!({
                            "id": chunk.id,
                            "object": "text_completion.chunk",
                            "created": chunk.created,
                            "model": model_id,
                            "choices": [{
                                "text": content,
                                "index": 0,
                                "logprobs": Value::Null,
                                "finish_reason": chunk.choices.first().and_then(|c| c.finish_reason.clone()),
                            }],
                        });
                        yield Ok(Bytes::from(format!("data: {cc}\n\n")));
                    }
                    Err(err) => {
                        mid_stream_error = Some(err.message.clone());
                        break;
                    }
                }
            }

            if let Some(msg) = mid_stream_error {
                if stream_started {
                    tracing::error!("[Completions] Mid-stream error from {display_name}: {msg}");
                    let _ = record_observation(
                        &platform,
                        &model_id,
                        RuntimeObservation {
                            outcome: ObservationOutcome::Failure {
                                status_code: None,
                                message: msg.clone(),
                            },
                            latency_ms: Some(chrono::Utc::now().timestamp_millis() - start),
                            ttft_ms: first_chunk_ttft_ms,
                            // The compatibility stream estimates response
                            // usage for the client, but that estimate is not
                            // valid token telemetry.
                            output_tokens: observed_output_tokens,
                            generation_duration_ms: first_chunk_ttft_ms
                                .map(|ttft| chrono::Utc::now().timestamp_millis() - start - ttft)
                                .filter(|duration| *duration > 0),
                        },
                    )
                    .await;
                    let payload = json!({ "error": { "message": "stream interrupted", "type": "stream_error" } });
                    yield Ok(Bytes::from(format!("data: {payload}\n\n")));
                    yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));
                    return;
                }
                return;
            }

            let final_id = first_chunk.as_ref().map(|(id, _, _, _)| id.clone())
                .unwrap_or_else(|| format!("cmpl-{}", chrono::Utc::now().timestamp_millis()));
            let mut final_chunk = json!({
                "id": final_id,
                "object": "text_completion.chunk",
                "created": chrono::Utc::now().timestamp(),
                "model": model_id,
                "choices": [{ "text": "", "index": 0, "logprobs": Value::Null, "finish_reason": "stop" }],
            });
            if include_usage {
                final_chunk["usage"] = json!({
                    "prompt_tokens": estimated_input_tokens,
                    "completion_tokens": total_output_tokens,
                    "total_tokens": estimated_input_tokens + total_output_tokens,
                });
            }
            yield Ok(Bytes::from(format!("data: {final_chunk}\n\n")));
            yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));

            record_tokens(&platform, &model_id, key_id, estimated_input_tokens + total_output_tokens);
            record_success(model_db_id);
            record_request(&platform, &model_id, key_id);
            let elapsed_ms = chrono::Utc::now().timestamp_millis() - start;
            let _ = record_observation(
                &platform,
                &model_id,
                RuntimeObservation {
                    outcome: ObservationOutcome::Success,
                    latency_ms: Some(elapsed_ms),
                    ttft_ms: first_chunk_ttft_ms,
                    // Do not turn the character-based compatibility estimate
                    // into TPS when the provider supplied no usage.
                    output_tokens: observed_output_tokens,
                    generation_duration_ms: first_chunk_ttft_ms
                        .map(|ttft| elapsed_ms - ttft)
                        .filter(|duration| *duration > 0),
                },
            )
            .await;
        };

        let mut response = Response::new(Body::from_stream(body));
        *response.status_mut() = StatusCode::OK;
        {
            let h = response.headers_mut();
            h.insert(
                axum::http::header::CONTENT_TYPE,
                "text/event-stream".parse().unwrap(),
            );
            h.insert(
                axum::http::header::CACHE_CONTROL,
                "no-cache".parse().unwrap(),
            );
            h.insert("connection", "keep-alive".parse().unwrap());
            h.insert("x-routed-via", routed_via_header.parse().unwrap());
            if attempt > 0 {
                h.insert("x-fallback-attempts", attempt.to_string().parse().unwrap());
            }
        }
        let _ = start;
        return response;
    }

    rate_limited_response(last_error.as_deref().unwrap_or("null"), true)
}
