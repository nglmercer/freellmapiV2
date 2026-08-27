//! Port of `server/src/routes/proxy.ts` — the OpenAI-compatible proxy
//! endpoints (`GET /v1/models`, `POST /v1/chat/completions`) with the
//! resilient fallback retry loop.

use std::collections::HashSet;

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::future::join_all;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::providers::base::ProviderError;
use crate::routes::middleware::{
    api_key_auth, estimate_input_tokens, malformed_json, normalize_messages,
    validate_chat_body_value,
};
use crate::routes::stream_handler::{handle_standard_completion, handle_streaming_completion};
use crate::services::ratelimit::{
    get_sticky_model, record_request, record_tokens, set_cooldown, set_sticky_model,
};
use crate::services::router::{
    record_rate_limit_hit, record_success, route_request, RouteResult,
};
use crate::types::{ChatCompletionResponse, ChatMessage, CompletionOptions};

// Virtual "auto" model. Clients like Hermes require a non-empty `model` field
// on every request, but freellmapi's whole point is to pick the model itself.
// Requesting this id means "let the router decide" — identical to omitting
// `model` entirely.
const AUTO_MODEL_ID: &str = "auto";

fn is_auto_model(model_id: Option<&String>) -> bool {
    model_id.map(|s| s.as_str()) == Some(AUTO_MODEL_ID)
}

const MAX_RETRIES: i64 = 30;

/// `errorMessage.slice(0, 60)` — UTF-8-safe prefix.
fn preview(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Port of `extractStatus` — routing errors carry their HTTP status;
/// anything out of range defaults to 503.
fn extract_status(status: u16) -> u16 {
    if (100..600).contains(&status) {
        status
    } else {
        503
    }
}

fn is_retryable_error(err: &ProviderError) -> bool {
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
        "401",
        "403",
        "unauthorized",
        "forbidden",
        "sign in",
    ]
    .iter()
    .any(|p| msg.contains(p))
}

/// `GET /v1/models` — OpenAI-compatible /models endpoint (used by Hermes for
/// metadata).
pub async fn list_models() -> Response {
    let rows: Vec<(String, String, String, Option<i64>)> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT platform, model_id, display_name, context_window FROM models \
             WHERE enabled = 1 ORDER BY intelligence_rank",
        )
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default()
    };

    let mut data = vec![json!({
        "id": AUTO_MODEL_ID,
        "object": "model",
        "created": 0,
        "owned_by": "freellmapi",
        "name": "Auto (router picks the best available model)",
        "context_window": Value::Null,
    })];
    for (platform, model_id, display_name, context_window) in rows {
        data.push(json!({
            "id": model_id,
            "object": "model",
            "created": 0,
            "owned_by": platform,
            "name": display_name,
            "context_window": context_window,
        }));
    }

    axum::Json(json!({ "object": "list", "data": data })).into_response()
}

fn log_request(
    platform: String,
    model_id: String,
    status: &'static str,
    input_tokens: i64,
    output_tokens: i64,
    latency_ms: i64,
    error: Option<String>,
) {
    tokio::spawn(async move {
        let conn = db().lock().await;
        conn.execute(
            "INSERT INTO requests (platform, model_id, status, input_tokens, output_tokens, latency_ms, error) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![platform, model_id, status, input_tokens, output_tokens, latency_ms, error],
        )
        .ok();
    });
}

