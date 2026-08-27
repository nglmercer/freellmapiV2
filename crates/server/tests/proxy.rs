//! OpenAI-compatible proxy integration tests (`GET /v1/models` and
//! `POST /v1/chat/completions`).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

/// POST /v1/chat/completions (or /v1/completions) with a RAW body string.
/// `common::request` only serializes JSON values, so tests that exercise
/// empty / malformed bodies build the request here.
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

fn chat_body(model: &str, messages: Value) -> Value {
    json!({ "model": model, "messages": messages })
}

// ─────────────────────────────────────────────────────────────────────
// GET /v1/models
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn models_returns_openai_compatible_list() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["object"], "list");
    let data = res.body["data"].as_array().expect("data array");
    assert!(!data.is_empty());
}

#[tokio::test]
async fn models_includes_virtual_auto_first() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body["data"].as_array().expect("data array");
    assert_eq!(data[0]["id"], "auto");
    assert_eq!(data[0]["owned_by"], "freellmapi");
}

#[tokio::test]
async fn models_include_required_openai_fields() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body["data"].as_array().expect("data array");
    let model = &data[1]; // index 0 is the virtual "auto" entry
    assert!(model.get("id").is_some());
    assert!(model.get("object").is_some());
    assert!(model.get("owned_by").is_some());
    assert!(model.get("name").is_some());
    assert_eq!(model["object"], "model");
}

#[tokio::test]
async fn models_require_no_authorization() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
}

#[tokio::test]
async fn models_contain_at_least_one_real_model() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body["data"].as_array().expect("data array");
    let real_models: Vec<&Value> = data
        .iter()
        .filter(|m| m["owned_by"] != "freellmapi")
        .collect();
    assert!(!real_models.is_empty(), "expected seeded real models");
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/chat/completions — authentication
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_completions_401_without_auth_header() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/chat/completions",
        None,
        &json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hello" }] })
            .to_string(),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(res.body.get("error").is_some());
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Missing Authorization header"),
        "{}",
        res.text
    );
    assert_eq!(res.body["error"]["type"], "authentication_error");
}

#[tokio::test]
async fn chat_completions_401_with_wrong_key() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/chat/completions",
        Some("wrong-key"),
        &json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hello" }] })
            .to_string(),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Invalid API key"),
        "{}",
        res.text
    );
    assert_eq!(res.body["error"]["type"], "authentication_error");
}

#[tokio::test]
async fn chat_completions_rejects_empty_token() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/chat/completions",
        Some(""),
        &json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hi" }] }).to_string(),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    // NOTE: the legacy client expected 'Invalid API key' here (fetch trims the
    // trailing space off `Authorization: 'Bearer '`, leaving a non-empty
    // token). The Rust server receives the raw value, so the empty token is
    // treated as a missing header. The compatibility intent — reject with 401 +
    // authentication_error — is preserved.
    assert_eq!(res.body["error"]["type"], "authentication_error");
    let message = res.body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Missing Authorization header") || message.contains("Invalid API key"),
        "{}",
        res.text
    );
}

#[tokio::test]
async fn chat_completions_allows_correct_unified_key() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/chat/completions",
        Some(&app.api_key),
        &json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hello" }] })
            .to_string(),
    )
    .await;
    // Must NOT be 401 — auth passes, the handler is reached.
    assert_ne!(res.status, 401, "auth must succeed: {}", res.text);
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/chat/completions — request validation
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_completions_400_for_empty_body() {
    let app = common::setup().await;
    let res = raw_post(&app.app, "/v1/chat/completions", Some(&app.api_key), "").await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn chat_completions_400_for_malformed_json() {
    let app = common::setup().await;
    let res = raw_post(
        &app.app,
        "/v1/chat/completions",
        Some(&app.api_key),
        "not json at all",
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn chat_completions_400_when_messages_missing() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto" }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
    assert_eq!(res.body["error"]["type"], "invalid_request_error");
}

#[tokio::test]
async fn chat_completions_400_when_messages_empty() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        chat_body("auto", json!([])),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

#[tokio::test]
async fn chat_completions_400_when_message_has_no_role() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "messages": [{ "content": "hi" }] }),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

fn chat_hi(
    temperature: Option<f64>,
    max_tokens: Option<i64>,
    top_p: Option<f64>,
    n: Option<i64>,
    extra: Value,
) -> Value {
    let mut body = json!({ "model": "auto", "messages": [{ "role": "user", "content": "hi" }] });
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
async fn chat_completions_400_for_negative_temperature() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        chat_hi(Some(-1.0), None, None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_completions_400_for_temperature_above_2() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        chat_hi(Some(3.0), None, None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_completions_400_for_non_positive_max_tokens() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        chat_hi(None, Some(0), None, None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_completions_400_for_top_p_out_of_range() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        chat_hi(None, None, Some(1.5), None, json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_completions_accepts_valid_user_message() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "Hello world" }] }),
    )
    .await;
    // Validation passes — the handler is invoked.
    assert_ne!(res.status, 400, "validation must pass: {}", res.text);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn chat_completions_accepts_system_and_user_pair() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [
                { "role": "system", "content": "You are a helpful assistant." },
                { "role": "user", "content": "Say hi" },
            ],
        }),
    )
    .await;
    assert_ne!(res.status, 400);
    assert_ne!(res.status, 401);
}

#[tokio::test]
async fn chat_completions_accepts_assistant_message_with_content() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": "auto",
            "messages": [
                { "role": "user", "content": "Hi" },
                { "role": "assistant", "content": "Hello there" },
                { "role": "user", "content": "How are you?" },
            ],
        }),
    )
    .await;
    assert_ne!(res.status, 400);
    assert_ne!(res.status, 401);
}

// ─────────────────────────────────────────────────────────────────────
// POST /v1/chat/completions — handler results (fresh in-memory DB)
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn chat_completions_errors_when_no_provider_keys() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "test" }] }),
    )
    .await;
    // Not 400/401 — those are auth/validation failures. With zero key rows
    // routeRequest() reports "No API keys configured" as a 503 routing error.
    assert!(
        [429, 502, 503, 500].contains(&res.status),
        "expected a routed error status, got {}: {}",
        res.status,
        res.text
    );
    assert!(res.body.get("error").is_some(), "must have an error object");
}

#[tokio::test]
async fn chat_completions_returns_structured_error_object() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({ "model": "auto", "messages": [{ "role": "user", "content": "test" }] }),
    )
    .await;
    assert!(res.body["error"].is_object(), "error object: {}", res.text);
    assert!(res.body["error"]["message"].is_string());
    assert!(res.body["error"]["type"].is_string());
}
