//! Legacy `POST /v1/completions` integration tests plus HTTP-visible
//! validation cases.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use server::routes::middleware::{validate_chat_body_value, validate_completion_body_value};
use server::types::{PromptField, StopField};
use tower::ServiceExt;

/// POST a raw body (for empty / malformed payloads).
async fn raw_post(
    app: &axum::Router,
    path: &str,
    key: Option<&str>,
    body: &str,
) -> common::TestResponse {
    let mut builder = Request::builder().method("POST").uri(path);
    if let Some(k) = key {
        builder = builder.header("authorization", format!("Bearer {k}"));
    }
    builder = builder.header("content-type", "application/json");
    let req = builder.body(Body::from(body.to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    common::TestResponse {
        status,
        body,
        headers,
        text,
    }
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/completions — authentication
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn completions_401_without_auth_header() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/completions",
        None,
        &json!({ "model": "auto", "prompt": "Hello world" }).to_string(),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(res.body.get("error").is_some());
    assert!(res.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Missing Authorization header"));
    assert_eq!(res.body["error"]["type"], "authentication_error");
}

#[tokio::test]
async fn completions_401_with_wrong_key() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/completions",
        Some("wrong-key"),
        &json!({ "model": "auto", "prompt": "Hello world" }).to_string(),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(res.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid API key"));
    assert_eq!(res.body["error"]["type"], "authentication_error");
}

#[tokio::test]
async fn completions_allows_correct_unified_key() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto", "prompt": "Hello world", "max_tokens": 10 }),
    )
    .await;
    assert_ne!(res.status, 401, "auth must succeed: {}", res.text);
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/completions — request validation
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn completions_400_for_empty_body() {
    let app = common::setup().await;
    let res = raw_post(&app.app, "/v1/completions", Some(&app.api_key), "").await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn completions_400_for_malformed_json() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/completions",
        Some(&app.api_key),
        "not json at all",
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn completions_400_when_model_missing() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "prompt": "Hello" }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn completions_400_when_prompt_missing() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto" }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn completions_accepts_a_string_prompt() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto", "prompt": "Say hello", "max_tokens": 10 }),
    )
    .await;
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
}

#[tokio::test]
async fn completions_accepts_an_array_of_prompts() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto", "prompt": ["Hello", "World"], "max_tokens": 10 }),
    )
    .await;
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
}

fn completion_body(
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    top_p: Option<f64>,
    n: Option<i64>,
    extra: Value,
) -> Value {
    let mut body = json!({ "model": "auto", "prompt": "Hello" });
    if let Some(t) = temperature {
        body["temperature"] = json!(t);
    }
    if let Some(m) = max_tokens {
        body["max_tokens"] = json!(m);
    }
    if let Some(p) = top_p {
        body["top_p"] = json!(p);
    }
    if let Some(nv) = n {
        body["n"] = json!(nv);
    }
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            body[k] = v.clone();
        }
    }
    body
}

#[tokio::test]
async fn completions_400_for_negative_temperature() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(Some(-1.0), None, None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_400_for_temperature_above_2() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(Some(3.0), None, None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_400_for_non_positive_max_tokens() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(None, Some(0), None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_400_when_max_tokens_exceeds_4000() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(None, Some(5000), None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_400_for_top_p_out_of_range() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(None, None, Some(1.5), None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_400_when_n_exceeds_10() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        completion_body(None, None, None, Some(11), json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn completions_accepts_valid_request_with_optional_fields() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "prompt": "Continue: Once upon a time",
            "suffix": "The end.",
            "max_tokens": 50,
            "temperature": 0.7,
            "top_p": 0.9,
            "n": 2,
            "echo": true,
            "best_of": 2,
            "stop": ["\n"],
        }),
    )
    .await;
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn completions_accepts_stream_options_with_include_usage() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "prompt": "Hello",
            "max_tokens": 10,
            "stream": true,
            "stream_options": { "include_usage": true },
        }),
    )
    .await;
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/completions — handler (no API keys)
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn completions_handler_errors_with_no_provider_keys() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto", "prompt": "Hello world", "max_tokens": 10 }),
    )
    .await;
    // routeRequest() with zero keys returns an error that maps to 429 here.
    assert!(
        [429, 502, 503, 500].contains(&res.status),
        "expected a routed error status, got {}: {}",
        res.status,
        res.text
    );
    assert!(res.body.get("error").is_some());
}

