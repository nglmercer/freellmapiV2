//! Unified-key and API-key authentication integration tests.

mod common;

use axum::http::{header, HeaderMap, HeaderValue};

fn auth_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).expect("valid header value"),
    );
    headers
}

/// The generated unified key is 48 lowercase hexadecimal characters.
#[tokio::test]
async fn default_key_is_48_hex_chars() {
    let _app = common::setup().await;
    let key = server::db::get_unified_api_key().await;
    assert_eq!(key.len(), 48);
    assert!(
        key.chars().all(|c| c.is_ascii_hexdigit()),
        "key must match /^[a-f0-9]{{48}}$/: {key}"
    );
}

/// Repeated calls return the same (existing) key.
#[tokio::test]
async fn existing_key_is_stable() {
    let _app = common::setup().await;
    let key = server::db::get_unified_api_key().await;
    let second_key = server::db::get_unified_api_key().await;
    assert_eq!(key, second_key);
}

/// apiKeyAuth: no Authorization header → 401 with missing-header message.
#[tokio::test]
async fn middleware_rejects_missing_header() {
    let _app = common::setup().await;
    let headers = HeaderMap::new();
    let err = server::routes::middleware::api_key_auth(&headers)
        .await
        .unwrap_err();
    assert_eq!(err.status().as_u16(), 401);
    let v: serde_json::Value = read_body(err).await;
    assert_eq!(v["error"]["type"], "authentication_error");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Missing Authorization header"));
}

/// apiKeyAuth: wrong token → 401 with invalid-key message.
#[tokio::test]
async fn middleware_rejects_wrong_token() {
    let _app = common::setup().await;
    let headers = auth_headers("wrong-key");
    let err = server::routes::middleware::api_key_auth(&headers)
        .await
        .unwrap_err();
    assert_eq!(err.status().as_u16(), 401);
    let v: serde_json::Value = read_body(err).await;
    assert_eq!(v["error"]["type"], "authentication_error");
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid API key"));
}

/// apiKeyAuth: correct unified key → Ok (pipeline proceeds to next()).
#[tokio::test]
async fn middleware_accepts_unified_key() {
    let app = common::setup().await;
    let headers = auth_headers(&app.api_key);
    let result = server::routes::middleware::api_key_auth(&headers).await;
    assert!(result.is_ok(), "authorized token must pass apiKeyAuth");
}

async fn read_body(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}
