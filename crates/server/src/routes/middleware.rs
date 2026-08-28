//! Request validation, unified-key authentication, message normalization, and
//! token estimation.

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::db::unified_key::get_unified_api_key;
use crate::types::{ChatCompletionRequest, ChatMessage, ContentPart, MessageContent};

/// Errors collect zod-style messages; the proxy route joins them with ", "
/// and renders `Invalid request: <joined>`.
#[derive(Debug)]
pub struct ValidationError(pub Vec<String>);

impl ValidationError {
    pub fn message(&self) -> String {
        format!("Invalid request: {}", self.0.join(", "))
    }
    pub fn json(&self) -> Response {
        (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": { "message": self.message(), "type": "invalid_request_error" }
            })),
        )
            .into_response()
    }
}

// ---- zod helpers ----

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn req_str_min1(v: &Value, key: &str, errors: &mut Vec<String>) {
    if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
        if s.is_empty() {
            errors.push("String must contain at least 1 character(s)".to_string());
        }
    }
}

fn opt_num(v: &Value, key: &str, min: Option<f64>, max: Option<f64>, errors: &mut Vec<String>) {
    let Some(n) = v.get(key) else { return };
    if n.is_null() {
        return;
    }
    let Some(f) = n.as_f64() else {
        errors.push(format!("Expected number, received {}", type_name(n)));
        return;
    };
    if let Some(min) = min {
        if f < min {
            errors.push(format!("Number must be greater than or equal to {min}"));
        }
    }
    if let Some(max) = max {
        if f > max {
            errors.push(format!("Number must be less than or equal to {max}"));
        }
    }
}

fn content_errors(content: &Value, allow_null: bool, path: &str, errors: &mut Vec<String>) {
    match content {
        Value::Null => {
            if !allow_null {
                errors.push(format!("{path}: Expected string, received null"));
            }
        }
        Value::String(_) => {}
        Value::Array(parts) => {
            for (i, part) in parts.iter().enumerate() {
                let Some(obj) = part.as_object() else {
                    errors.push(format!(
                        "{path}[{i}]: Expected object, received {}",
                        type_name(part)
                    ));
                    continue;
                };
                if obj.get("type").and_then(|t| t.as_str()).is_none() {
                    errors.push(format!(
                        "{path}[{i}].type: Expected string, received undefined"
                    ));
                }
                if let Some(t) = obj.get("text") {
                    if !t.is_null() && t.as_str().is_none() {
                        errors.push(format!("{path}[{i}].text: Expected string"));
                    }
                }
                if let Some(iu) = obj.get("image_url") {
                    if !iu.is_null() {
                        req_str_min1(iu, "url", errors);
                    }
                }
            }
        }
        other => {
            errors.push(format!(
                "{path}: Expected string or array, received {}",
                type_name(other)
            ));
        }
    }
}

