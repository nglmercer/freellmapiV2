//! Audio endpoints against a real mock upstream on a local port: candidate
//! selection, multipart forwarding, streamed speech, fallback, and the admin
//! declaration API.

mod common;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone, Copy)]
struct Mock {
    /// Answer every speech call with 429 (to exercise fallback).
    rate_limited: bool,
}

async fn mock_speech(
    State(mock): State<Mock>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if mock.rate_limited {
        return (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
    }
    if req["input"] == "bad" {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": {"message": "unsupported response_format"}})),
        )
            .into_response();
    }
    (
        [(axum::http::header::CONTENT_TYPE, "audio/mpeg")],
        b"FAKE-MP3-BYTES".to_vec(),
    )
        .into_response()
}

async fn mock_multipart(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = String::from_utf8_lossy(&body).to_string();
    let has_model = text.contains("name=\"model\"");
    let has_file = text.contains("name=\"file\"; filename=\"clip.wav\"");
    axum::Json(
        json!({ "text": "hello", "auth": auth, "has_model": has_model, "has_file": has_file }),
    )
    .into_response()
}

async fn mock_image_generate(
    State(mock): State<Mock>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if mock.rate_limited {
        return (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
    }
    axum::Json(json!({ "created": 1, "data": [{ "b64_json": "AAAA", "model": req["model"] }] }))
        .into_response()
}

async fn mock_image_edit(body: axum::body::Bytes) -> axum::response::Response {
    let text = String::from_utf8_lossy(&body).to_string();
    axum::Json(json!({
        "created": 1,
        "data": [{ "url": "https://example.test/edited.png" }],
        "has_image": text.contains("name=\"image\""),
        "has_mask": text.contains("name=\"mask\"")
    }))
    .into_response()
}

async fn mock_embeddings(body: axum::body::Bytes) -> axum::response::Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let count = match &req["input"] {
        Value::Array(items) => items.len(),
        _ => 1,
    };
    let data: Vec<Value> = (0..count)
        .map(|i| json!({"object": "embedding", "index": i, "embedding": [0.1, 0.2]}))
        .collect();
    axum::Json(json!({"object": "list", "model": req["model"], "data": data})).into_response()
}

async fn mock_translate() -> axum::response::Response {
    axum::Json(json!({ "text": "translated" })).into_response()
}

/// Starts a mock upstream and returns its `/v1` base URL.
async fn start_mock(rate_limited: bool) -> String {
    let app = Router::new()
        .route("/v1/audio/speech", post(mock_speech))
        .route("/v1/audio/transcriptions", post(mock_multipart))
        .route("/v1/audio/translations", post(mock_translate))
        .route("/v1/images/generations", post(mock_image_generate))
        .route("/v1/images/edits", post(mock_image_edit))
        .route("/v1/embeddings", post(mock_embeddings))
        .with_state(Mock { rate_limited });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

/// Inserts a custom provider with the given capabilities and returns its platform id.
async fn add_provider(name: &str, base_url: &str, capabilities: &str) -> String {
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO custom_providers (name, base_url, enabled, capabilities) VALUES (?1, ?2, 1, ?3)",
        rusqlite::params![name, base_url, capabilities],
    )
    .unwrap();
    format!("custom:{}", conn.last_insert_rowid())
}

async fn add_key(platform: &str, key: &str) {
    let (encrypted, iv, tag) = server::crypto::encrypt(key);
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled) \
         VALUES (?1, 'test', ?2, ?3, ?4, 'unknown', 1)",
        rusqlite::params![platform, encrypted, iv, tag],
    )
    .unwrap();
}

async fn declare(platform: &str, model: &str, capability: &str, priority: i64) {
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO media_models (platform, model_id, capability, priority, enabled) \
         VALUES (?1, ?2, ?3, ?4, 1)",
        rusqlite::params![platform, model, capability, priority],
    )
    .unwrap();
}

/// Sends a request and returns (status, content-type, body bytes).
async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, String, Vec<u8>) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, bytes)
}