#[tokio::test]
async fn completions_handler_returns_structured_error_object() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({ "model": "auto", "prompt": ["Hello", "World"], "max_tokens": 10, "n": 2 }),
    )
    .await;
    assert!(res.body["error"].is_object(), "error object: {}", res.text);
    assert!(res.body["error"]["message"].is_string());
    assert!(res.body["error"]["type"].is_string());
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/chat/completions — n > 1
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_n_1_accepted() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }], "n": 1 }),
    )
    .await;
    assert_ne!(res.status, 400, "n=1 must pass validation: {}", res.text);
}

#[tokio::test]
async fn chat_n_between_2_and_10_accepted() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }], "n": 5 }),
    )
    .await;
    assert_ne!(res.status, 400, "n=5 must pass validation: {}", res.text);
}

#[tokio::test]
async fn chat_n_zero_rejected() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }], "n": 0 }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_n_above_10_rejected() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }], "n": 11 }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_n_negative_rejected() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }], "n": -1 }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_n_greater_than_1_errors_with_no_keys() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Hello" }],
            "n": 3,
        }),
    )
    .await;
    assert!(
        [429, 502, 503, 500].contains(&res.status),
        "expected a routed error status, got {}: {}",
        res.status,
        res.text
    );
    assert!(res.body.get("error").is_some());
}

// ─────────────────────────────────────────────────────────────────────
// completionSchema unit tests (Rust port of validate_completion_body_value)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn completion_schema_parses_minimal_valid_request() {
    let result = validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
    }))
    .expect("valid request");
    // A text prompt resolves to a single-element message array.
    match &result.prompt {
        PromptField::Text(s) => assert_eq!(s, "Hello"),
        PromptField::List(l) => assert_eq!(l, &["Hello"]),
    }
    assert_eq!(result.max_tokens, Some(16));
    assert_eq!(result.n, Some(1));
    assert_eq!(result.echo, Some(false));
}

#[test]
fn completion_schema_transforms_single_string_prompt() {
    let result = validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Single prompt",
    }))
    .expect("valid request");
    match &result.prompt {
        PromptField::Text(s) => assert_eq!(s, "Single prompt"),
        PromptField::List(l) => assert_eq!(l, &["Single prompt"]),
    }
}

#[test]
fn completion_schema_keeps_array_prompt() {
    let result = validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": ["First", "Second", "Third"],
    }))
    .expect("valid request");
    match &result.prompt {
        PromptField::Text(s) => panic!("expected list prompt, got text {s}"),
        PromptField::List(l) => assert_eq!(l, &["First", "Second", "Third"]),
    }
}

#[test]
fn completion_schema_transforms_string_stop() {
    let result = validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
        "stop": "\n",
    }))
    .expect("valid request");
    match &result.stop {
        Some(StopField::Text(s)) => assert_eq!(s, "\n"),
        Some(StopField::List(l)) => assert_eq!(l, &["\n"]),
        None => panic!("stop must be preserved"),
    }
}

#[test]
fn completion_schema_defaults_optional_fields() {
    let result = validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
    }))
    .expect("valid request");
    assert_eq!(result.max_tokens, Some(16));
    assert_eq!(result.n, Some(1));
    assert_eq!(result.echo, Some(false));
    assert_eq!(result.stream, None);
    assert_eq!(result.temperature, None);
    assert_eq!(result.top_p, None);
    assert_eq!(result.suffix, None);
    assert!(result.stop.is_none());
}

#[test]
fn completion_schema_rejects_max_tokens_zero() {
    assert!(validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
        "max_tokens": 0,
    }))
    .is_err());
}

#[test]
fn completion_schema_rejects_max_tokens_over_4000() {
    assert!(validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
        "max_tokens": 5000,
    }))
    .is_err());
}

#[test]
fn completion_schema_rejects_n_zero() {
    assert!(validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
        "n": 0,
    }))
    .is_err());
}

#[test]
fn completion_schema_rejects_n_over_10() {
    assert!(validate_completion_body_value(&json!({
        "model": "gpt-3.5-turbo",
        "prompt": "Hello",
        "n": 11,
    }))
    .is_err());
}

// ─────────────────────────────────────────────────────────────────────
// chatCompletionSchema — n field unit tests
// ─────────────────────────────────────────────────────────────────────

#[test]
fn chat_schema_defaults_n_to_1() {
    let result = validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
    }))
    .expect("valid request");
    assert_eq!(result.n, Some(1));
}

