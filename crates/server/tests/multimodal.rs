//! Multimodal request handling on `POST /v1/chat/completions`: content
//! validation, mixed-content preservation and the `/v1` body limit.

mod common;

use axum::http::StatusCode;
use serde_json::json;

const PNG_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[tokio::test]
async fn multimodal_only_image_message_passes_validation() {
    let app = common::setup().await;
    let body = json!({
        "model": "auto",
        "messages": [{
            "role": "user",
            "content": [{ "type": "image_url", "image_url": { "url": PNG_DATA_URL } }]
        }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    // Not a validation failure; routing may still fail without provider keys.
    assert_ne!(res.status, StatusCode::BAD_REQUEST.as_u16(), "{}", res.text);
    assert_ne!(res.status, StatusCode::PAYLOAD_TOO_LARGE.as_u16());
}

#[tokio::test]
async fn malformed_image_part_is_rejected_not_silently_dropped() {
    let app = common::setup().await;
    let body = json!({
        "model": "auto",
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "describe" },
                { "type": "image_url", "image_url": "https://example.test/cat.png" }
            ]
        }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    let message = res.body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("image_url"),
        "unexpected message: {message}"
    );
}

#[tokio::test]
async fn mixed_text_image_audio_message_passes_validation() {
    let app = common::setup().await;
    let body = json!({
        "model": "auto",
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "compare these" },
                { "type": "image_url", "image_url": { "url": PNG_DATA_URL } },
                { "type": "input_audio", "input_audio": { "data": "AAAA", "format": "wav" } }
            ]
        }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    assert_ne!(res.status, StatusCode::BAD_REQUEST.as_u16(), "{}", res.text);
    assert_ne!(res.status, StatusCode::PAYLOAD_TOO_LARGE.as_u16());
}

#[tokio::test]
async fn v1_accepts_bodies_above_the_old_one_megabyte_limit() {
    let app = common::setup().await;
    // ~2 MB of text: above the old global limit and above axum's default
    // 2 MB extractor limit, but below the 20 MB `/v1` default.
    let filler = "a".repeat(2 * 1024 * 1024 + 1024);
    let body = json!({
        "model": "auto",
        "messages": [{ "role": "user", "content": filler }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    assert_ne!(
        res.status,
        StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
        "{}",
        res.text
    );
}

#[tokio::test]
async fn v1_rejects_bodies_above_the_configured_limit() {
    let app = common::setup().await;
    // Default `/v1` limit is 20 MiB.
    let filler = "a".repeat(21 * 1024 * 1024);
    let body = json!({
        "model": "auto",
        "messages": [{ "role": "user", "content": filler }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    common::expect_status(&res, StatusCode::PAYLOAD_TOO_LARGE);
}