/// `chatCompletionSchema.safeParse` — validates the raw JSON and returns the
/// typed request, defaulting `n` to one.
pub fn validate_chat_body_value(body: &Value) -> Result<ChatCompletionRequest, ValidationError> {
    let mut errors: Vec<String> = Vec::new();

    let Value::Object(_) = body else {
        return Err(ValidationError(vec![
            "Expected object, received invalid_json".to_string(),
        ]));
    };

    // messages: union of message schemas, min(1)
    let messages = body.get("messages");
    match messages {
        Some(Value::Array(list)) if !list.is_empty() => {
            for (i, m) in list.iter().enumerate() {
                let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("");
                match role {
                    "system" | "user" => {
                        if m.get("content").is_none() {
                            errors.push(format!("messages[{i}].content: Required"));
                        } else {
                            content_errors(
                                &m["content"],
                                false,
                                &format!("messages[{i}].content"),
                                &mut errors,
                            );
                        }
                    }
                    "assistant" => {
                        // content nullable().optional()
                        if let Some(c) = m.get("content") {
                            if !c.is_null() {
                                content_errors(
                                    c,
                                    false,
                                    &format!("messages[{i}].content"),
                                    &mut errors,
                                );
                            }
                        }
                        // refine: non-empty content or tool_calls
                        let has_content = match m.get("content") {
                            Some(Value::String(s)) => !s.is_empty(),
                            Some(Value::Array(a)) => !a.is_empty(),
                            _ => false,
                        };
                        let has_tool_calls = m
                            .get("tool_calls")
                            .and_then(|t| t.as_array())
                            .map(|a| !a.is_empty())
                            .unwrap_or(false);
                        if !has_content && !has_tool_calls {
                            errors.push(
                                "assistant messages must include non-empty content or tool_calls"
                                    .to_string(),
                            );
                        }
                        for (j, tc) in m
                            .get("tool_calls")
                            .and_then(|t| t.as_array())
                            .map(|a| a.iter().enumerate())
                            .into_iter()
                            .flatten()
                        {
                            req_str_min1(tc, "id", &mut errors);
                            if tc.get("type").and_then(|x| x.as_str()) != Some("function") {
                                errors.push(format!(
                                    "messages[{i}].tool_calls[{j}].type: Invalid literal value, expected \"function\""
                                ));
                            }
                            if let Some(f) = tc.get("function") {
                                req_str_min1(f, "name", &mut errors);
                                if f.get("arguments").and_then(|a| a.as_str()).is_none() {
                                    errors.push(format!("messages[{i}].tool_calls[{j}].function.arguments: Expected string, received undefined"));
                                }
                            } else {
                                errors.push(format!(
                                    "messages[{i}].tool_calls[{j}].function: Required"
                                ));
                            }
                        }
                    }
                    "tool" => {
                        if m.get("content").is_none() {
                            errors.push(format!("messages[{i}].content: Required"));
                        } else {
                            content_errors(
                                &m["content"],
                                false,
                                &format!("messages[{i}].content"),
                                &mut errors,
                            );
                        }
                        req_str_min1(m, "tool_call_id", &mut errors);
                    }
                    other => {
                        errors.push(format!(
                            "messages[{i}]: Invalid discriminator value. Expected 'system' | 'user' | 'assistant' | 'tool'"
                        ));
                        let _ = other;
                    }
                }
            }
        }
        Some(Value::Array(_)) => {
            errors.push("Array must contain at least 1 element(s)".to_string())
        }
        Some(other) => errors.push(format!(
            "messages: Expected array, received {}",
            type_name(other)
        )),
        None => errors.push("messages: Required".to_string()),
    }

    if let Some(model) = body.get("model") {
        if !model.is_null() && model.as_str().is_none() {
            errors.push("model: Expected string, received undefined".to_string());
        }
    }

    opt_num(body, "temperature", Some(0.0), Some(2.0), &mut errors);
    // max_tokens: int positive
    if let Some(v) = body.get("max_tokens") {
        if !v.is_null() {
            match v.as_f64() {
                None => errors.push("max_tokens: Expected number".to_string()),
                Some(f) => {
                    if f.fract() != 0.0 {
                        errors.push("max_tokens: Expected integer, received float".to_string());
                    }
                    if f <= 0.0 {
                        errors.push("max_tokens: Number must be greater than 0".to_string());
                    }
                }
            }
        }
    }
    // n: int 1..=10, default 1
    if let Some(v) = body.get("n") {
        if !v.is_null() {
            match v.as_f64() {
                None => errors.push("n: Expected number".to_string()),
                Some(f) => {
                    if f.fract() != 0.0 {
                        errors.push("n: Expected integer, received float".to_string());
                    }
                    if f < 1.0 {
                        errors.push("n: Number must be greater than or equal to 1".to_string());
                    }
                    if f > 10.0 {
                        errors.push("n: Number must be less than or equal to 10".to_string());
                    }
                }
            }
        }
    }
    opt_num(body, "top_p", Some(0.0), Some(1.0), &mut errors);
    if let Some(v) = body.get("seed") {
        if !v.is_null() && v.as_f64().is_none() {
            errors.push("seed: Expected number".to_string());
        }
    }
    opt_num(
        body,
        "frequency_penalty",
        Some(-2.0),
        Some(2.0),
        &mut errors,
    );
    opt_num(body, "presence_penalty", Some(-2.0), Some(2.0), &mut errors);
    if let Some(user) = body.get("user") {
        if !user.is_null() && user.as_str().is_none() {
            errors.push("user: Expected string".to_string());
        }
    }
    // response_format
    if let Some(rf) = body.get("response_format") {
        if !rf.is_null() {
            let t = rf.get("type").and_then(|x| x.as_str());
            if !matches!(t, Some("text") | Some("json_object") | Some("json_schema")) {
                errors.push(format!(
                    "response_format.type: Invalid enum value. Expected 'text' | 'json_object' | 'json_schema', received {:?}",
                    rf.get("type")
                ));
            }
        }
    }
    if let Some(stream) = body.get("stream") {
        if !stream.is_null() && !stream.is_boolean() {
            errors.push("stream: Expected boolean".to_string());
        }
    }
    // tools
    if let Some(tools) = body.get("tools") {
        if let Some(list) = tools.as_array() {
            for (i, t) in list.iter().enumerate() {
                if t.get("type").and_then(|x| x.as_str()) != Some("function") {
                    errors.push(format!(
                        "tools[{i}].type: Invalid literal value, expected \"function\""
                    ));
                }
                if let Some(f) = t.get("function") {
                    req_str_min1(f, "name", &mut errors);
                } else {
                    errors.push(format!("tools[{i}].function: Required"));
                }
            }
        }
    }
    // tool_choice union: enum or { type:'function', function:{name} }
    if let Some(tc) = body.get("tool_choice") {
        if !tc.is_null() {
            let ok = match tc {
                Value::String(s) => matches!(s.as_str(), "none" | "auto" | "required"),
                Value::Object(o) => {
                    o.get("type").and_then(|x| x.as_str()) == Some("function")
                        && o.get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .is_some_and(|n| !n.is_empty())
                }
                _ => false,
            };
            if !ok {
                errors.push("tool_choice: Invalid union value".to_string());
            }
        }
    }
    if let Some(ptc) = body.get("parallel_tool_calls") {
        if !ptc.is_null() && !ptc.is_boolean() {
            errors.push("parallel_tool_calls: Expected boolean".to_string());
        }
    }
    if let Some(l) = body.get("logprobs") {
        if !l.is_null() && !l.is_boolean() {
            errors.push("logprobs: Expected boolean".to_string());
        }
    }
    opt_num(body, "top_logprobs", Some(0.0), Some(5.0), &mut errors);

    if !errors.is_empty() {
        return Err(ValidationError(errors));
    }

    match serde_json::from_value::<ChatCompletionRequest>(body.clone()) {
        Ok(mut req) => {
            if req.n.is_none() {
                req.n = Some(1); // zod default
            }
            Ok(req)
        }
        Err(e) => Err(ValidationError(vec![e.to_string()])),
    }
}

