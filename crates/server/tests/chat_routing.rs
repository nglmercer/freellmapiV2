//! Chat routing rules: no silent substitution of a requested model, and
//! automatic fallback past an out-of-credits (402) candidate.

mod common;

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};

/// Chat upstream: models whose name contains "paid" answer 402; others answer.
async fn mock_chat(body: Bytes) -> axum::response::Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let model = req["model"].as_str().unwrap_or_default().to_string();
    if model.contains("paid") {
        return (
            StatusCode::PAYMENT_REQUIRED,
            axum::Json(
                json!({"error": {"message": "Add credits to continue, or switch to a free model"}}),
            ),
        )
            .into_response();
    }
    axum::Json(json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": format!("answer from {model}")},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }))
    .into_response()
}

async fn start_mock() -> String {
    let app = Router::new().route("/v1/chat/completions", post(mock_chat));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

/// Creates a custom chat provider with one key and returns its platform id.
async fn add_provider(base_url: &str) -> String {
    let (encrypted, iv, tag) = server::crypto::encrypt("chat-key");
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO custom_providers (name, base_url, enabled, capabilities) VALUES ('chat-mock', ?1, 1, '[\"chat\"]')",
        rusqlite::params![base_url],
    )
    .unwrap();
    let platform = format!("custom:{}", conn.last_insert_rowid());
    conn.execute(
        "INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled) \
         VALUES (?1, 'test', ?2, ?3, ?4, 'unknown', 1)",
        rusqlite::params![platform, encrypted, iv, tag],
    )
    .unwrap();
    platform
}

/// Adds an enabled chat model to the fallback chain at `priority`.
async fn add_model(platform: &str, model_id: &str, priority: i64) {
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO models (platform, model_id, display_name, intelligence_rank, speed_rank, context_window, enabled) \
         VALUES (?1, ?2, ?2, 50, 50, 100000, 1)",
        rusqlite::params![platform, model_id],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO fallback_config (model_db_id, priority, manual_priority, enabled) VALUES (?1, ?2, ?2, 1)",
        rusqlite::params![id, priority],
    )
    .unwrap();
}

async fn chat(app: &axum::Router, key: &str, model: &str) -> common::TestResponse {
    common::post_json(
        app,
        "/v1/chat/completions",
        key,
        json!({"model": model, "messages": [{"role": "user", "content": "hi"}]}),
    )
    .await
}

#[tokio::test]
async fn unknown_requested_model_is_not_silently_replaced() {
    let app = common::setup().await;
    let res = chat(&app.app, &app.api_key, "routing-no-such-model").await;
    common::expect_status(&res, StatusCode::NOT_FOUND);
    assert!(res.text.contains("is not available"), "{}", res.text);
}

#[tokio::test]
async fn requested_model_that_runs_out_of_credits_is_not_substituted() {
    let app = common::setup().await;
    let base = start_mock().await;
    let platform = add_provider(&base).await;
    add_model(&platform, "gate-paid-model", 0).await;
    add_model(&platform, "gate-free-model", 1).await;

    let res = chat(&app.app, &app.api_key, "gate-paid-model").await;
    // The free model would answer, but the caller asked for this one.
    assert_ne!(res.status, StatusCode::OK.as_u16(), "{}", res.text);
    assert!(
        res.text.contains("402") || res.text.contains("Add credits"),
        "{}",
        res.text
    );
}

#[tokio::test]
async fn auto_routing_falls_back_past_out_of_credits_candidate() {
    let app = common::setup().await;
    let base = start_mock().await;
    let platform = add_provider(&base).await;
    add_model(&platform, "auto-paid-model", 0).await;
    add_model(&platform, "auto-free-model", 1).await;

    let res = chat(&app.app, &app.api_key, "auto").await;
    common::expect_status(&res, StatusCode::OK);
    let content = res.body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default();
    assert!(content.contains("answer from"), "{content}");
}