#[test]
fn chat_schema_accepts_n_1_explicit() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "n": 1,
    }))
    .is_ok());
}

#[test]
fn chat_schema_accepts_n_10_max() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "n": 10,
    }))
    .is_ok());
}

#[test]
fn chat_schema_rejects_n_zero() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "n": 0,
    }))
    .is_err());
}

#[test]
fn chat_schema_rejects_n_11() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "n": 11,
    }))
    .is_err());
}

#[test]
fn chat_schema_rejects_n_negative() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "n": -1,
    }))
    .is_err());
}

// ─────────────────────────────────────────────────────────────────────
// chatCompletionSchema — new params
// ─────────────────────────────────────────────────────────────────────

#[test]
fn chat_schema_accepts_seed() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "seed": 42,
    }))
    .is_ok());
}

#[test]
fn chat_schema_accepts_frequency_penalty_in_range() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "frequency_penalty": 0.5,
    }))
    .is_ok());
}

#[test]
fn chat_schema_rejects_frequency_penalty_outside_range() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "frequency_penalty": 3,
    }))
    .is_err());
}

#[test]
fn chat_schema_accepts_presence_penalty_in_range() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "presence_penalty": -1,
    }))
    .is_ok());
}

#[test]
fn chat_schema_rejects_presence_penalty_outside_range() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "presence_penalty": -3,
    }))
    .is_err());
}

#[test]
fn chat_schema_accepts_user() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "user": "end-user-123",
    }))
    .is_ok());
}

#[test]
fn chat_schema_accepts_response_format_json_object() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "response_format": { "type": "json_object" },
    }))
    .is_ok());
}

#[test]
fn chat_schema_accepts_response_format_json_schema() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "response_format": { "type": "json_schema", "json_schema": { "name": "test" } },
    }))
    .is_ok());
}

#[test]
fn chat_schema_rejects_response_format_invalid_type() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "response_format": { "type": "invalid" },
    }))
    .is_err());
}

#[test]
fn chat_schema_accepts_logprobs_boolean() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "logprobs": true,
    }))
    .is_ok());
}

#[test]
fn chat_schema_accepts_top_logprobs_0_5() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "top_logprobs": 3,
    }))
    .is_ok());
}

#[test]
fn chat_schema_rejects_top_logprobs_over_5() {
    assert!(validate_chat_body_value(&json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "top_logprobs": 6,
    }))
    .is_err());
}

// ─────────────────────────────────────────────────────────────────────
// completionSchema — new params
// ─────────────────────────────────────────────────────────────────────

#[test]
fn completion_schema_accepts_seed() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "seed": 42,
    }))
    .is_ok());
}

#[test]
fn completion_schema_accepts_frequency_penalty() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "frequency_penalty": 0.8,
    }))
    .is_ok());
}

#[test]
fn completion_schema_accepts_presence_penalty() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "presence_penalty": 0.3,
    }))
    .is_ok());
}

#[test]
fn completion_schema_accepts_user() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "user": "test-user",
    }))
    .is_ok());
}

#[test]
fn completion_schema_accepts_logprobs_0_5() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "logprobs": 3,
    }))
    .is_ok());
}

#[test]
fn completion_schema_rejects_logprobs_over_5() {
    assert!(validate_completion_body_value(&json!({
        "model": "auto",
        "prompt": "Hello",
        "logprobs": 6,
    }))
    .is_err());
}

// ─────────────────────────────────────────────────────────────────────
// /v1/chat/completions — new params accepted through the HTTP route
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_route_accepts_seed_and_penalties() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Hi" }],
            "seed": 42,
            "frequency_penalty": 0.5,
            "presence_penalty": -0.5,
            "user": "end-user-1",
        }),
    )
    .await;
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn chat_route_accepts_response_format_json_object() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Output JSON" }],
            "response_format": { "type": "json_object" },
        }),
    )
    .await;
    assert_ne!(res.status, 400);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn chat_route_accepts_logprobs_and_top_logprobs() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Hi" }],
            "logprobs": true,
            "top_logprobs": 3,
        }),
    )
    .await;
    assert_ne!(res.status, 400);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn completions_route_accepts_seed_and_logprobs() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "prompt": "Hello",
            "seed": 42,
            "frequency_penalty": 0.5,
            "presence_penalty": -0.3,
            "user": "test-user",
            "logprobs": 3,
            "max_tokens": 50,
        }),
    )
    .await;
    assert_ne!(res.status, 400);
    assert_ne!(res.status, 401);
}