/// `completionSchema.safeParse` — legacy completions route. Returns the
/// typed request.
pub fn validate_completion_body_value(
    body: &Value,
) -> Result<crate::types::CompletionRequest, ValidationError> {
    let mut errors: Vec<String> = Vec::new();

    req_str_min1(body, "model", &mut errors);
    match body.get("prompt") {
        Some(Value::String(_)) | Some(Value::Array(_)) => {}
        Some(other) => errors.push(format!(
            "prompt: Expected string or array, received {}",
            type_name(other)
        )),
        None => errors.push("prompt: Required".to_string()),
    }
    if let Some(s) = body.get("suffix") {
        if !s.is_null() && s.as_str().is_none() {
            errors.push("suffix: Expected string".to_string());
        }
    }
    // max_tokens int 1..=4000, default 16
    if let Some(v) = body.get("max_tokens") {
        if !v.is_null() {
            match v.as_f64() {
                None => errors.push("max_tokens: Expected number".to_string()),
                Some(f) => {
                    if f.fract() != 0.0 {
                        errors.push("max_tokens: Expected integer, received float".to_string());
                    }
                    if f < 1.0 {
                        errors.push(
                            "max_tokens: Number must be greater than or equal to 1".to_string(),
                        );
                    }
                    if f > 4000.0 {
                        errors.push(
                            "max_tokens: Number must be less than or equal to 4000".to_string(),
                        );
                    }
                }
            }
        }
    }
    opt_num(body, "temperature", Some(0.0), Some(2.0), &mut errors);
    opt_num(body, "top_p", Some(0.0), Some(1.0), &mut errors);
    if let Some(v) = body.get("n") {
        if !v.is_null() {
            match v.as_f64() {
                None => errors.push("n: Expected number".to_string()),
                Some(f) => {
                    if f.fract() != 0.0 {
                        errors.push("n: Expected integer, received float".to_string());
                    }
                    if f < 1.0 {
                        errors.push("n: Number must be greater than or equal to 1".to_string());
                    }
                    if f > 10.0 {
                        errors.push("n: Number must be less than or equal to 10".to_string());
                    }
                }
            }
        }
    }
    if let Some(v) = body.get("seed") {
        if !v.is_null() && v.as_f64().is_none() {
            errors.push("seed: Expected number".to_string());
        }
    }
    opt_num(
        body,
        "frequency_penalty",
        Some(-2.0),
        Some(2.0),
        &mut errors,
    );
    opt_num(body, "presence_penalty", Some(-2.0), Some(2.0), &mut errors);
    if let Some(user) = body.get("user") {
        if !user.is_null() && user.as_str().is_none() {
            errors.push("user: Expected string".to_string());
        }
    }
    opt_num(body, "logprobs", Some(0.0), Some(5.0), &mut errors);
    if let Some(stream) = body.get("stream") {
        if !stream.is_null() && !stream.is_boolean() {
            errors.push("stream: Expected boolean".to_string());
        }
    }
    match body.get("stop") {
        None | Some(Value::Null) => {}
        Some(Value::String(_)) | Some(Value::Array(_)) => {}
        Some(other) => errors.push(format!(
            "stop: Expected string or array, received {}",
            type_name(other)
        )),
    }
    if let Some(echo) = body.get("echo") {
        if !echo.is_null() && !echo.is_boolean() {
            errors.push("echo: Expected boolean".to_string());
        }
    }
    if let Some(v) = body.get("best_of") {
        if !v.is_null() {
            match v.as_f64() {
                None => errors.push("best_of: Expected number".to_string()),
                Some(f) => {
                    if f.fract() != 0.0 {
                        errors.push("best_of: Expected integer, received float".to_string());
                    }
                    if f <= 0.0 {
                        errors.push("best_of: Number must be greater than 0".to_string());
                    }
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(ValidationError(errors));
    }

    match serde_json::from_value::<crate::types::CompletionRequest>(body.clone()) {
        Ok(mut req) => {
            if req.max_tokens.is_none() {
                req.max_tokens = Some(16); // zod default
            }
            if req.n.is_none() {
                req.n = Some(1);
            }
            if req.echo.is_none() {
                req.echo = Some(false);
            }
            Ok(req)
        }
        Err(e) => Err(ValidationError(vec![e.to_string()])),
    }
}

pub fn malformed_json() -> Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({
            "error": { "message": "Malformed JSON payload", "type": "invalid_request_error" }
        })),
    )
        .into_response()
}