/// `POST /v1/chat/completions`
pub async fn chat_completions(headers: HeaderMap, body: Bytes) -> Response {
    // apiKeyAuth
    if let Err(resp) = api_key_auth(&headers).await {
        return resp;
    }

    // validateChatBody
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_json(),
    };
    let data = match validate_chat_body_value(&raw) {
        Ok(d) => d,
        Err(e) => return e.json(),
    };

    let start = chrono::Utc::now().timestamp_millis();
    let requested_model = data.model.clone();
    let stream = data.stream.unwrap_or(false);
    let n = data.n.unwrap_or(1);
    let messages_raw = raw.get("messages").cloned().unwrap_or(json!([]));
    let messages = normalize_messages(&messages_raw);

    let options = CompletionOptions::from_request(&data);
    let estimated_input_tokens = estimate_input_tokens(&messages);
    let estimated_total = estimated_input_tokens + options.max_tokens.unwrap_or(1000);

    // Resolve preferred model: use requested model if specified, sticky
    // session for auto.
    let is_specific_model = !is_auto_model(requested_model.as_ref()) && requested_model.is_some();
    let mut preferred_model: Option<i64> = None;
    if is_auto_model(requested_model.as_ref()) {
        preferred_model = get_sticky_model(&messages);
    } else if let Some(ref requested) = requested_model {
        // Find the specific model in DB by modelId
        let conn = db().lock().await;
        preferred_model = conn
            .query_row(
                "SELECT id FROM models WHERE model_id = ?1 AND enabled = 1",
                rusqlite::params![requested],
                |r| r.get(0),
            )
            .ok();
        tracing::info!(
            "[Proxy] Requested specific model: {requested}, found ID: {preferred_model:?}, isSpecific: {is_specific_model}"
        );
    }

    let mut skip_keys: HashSet<String> = HashSet::new();
    let mut last_error: Option<String> = None;

    // Resilient Retry Loop
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
                preferred_model,
            )
        };
        let route = match route {
            Ok(r) => r,
            Err(err) => {
                if let Some(ref last) = last_error {
                    return (
                        StatusCode::TOO_MANY_REQUESTS,
                        axum::Json(json!({
                            "error": {
                                "message": format!("All models rate-limited. Last error: {last}"),
                                "type": "rate_limit_error",
                            }
                        })),
                    )
                        .into_response();
                }
                let status = extract_status(err.status);
                return (
                    StatusCode::from_u16(status).unwrap_or(StatusCode::SERVICE_UNAVAILABLE),
                    axum::Json(json!({
                        "error": { "message": err.message, "type": "routing_error" }
                    })),
                )
                    .into_response();
            }
        };

        // Provider call — runs WITHOUT the DB lock held.
        let call: Result<Response, ProviderError> = if stream {
            handle_streaming_completion(
                &route,
                messages.clone(),
                options.clone(),
                estimated_input_tokens,
                start,
                attempt,
            )
            .await
            .map(|resp| {
                // recordRequest after the response starts (TS line order)
                record_request(&route.platform, &route.model_id, route.key_id);
                resp
            })
        } else if n > 1 {
            handle_parallel(route.clone(), messages.clone(), options.clone(), n, attempt).await
        } else {
            handle_standard_completion(&route, messages.clone(), options.clone(), attempt)
                .await
                .map(|resp| {
                    record_request(&route.platform, &route.model_id, route.key_id);
                    resp
                })
        };

        let Err(err) = call else {
            return call.unwrap_ok();
        };

        let error_message = err.message.clone();
        tracing::error!(
            "[Proxy] Error: {{ platform: {}, model: {}, error: {error_message}, attempt: {attempt} }}",
            route.platform,
            route.model_id
        );
        log_request(
            route.platform.clone(),
            route.model_id.clone(),
            "error",
            estimated_input_tokens,
            0,
            chrono::Utc::now().timestamp_millis() - start,
            Some(error_message.clone()),
        );

        if is_retryable_error(&err) {
            // If a specific model was requested, don't fallback - return
            // error immediately.
            if is_specific_model {
                return provider_error_response(&route, &error_message);
            }

            skip_keys.insert(format!(
                "{}:{}:{}",
                route.platform, route.model_id, route.key_id
            ));
            set_cooldown(&route.platform, &route.model_id, route.key_id, 600_000);
            record_rate_limit_hit(route.model_db_id);
            last_error = Some(error_message.clone());
            tracing::info!(
                "[Proxy] {} from {}, falling back ({}/{})",
                preview(&error_message, 60),
                route.display_name,
                attempt + 1,
                MAX_RETRIES
            );
            continue;
        }

        // Check if this is a 404 error indicating model no longer available
        let is_model_not_found = error_message.contains("404")
            && (error_message.contains("no longer available")
                || error_message.contains("paid model")
                || error_message.contains("not found")
                || error_message.contains("model.*not found"));

        if is_model_not_found {
            // Mark the key as invalid for this specific model since it's no
            // longer available.
            {
                let conn = db().lock().await;
                conn.execute(
                    "UPDATE api_keys SET status = 'invalid', last_checked_at = datetime('now') \
                     WHERE platform = ?1 AND id = ?2",
                    rusqlite::params![route.platform, route.key_id],
                )
                .ok();
            }
            tracing::info!(
                "[Proxy] Marked key {} ({}:{}) as invalid due to 404: {error_message}",
                route.key_id,
                route.platform,
                route.model_id
            );
        }

        // Non-retryable provider error (and the 404 case above — no retry).
        return provider_error_response(&route, &error_message);
    }

    // Exhausted Retries
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(json!({
            "error": {
                "message": format!("All models rate-limited after {MAX_RETRIES} attempts. Last: {}", last_error.unwrap_or_default()),
                "type": "rate_limit_error",
            }
        })),
    )
        .into_response()
}