fn speech_request(key: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/audio/speech")
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn multipart_request(key: &str, uri: &str, model: &str) -> Request<Body> {
    let boundary = "----testboundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(b"RIFFfakeWAVEdata");
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {key}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn speech_streams_upstream_audio_with_the_declared_model() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("tts-mock", &base, r#"["chat","speech"]"#).await;
    add_key(&platform, "provider-secret").await;
    declare(&platform, "tts-1", "speech", 0).await;

    let (status, content_type, bytes) = send(
        &app.app,
        speech_request(
            &app.api_key,
            json!({"model": "tts-1", "input": "hi", "voice": "alloy"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "audio/mpeg");
    assert_eq!(bytes, b"FAKE-MP3-BYTES");
}

#[tokio::test]
async fn speech_rejects_undeclared_models_explicitly() {
    let app = common::setup().await;
    let (status, _, bytes) = send(
        &app.app,
        speech_request(
            &app.api_key,
            json!({"model": "not-declared", "input": "hi", "voice": "a"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&bytes).contains("not enabled for 'speech'"));
}

#[tokio::test]
async fn speech_validates_required_fields() {
    let app = common::setup().await;
    let (status, _, _) = send(
        &app.app,
        speech_request(&app.api_key, json!({"model": "tts-1", "voice": "a"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn upstream_client_errors_are_not_retried_or_masked() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("tts-400", &base, r#"["chat","speech"]"#).await;
    add_key(&platform, "k").await;
    declare(&platform, "tts-1", "speech", 0).await;

    let (status, _, bytes) = send(
        &app.app,
        speech_request(
            &app.api_key,
            json!({"model": "tts-1", "input": "bad", "voice": "a"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&bytes).contains("unsupported response_format"));
}

#[tokio::test]
async fn speech_falls_back_to_next_candidate_on_rate_limit() {
    let app = common::setup().await;
    let limited = start_mock(true).await;
    let healthy = start_mock(false).await;
    let first = add_provider("tts-limited", &limited, r#"["chat","speech"]"#).await;
    let second = add_provider("tts-healthy", &healthy, r#"["chat","speech"]"#).await;
    add_key(&first, "k1").await;
    add_key(&second, "k2").await;
    declare(&first, "shared-tts", "speech", 0).await;
    declare(&second, "shared-tts", "speech", 1).await;

    let (status, _, bytes) = send(
        &app.app,
        speech_request(
            &app.api_key,
            json!({"model": "shared-tts", "input": "hi", "voice": "a"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"FAKE-MP3-BYTES");
}

#[tokio::test]
async fn provider_without_speech_capability_is_rejected_at_declaration() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("chat-only", &base, r#"["chat"]"#).await;
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "POST",
        "/api/media-models",
        Some(&owner),
        Some(json!({"platform": platform, "modelId": "tts-1", "capability": "speech"})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.text.contains("does not support 'speech'"));
}

#[tokio::test]
async fn admin_api_declares_and_removes_media_models() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("admin-tts", &base, r#"["chat","speech"]"#).await;
    let owner = common::owner_credential();
    let created = common::request(
        &app.app,
        "POST",
        "/api/media-models",
        Some(&owner),
        Some(json!({"platform": platform, "modelId": "tts-1", "capability": "speech"})),
    )
    .await;
    common::expect_status(&created, StatusCode::CREATED);
    let id = created.body["id"].as_i64().unwrap();

    let listed = common::request(&app.app, "GET", "/api/media-models", Some(&owner), None).await;
    common::expect_status(&listed, StatusCode::OK);
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 1);

    let removed = common::request(
        &app.app,
        "DELETE",
        &format!("/api/media-models/{id}"),
        Some(&owner),
        None,
    )
    .await;
    common::expect_status(&removed, StatusCode::OK);
}

#[tokio::test]
async fn transcription_forwards_multipart_and_provider_key() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("stt-mock", &base, r#"["chat","transcription"]"#).await;
    add_key(&platform, "stt-provider-key").await;
    declare(&platform, "whisper-1", "transcription", 0).await;

    let (status, content_type, bytes) = send(
        &app.app,
        multipart_request(&app.api_key, "/v1/audio/transcriptions", "whisper-1"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(content_type, "application/json");
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["text"], "hello");
    assert_eq!(body["has_model"], true);
    assert_eq!(body["has_file"], true);
    // The upstream sees the provider's key, never the proxy's own API key.
    assert_eq!(body["auth"], "Bearer stt-provider-key");
}

#[tokio::test]
async fn translation_uses_the_translation_endpoint() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("tr-mock", &base, r#"["chat","translation"]"#).await;
    add_key(&platform, "k").await;
    declare(&platform, "whisper-translate", "translation", 0).await;

    let (status, _, bytes) = send(
        &app.app,
        multipart_request(&app.api_key, "/v1/audio/translations", "whisper-translate"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["text"], "translated");
}

#[tokio::test]
async fn transcription_without_usable_key_is_service_unavailable() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("stt-nokey", &base, r#"["chat","transcription"]"#).await;
    declare(&platform, "whisper-nokey", "transcription", 0).await;

    let (status, _, _) = send(
        &app.app,
        multipart_request(&app.api_key, "/v1/audio/transcriptions", "whisper-nokey"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

fn json_request(key: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn image_edit_request(key: &str, model: &str, with_mask: bool) -> Request<Body> {
    let boundary = "----imgboundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nadd a hat\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"cat.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    body.extend_from_slice(b"\x89PNG-fake");
    body.extend_from_slice(b"\r\n");
    if with_mask {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"mask\"; filename=\"mask.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
        body.extend_from_slice(b"mask-bytes\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/v1/images/edits")
        .header("authorization", format!("Bearer {key}"))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn image_generation_returns_provider_json() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("img-mock", &base, r#"["chat","image_generation"]"#).await;
    add_key(&platform, "img-key").await;
    declare(&platform, "img-model", "image_generation", 0).await;

    let (status, _, bytes) = send(
        &app.app,
        json_request(
            &app.api_key,
            "/v1/images/generations",
            json!({"model": "img-model", "prompt": "a cat"}),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"][0]["b64_json"], "AAAA");
    // Provider-specific fields such as the model are forwarded.
    assert_eq!(body["data"][0]["model"], "img-model");
}

#[tokio::test]
async fn image_generation_requires_prompt_and_declared_model() {
    let app = common::setup().await;
    let (status, _, _) = send(
        &app.app,
        json_request(
            &app.api_key,
            "/v1/images/generations",
            json!({"model": "img-model"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, bytes) = send(
        &app.app,
        json_request(
            &app.api_key,
            "/v1/images/generations",
            json!({"model": "undeclared", "prompt": "x"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&bytes).contains("'image_generation'"));
}

#[tokio::test]
async fn image_edit_forwards_all_image_and_mask_parts() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("edit-mock", &base, r#"["chat","image_edit"]"#).await;
    add_key(&platform, "edit-key").await;
    declare(&platform, "edit-model", "image_edit", 0).await;

    let (status, _, bytes) = send(
        &app.app,
        image_edit_request(&app.api_key, "edit-model", true),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["has_image"], true);
    assert_eq!(body["has_mask"], true);
}

#[tokio::test]
async fn image_generation_falls_back_on_rate_limit_only() {
    let app = common::setup().await;
    let limited = start_mock(true).await;
    let healthy = start_mock(false).await;
    let first = add_provider("img-limited", &limited, r#"["chat","image_generation"]"#).await;
    let second = add_provider("img-healthy", &healthy, r#"["chat","image_generation"]"#).await;
    add_key(&first, "k1").await;
    add_key(&second, "k2").await;
    declare(&first, "img-shared", "image_generation", 0).await;
    declare(&second, "img-shared", "image_generation", 1).await;

    let (status, _, _) = send(
        &app.app,
        json_request(
            &app.api_key,
            "/v1/images/generations",
            json!({"model": "img-shared", "prompt": "a cat"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn image_generation_never_retries_after_transport_failure() {
    let app = common::setup().await;
    // Port 9 (discard) on loopback is closed: a transport failure, where the
    // provider may already have generated the image.
    let dead = "http://127.0.0.1:9/v1";
    let healthy = start_mock(false).await;
    let first = add_provider("img-dead", dead, r#"["chat","image_generation"]"#).await;
    let second = add_provider("img-up", &healthy, r#"["chat","image_generation"]"#).await;
    add_key(&first, "k1").await;
    add_key(&second, "k2").await;
    declare(&first, "img-ambiguous", "image_generation", 0).await;
    declare(&second, "img-ambiguous", "image_generation", 1).await;

    let (status, _, _) = send(
        &app.app,
        json_request(
            &app.api_key,
            "/v1/images/generations",
            json!({"model": "img-ambiguous", "prompt": "a dog"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

fn embeddings_request(key: &str, body: Value) -> Request<Body> {
    json_request(key, "/v1/embeddings", body)
}

#[tokio::test]
async fn embeddings_batch_returns_one_vector_per_input() {
    let app = common::setup().await;
    let base = start_mock(false).await;
    let platform = add_provider("emb-mock", &base, r#"["chat","embeddings"]"#).await;
    add_key(&platform, "emb-key").await;
    declare(&platform, "embed-model", "embeddings", 0).await;

    let (status, _, bytes) = send(
        &app.app,
        embeddings_request(
            &app.api_key,
            json!({"model": "embed-model", "input": ["one", "two", "three"]}),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"][2]["index"], 2);
}

#[tokio::test]
async fn embeddings_validate_input_shape_and_batch_size() {
    let app = common::setup().await;
    for bad in [
        json!({"model": "embed-model"}),
        json!({"model": "embed-model", "input": ""}),
        json!({"model": "embed-model", "input": []}),
        json!({"model": "embed-model", "input": ["ok", ""]}),
        json!({"model": "embed-model", "input": [1, 2.5]}),
        json!({"model": "embed-model", "input": vec!["x"; 2049]}),
    ] {
        let (status, _, _) = send(&app.app, embeddings_request(&app.api_key, bad.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400 for {bad}");
    }
}

#[tokio::test]
async fn embeddings_require_a_declared_model() {
    let app = common::setup().await;
    let (status, _, bytes) = send(
        &app.app,
        embeddings_request(&app.api_key, json!({"model": "nope", "input": "hi"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&bytes).contains("'embeddings'"));
}