// ---- Auth ----

/// Constant-time comparison for authentication tokens.
pub fn timing_safe_string_equal(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let match_len = ab.len() == bb.len();
    // Constant-time compare over equal lengths; on length mismatch, compare
    // against itself to burn the same amount of CPU before returning false.
    let compare_to: &[u8] = if match_len { bb } else { ab };
    let mut diff = 0u8;
    for i in 0..ab.len() {
        diff |= ab[i] ^ compare_to[i];
    }
    diff == 0 && match_len
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    // header.replace(/^Bearer\s+/i, '')
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^Bearer\s+").unwrap());
    let token = RE.replace(auth, "").to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// `apiKeyAuth` — returns Ok(()) or a 401 JSON response.
pub async fn api_key_auth(headers: &HeaderMap) -> Result<(), Response> {
    let Some(token) = bearer_token(headers) else {
        tracing::info!("[Auth] Missing Authorization header");
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": {
                    "message": "Missing Authorization header. Include \"Authorization: Bearer <your-api-key>\"",
                    "type": "authentication_error"
                }
            })),
        )
            .into_response());
    };

    let unified_key = get_unified_api_key().await;
    if !timing_safe_string_equal(&token, &unified_key) {
        tracing::info!(
            "[Auth] Invalid API key provided (length: {} expected: {})",
            token.len(),
            unified_key.len()
        );
        return Err((
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": {
                    "message": "Invalid API key. Get your key from the dashboard Settings page.",
                    "type": "authentication_error"
                }
            })),
        )
            .into_response());
    }
    Ok(())
}