fn provider_error_response(route: &RouteResult, error_message: &str) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        axum::Json(json!({
            "error": {
                "message": format!("Provider error ({}): {error_message}", route.display_name),
                "type": "provider_error",
            }
        })),
    )
        .into_response()
}

/// The `n > 1` parallel branch: fire `n` requests at the same provider and
/// return merged choices. Bookkeeping matches TS exactly (no recordRequest
/// on this path).
async fn handle_parallel(
    route: RouteResult,
    messages: Vec<ChatMessage>,
    options: CompletionOptions,
    n: i64,
    attempt: i64,
) -> Result<Response, ProviderError> {
    let calls = (0..n).map(|_| {
        route.provider.chat_completion(
            &route.api_key,
            &messages,
            &route.model_id,
            &options,
        )
    });
    let results: Vec<ChatCompletionResponse> = join_all(calls)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, ProviderError>>()?;

    let mut merged_choices: Vec<Value> = Vec::new();
    let mut total_usage = json!({
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "total_tokens": 0,
    });
    for r in &results {
        let offset = merged_choices.len() as i64;
        for ch in &r.choices {
            let mut v = serde_json::to_value(ch).unwrap();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("index".to_string(), json!(ch.index + offset));
            }
            merged_choices.push(v);
        }
        let tu = total_usage.as_object_mut().unwrap();
        for (k, val) in [
            ("prompt_tokens", r.usage.prompt_tokens),
            ("completion_tokens", r.usage.completion_tokens),
            ("total_tokens", r.usage.total_tokens),
        ] {
            tu.insert(k.to_string(), json!(tu[k].as_i64().unwrap_or(0) + val));
        }
    }

    let total_tokens = total_usage["total_tokens"].as_i64().unwrap_or(0);
    record_tokens(&route.platform, &route.model_id, route.key_id, total_tokens);
    record_success(route.model_db_id);
    set_sticky_model(&messages, route.model_db_id);

    let mut response = (
        StatusCode::OK,
        axum::Json(json!({
            "id": results.first().map(|r| r.id.clone()).unwrap_or_default(),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": route.model_id,
            "choices": merged_choices,
            "usage": total_usage,
        })),
    )
        .into_response();
    {
        let h = response.headers_mut();
        h.insert(
            "x-routed-via",
            format!("{}/{}", route.platform, route.model_id).parse().unwrap(),
        );
        if attempt > 0 {
            h.insert("x-fallback-attempts", attempt.to_string().parse().unwrap());
        }
    }
    Ok(response)
}

/// Small extension to unwrap `Result<Response, _>` in the match-free flow.
trait UnwrapOk<T, E: std::fmt::Debug> {
    fn unwrap_ok(self) -> T;
}
impl<T, E: std::fmt::Debug> UnwrapOk<T, E> for Result<T, E> {
    fn unwrap_ok(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}
