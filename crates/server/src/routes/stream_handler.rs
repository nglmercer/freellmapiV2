//! Server-Sent Events response handling for streaming completions.

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::providers::base::{ChunkReceiver, ProviderError};
use crate::services::ratelimit::{record_tokens, set_sticky_model};
use crate::services::router::{record_success, RouteResult};
use crate::services::telemetry::{record_observation, ObservationOutcome, RuntimeObservation};
use crate::types::{ChatCompletionChunk, ChatMessage, CompletionOptions};

fn sse_headers(platform_model: String, attempt: i64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
    headers.insert("connection", "keep-alive".parse().unwrap());
    headers.insert("x-routed-via", platform_model.parse().unwrap());
    if attempt > 0 {
        headers.insert("x-fallback-attempts", attempt.to_string().parse().unwrap());
    }
    headers
}

fn sse_line(obj: &serde_json::Value) -> Bytes {
    Bytes::from(format!("data: {}\n\n", obj))
}

fn chunk_delta_text(chunk: &ChatCompletionChunk) -> &str {
    chunk
        .choices
        .first()
        .and_then(|c| c.delta.content.as_deref())
        .unwrap_or("")
}

/// Handle SSE Stream Generation. `Err` means a pre-stream handshake failure —
/// bubble it up to the retry loop in proxy.rs so it can fail over to the
/// next model.
pub async fn handle_streaming_completion(
    route: &RouteResult,
    messages: Vec<ChatMessage>,
    options: CompletionOptions,
    estimated_input_tokens: i64,
    start: i64,
    attempt: i64,
) -> Result<Response, ProviderError> {
    let mut rx: ChunkReceiver = route
        .provider
        .stream_chat_completion(&route.api_key, &messages, &route.model_id, &options)
        .await?;

    // Pre-stream handshake: try to get the first chunk before the SSE body
    // starts. If this errors, propagate for retry.
    let mut first_chunk: Option<ChatCompletionChunk> = None;
    let mut stream_id: Option<String> = None;
    let mut first_chunk_ttft_ms: Option<i64> = None;
    match rx.recv().await {
        Some(Ok(chunk)) => {
            stream_id = Some(chunk.id.clone());
            first_chunk = Some(chunk);
            first_chunk_ttft_ms = Some(chrono::Utc::now().timestamp_millis() - start);
        }
        Some(Err(err)) => return Err(err),
        None => {}
    }

    let include_usage = options
        .stream_options
        .as_ref()
        .and_then(|so| so.include_usage)
        .unwrap_or(false);

    let platform = route.platform.clone();
    let model_id = route.model_id.clone();
    let model_db_id = route.model_db_id;
    let key_id = route.key_id;
    let display_name = route.display_name.clone();

    let body = async_stream::stream! {
        let mut total_output_tokens: i64 = 0;
        let mut stream_started = false;
        // Providers may send authoritative usage in a final chunk. Keep that
        // separate from the character-based compatibility estimate below.
        let mut observed_output_tokens: Option<i64> = first_chunk
            .as_ref()
            .and_then(|chunk| chunk.usage.as_ref())
            .map(|usage| usage.completion_tokens)
            .filter(|tokens| *tokens > 0);

        // Write the pre-fetched first chunk (if any)
        if let Some(ref first) = first_chunk {
            stream_started = true;
            total_output_tokens +=
                (chunk_delta_text(first).chars().count() as f64 / 4.0).ceil() as i64;
            yield Ok::<Bytes, std::io::Error>(sse_line(&serde_json::to_value(first).unwrap()));
        }

        // Write remaining chunks
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
                    total_output_tokens +=
                        (chunk_delta_text(&chunk).chars().count() as f64 / 4.0).ceil() as i64;
                    yield Ok(sse_line(&serde_json::to_value(&chunk).unwrap()));
                }
                Err(err) => {
                    mid_stream_error = Some(err.message.clone());
                    break;
                }
            }
        }

        if let Some(stream_err) = mid_stream_error {
            if stream_started {
                tracing::error!("[Proxy] Mid-stream error from {display_name}: {stream_err}");
                let payload_err = serde_json::json!({
                    "error": {
                        "message": format!("Provider error ({display_name}): stream interrupted"),
                        "type": "stream_error",
                    }
                });
                yield Ok(sse_line(&payload_err));
                yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));
                tracing::info!(
                    "{platform} {model_id} error {estimated_input_tokens} {total_output_tokens} {} {stream_err}",
                    chrono::Utc::now().timestamp_millis() - start
                );
                let _ = record_observation(
                    &platform,
                    &model_id,
                    RuntimeObservation {
                        outcome: ObservationOutcome::Failure {
                            status_code: None,
                            message: stream_err,
                        },
                        latency_ms: Some(chrono::Utc::now().timestamp_millis() - start),
                        ttft_ms: first_chunk_ttft_ms,
                        // Only provider-reported usage is authoritative. The
                        // character count above is only for the compatibility
                        // response usage field, so it must not become a
                        // fabricated TPS observation.
                        output_tokens: observed_output_tokens,
                        generation_duration_ms: first_chunk_ttft_ms
                            .map(|ttft| chrono::Utc::now().timestamp_millis() - start - ttft)
                            .filter(|duration| *duration > 0),
                    },
                )
                .await;
                return;
            }
            // Pre-stream errors are caught by the outer handshake; this
            // should not be reached.
            return;
        }

        // Emit usage in final chunk if requested (OpenAI compatibility)
        let mut final_chunk = serde_json::Map::new();
        final_chunk.insert(
            "id".to_string(),
            serde_json::json!(stream_id.clone().unwrap_or_else(|| {
                format!("chatcmpl-{}", chrono::Utc::now().timestamp_millis())
            })),
        );
        final_chunk.insert("object".into(), serde_json::json!("chat.completion.chunk"));
        final_chunk.insert("created".into(), serde_json::json!(chrono::Utc::now().timestamp()));
        final_chunk.insert("model".into(), serde_json::json!(&model_id));
        final_chunk.insert(
            "choices".into(),
            serde_json::json!([{ "index": 0, "delta": {}, "finish_reason": "stop" }]),
        );
        if include_usage {
            final_chunk.insert(
                "usage".into(),
                serde_json::json!({
                    "prompt_tokens": estimated_input_tokens,
                    "completion_tokens": total_output_tokens,
                    "total_tokens": estimated_input_tokens + total_output_tokens,
                }),
            );
        }
        yield Ok(sse_line(&serde_json::Value::Object(final_chunk)));
        yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));

        // Side-effects / Bookkeeping
        record_tokens(&platform, &model_id, key_id, estimated_input_tokens + total_output_tokens);
        record_success(model_db_id);
        set_sticky_model(&messages, model_db_id);
        let elapsed_ms = chrono::Utc::now().timestamp_millis() - start;
        let _ = record_observation(
            &platform,
            &model_id,
            RuntimeObservation {
                outcome: ObservationOutcome::Success,
                latency_ms: Some(elapsed_ms),
                ttft_ms: first_chunk_ttft_ms,
                // If the provider omitted usage, keep runtime TPS unknown
                // instead of deriving it from compatibility characters.
                output_tokens: observed_output_tokens,
                generation_duration_ms: first_chunk_ttft_ms
                    .map(|ttft| elapsed_ms - ttft)
                    .filter(|duration| *duration > 0),
            },
        )
        .await;
        tracing::info!(
            "{platform} {model_id} success {estimated_input_tokens} {total_output_tokens} {}",
            chrono::Utc::now().timestamp_millis() - start
        );
    };

    let mut response = Response::new(Body::from_stream(body));
    *response.status_mut() = StatusCode::OK;
    *response.headers_mut() =
        sse_headers(format!("{}/{}", route.platform, route.model_id), attempt);
    Ok(response)
}

/// Handle Standard Unary JSON response.
pub async fn handle_standard_completion(
    route: &RouteResult,
    messages: Vec<ChatMessage>,
    options: CompletionOptions,
    attempt: i64,
    start: i64,
) -> Result<Response, ProviderError> {
    let result = route
        .provider
        .chat_completion(&route.api_key, &messages, &route.model_id, &options)
        .await?;

    let total_tokens = result.usage.total_tokens;
    record_tokens(&route.platform, &route.model_id, route.key_id, total_tokens);
    record_success(route.model_db_id);
    set_sticky_model(&messages, route.model_db_id);
    let _ = record_observation(
        &route.platform,
        &route.model_id,
        RuntimeObservation {
            outcome: ObservationOutcome::Success,
            latency_ms: Some(chrono::Utc::now().timestamp_millis() - start),
            ttft_ms: None,
            output_tokens: Some(result.usage.completion_tokens),
            generation_duration_ms: None,
        },
    )
    .await;

    let mut response = axum::Json(result).into_response();
    {
        let h = response.headers_mut();
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
    Ok(response)
}