/// Authentication middleware for dashboard/admin routes. This credential is
/// deliberately separate from the unified proxy key because it grants access
/// to provider credentials, configuration, and key regeneration.
pub async fn admin_auth(request: Request, next: Next) -> Response {
    if let Err(response) = admin_api_key_auth(request.headers()).await {
        return response;
    }
    next.run(request).await
}

/// Returns Ok(()) or a generic 401/503 response without disclosing the admin
/// credential or whether a supplied token was close to valid.
pub async fn admin_api_key_auth(headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = crate::env::env_string("ADMIN_API_KEY")
        .filter(|key| crate::env::is_valid_admin_api_key(key))
    else {
        tracing::error!("ADMIN_API_KEY is not configured");
        return Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "error": {
                    "message": "Admin API authentication is not configured",
                    "type": "configuration_error"
                }
            })),
        )
            .into_response());
    };

    let Some(token) = bearer_token(headers) else {
        return Err(admin_authentication_error());
    };
    if !timing_safe_string_equal(&token, expected.trim()) {
        tracing::info!("[AdminAuth] Invalid admin credential provided");
        return Err(admin_authentication_error());
    }
    Ok(())
}

pub async fn unified_api_auth(request: Request, next: Next) -> Response {
    if let Err(response) = api_key_auth(request.headers()).await {
        return response;
    }
    next.run(request).await
}

fn admin_authentication_error() -> Response {
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "error": {
                "message": "Invalid or missing admin Authorization header",
                "type": "authentication_error"
            }
        })),
    )
        .into_response()
}

// ---- normalizeContent / normalizeMessages ----

fn normalize_content(content: &Value) -> MessageContent {
    match content {
        Value::String(s) => MessageContent::Text(s.clone()),
        Value::Array(parts) => {
            let has_images = parts.iter().any(|p| {
                let t = p.get("type").and_then(|x| x.as_str());
                t == Some("image_url") || t == Some("image")
            });
            if has_images {
                let parsed: Vec<ContentPart> = parts
                    .iter()
                    .filter_map(|p| serde_json::from_value(p.clone()).ok())
                    .collect();
                return MessageContent::Parts(parsed);
            }
            let text: Vec<String> = parts
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|x| x.as_str()) == Some("text") {
                        p.get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            let joined = text.join(" ");
            if joined.is_empty() {
                MessageContent::Null(())
            } else {
                MessageContent::Text(joined)
            }
        }
        _ => MessageContent::Null(()),
    }
}

/// `normalizeMessages(messages)` — takes the RAW parsed JSON messages array
/// and returns normalized chat messages.
pub fn normalize_messages(raw: &Value) -> Vec<ChatMessage> {
    let list: Vec<Value> = raw.as_array().cloned().unwrap_or_default();
    list.into_iter()
        .map(|m| {
            let role = m
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string();
            let content = normalize_content(m.get("content").unwrap_or(&Value::Null));
            let name = m
                .get("name")
                .and_then(|n| n.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let mut tool_calls = None;
            if role == "assistant" {
                if let Some(tcs) = m.get("tool_calls").and_then(|t| t.as_array()) {
                    let parsed: Vec<crate::types::ChatToolCall> = tcs
                        .iter()
                        .map(|tc| {
                            let mut call: crate::types::ChatToolCall =
                                serde_json::from_value(tc.clone()).unwrap_or_default();
                            call.r#type = "function".to_string();
                            call
                        })
                        .collect();
                    tool_calls = Some(parsed);
                }
            }
            let tool_call_id = if role == "tool" {
                m.get("tool_call_id")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            };
            ChatMessage {
                role,
                content,
                name,
                tool_call_id,
                tool_calls,
                extra: serde_json::Map::new(),
            }
        })
        .collect()
}

/// `estimateInputTokens(messages)` — chars/4 heuristic over text parts.
pub fn estimate_input_tokens(messages: &[ChatMessage]) -> i64 {
    messages
        .iter()
        .map(|m| match &m.content {
            MessageContent::Text(s) => ((s.chars().count() as f64) / 4.0).ceil() as i64,
            MessageContent::Parts(parts) => {
                let text: Vec<String> = parts
                    .iter()
                    .filter_map(|p| {
                        if p.r#type == "text" {
                            p.text.clone()
                        } else {
                            None
                        }
                    })
                    .collect();
                let joined = text.join(" ");
                ((joined.chars().count() as f64) / 4.0).ceil() as i64
            }
            MessageContent::Null(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_safe_equal() {
        assert!(timing_safe_string_equal("abc123", "abc123"));
        assert!(!timing_safe_string_equal("abc123", "abc124"));
        assert!(!timing_safe_string_equal("abc123", "abc1234"));
        assert!(!timing_safe_string_equal("", "x"));
    }

    #[test]
    fn estimate_tokens_matches_char_div_4() {
        let msgs = vec![ChatMessage::text("user", "hello world this is text")];
        assert_eq!(estimate_input_tokens(&msgs), (24.0f64 / 4.0).ceil() as i64);
    }

    #[test]
    fn normalize_joins_text_parts() {
        let raw = serde_json::json!([
            { "role": "user", "content": [ { "type": "text", "text": "a" }, { "type": "text", "text": "b" } ] }
        ]);
        let msgs = normalize_messages(&raw);
        assert_eq!(msgs[0].content.as_text().unwrap(), "a b");
    }

    #[test]
    fn normalize_keeps_image_parts() {
        let raw = serde_json::json!([
            { "role": "user", "content": [ { "type": "text", "text": "a" }, { "type": "image_url", "image_url": { "url": "http://x/y.png" } } ] }
        ]);
        let msgs = normalize_messages(&raw);
        assert!(matches!(msgs[0].content, MessageContent::Parts(_)));
    }

    #[test]
    fn assistant_without_content_or_tools_fails() {
        let body = serde_json::json!({
            "messages": [{ "role": "assistant", "content": "" }]
        });
        let err = validate_chat_body_value(&body).unwrap_err();
        assert!(err
            .message()
            .contains("assistant messages must include non-empty content or tool_calls"));
    }

    #[test]
    fn valid_body_defaults_n_to_1() {
        let body = serde_json::json!({
            "messages": [{ "role": "user", "content": "hi" }]
        });
        let req = validate_chat_body_value(&body).unwrap();
        assert_eq!(req.n, Some(1));
        assert_eq!(req.messages.len(), 1);
    }

    // ── validation behavior tests ──

    #[test]
    fn timing_safe_string_equal_cases() {
        assert!(timing_safe_string_equal("abc", "abc"));
        assert!(!timing_safe_string_equal("abc", "xyz"));
        assert!(!timing_safe_string_equal("abc", "abcdef"));
        assert!(!timing_safe_string_equal("", "abc"));
        assert!(timing_safe_string_equal("", ""));
    }

    #[test]
    fn normalize_basic_user_message() {
        let raw = serde_json::json!([{ "role": "user", "content": "Hello" }]);
        let result = normalize_messages(&raw);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content.as_text(), Some("Hello"));
    }

    #[test]
    fn normalize_preserves_system_messages() {
        let raw = serde_json::json!([{ "role": "system", "content": "You are a bot." }]);
        let result = normalize_messages(&raw);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[0].content.as_text(), Some("You are a bot."));
    }

    #[test]
    fn normalize_null_content_assistant() {
        let raw = serde_json::json!([{ "role": "assistant", "content": null }]);
        let result = normalize_messages(&raw);
        assert!(result[0].content.is_null());
    }

    #[test]
    fn normalize_preserves_assistant_tool_calls() {
        let raw = serde_json::json!([{
            "role": "assistant",
            "content": "Calling tool...",
            "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "get_weather", "arguments": "{}" } }],
        }]);
        let result = normalize_messages(&raw);
        let tcs = result[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "call_1");
    }

    #[test]
    fn normalize_preserves_tool_call_id() {
        let raw =
            serde_json::json!([{ "role": "tool", "content": "Sunny", "tool_call_id": "call_1" }]);
        let result = normalize_messages(&raw);
        assert_eq!(result[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn normalize_preserves_name_field() {
        let raw = serde_json::json!([{ "role": "user", "content": "Hi", "name": "Alice" }]);
        let result = normalize_messages(&raw);
        assert_eq!(result[0].name.as_deref(), Some("Alice"));
    }

    #[test]
    fn normalize_mixed_conversation() {
        let raw = serde_json::json!([
            { "role": "system", "content": "You are helpful." },
            { "role": "user", "content": "Hello" },
            { "role": "assistant", "content": "Hi there", "tool_calls": [] },
            { "role": "user", "content": "How are you?" },
        ]);
        let result = normalize_messages(&raw);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn estimate_tokens_cases() {
        assert_eq!(estimate_input_tokens(&[]), 0);
        assert_eq!(
            estimate_input_tokens(&[ChatMessage::text("user", "Hello")]),
            2
        );
        assert_eq!(
            estimate_input_tokens(&[
                ChatMessage::text("user", "Hello"),
                ChatMessage::text("assistant", "World"),
            ]),
            4
        );
        assert_eq!(
            estimate_input_tokens(&[ChatMessage {
                role: "assistant".into(),
                content: MessageContent::Null(()),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                extra: serde_json::Map::new(),
            }]),
            0
        );
        assert_eq!(
            estimate_input_tokens(&[ChatMessage::text("user", &"A".repeat(100))]),
            25
        );
    }

    #[test]
    fn stream_options_validation_accepted() {
        for so in [
            serde_json::json!({ "include_usage": true }),
            serde_json::json!({ "include_usage": false }),
            serde_json::json!({}),
        ] {
            let body = serde_json::json!({
                "messages": [{ "role": "user", "content": "Hello" }],
                "stream": true,
                "stream_options": so,
            });
            let result = validate_chat_body_value(&body);
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(
                result.unwrap().stream_options.and_then(|o| o.include_usage),
                match so.get("include_usage") {
                    Some(v) => v.as_bool(),
                    None => None,
                }
            );
        }
    }
}
