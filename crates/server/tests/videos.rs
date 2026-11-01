//! Durable video jobs against a stateful mock upstream: idempotent creation,
//! refresh, download, cancellation, expiry, and recovery.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone)]
struct VideoMock {
    creations: Arc<AtomicUsize>,
}

async fn mock_create(State(m): State<VideoMock>, body: Bytes) -> axum::response::Response {
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if req["prompt"] == "reject" {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": {"message": "content policy"}})),
        )
            .into_response();
    }
    m.creations.fetch_add(1, Ordering::SeqCst);
    let id = if req["prompt"] == "slow" {
        "r_slow"
    } else {
        "r_done"
    };
    axum::Json(json!({"id": id, "status": "queued"})).into_response()
}

async fn mock_status(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    let status = if id == "r_slow" {
        "in_progress"
    } else {
        "completed"
    };
    axum::Json(json!({"id": id, "status": status})).into_response()
}

async fn mock_content() -> axum::response::Response {
    (
        [(axum::http::header::CONTENT_TYPE, "video/mp4")],
        b"FAKE-MP4".to_vec(),
    )
        .into_response()
}

async fn mock_delete() -> axum::response::Response {
    axum::Json(json!({"deleted": true})).into_response()
}

/// Returns the mock's `/v1` base URL and a counter of accepted creations.
async fn start_mock() -> (String, Arc<AtomicUsize>) {
    let creations = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/v1/videos", post(mock_create))
        .route("/v1/videos/{id}", get(mock_status).delete(mock_delete))
        .route("/v1/videos/{id}/content", get(mock_content))
        .with_state(VideoMock {
            creations: creations.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/v1"), creations)
}

async fn add_provider(name: &str, base_url: &str) -> String {
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO custom_providers (name, base_url, enabled, capabilities) VALUES (?1, ?2, 1, ?3)",
        rusqlite::params![name, base_url, r#"["chat","video_generation"]"#],
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

async fn declare(platform: &str, model: &str) {
    let conn = server::db::connection::db().lock().await;
    conn.execute(
        "INSERT INTO media_models (platform, model_id, capability, priority, enabled) \
         VALUES (?1, ?2, 'video_generation', 0, 1)",
        rusqlite::params![platform, model],
    )
    .unwrap();
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, bytes)
}

fn create_request(key: &str, idem: Option<&str>, prompt: &str) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/v1/videos")
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json");
    if let Some(i) = idem {
        b = b.header("idempotency-key", i);
    }
    b.body(Body::from(
        json!({"model": "veo-test", "prompt": prompt}).to_string(),
    ))
    .unwrap()
}

fn get_request(key: &str, uri: &str, method: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn idempotent_create_starts_one_paid_job_then_downloads() {
    let app = common::setup().await;
    let (base, creations) = start_mock().await;
    let platform = add_provider("video-mock", &base).await;
    add_key(&platform, "video-key").await;
    declare(&platform, "veo-test").await;

    let (status, bytes) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-1"), "a fox"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let first: Value = serde_json::from_slice(&bytes).unwrap();
    let id = first["id"].as_str().unwrap().to_string();

    // Ambiguous retry with the same key: same job, no second upstream creation.
    let (status, bytes) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-1"), "a fox"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(second["id"], first["id"]);
    assert_eq!(creations.load(Ordering::SeqCst), 1);

    // Status refresh pulls the completed state from the provider.
    let (status, bytes) = send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}"), "GET"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let job: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(job["status"], "completed");

    let (status, bytes) = send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}/content"), "GET"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"FAKE-MP4");
}

#[tokio::test]
async fn invalid_idempotency_key_is_rejected() {
    let app = common::setup().await;
    let long = "k".repeat(201);
    let (status, _) = send(&app.app, create_request(&app.api_key, Some(&long), "x")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn explicit_provider_rejection_leaves_no_job_so_the_key_can_retry() {
    let app = common::setup().await;
    let (base, creations) = start_mock().await;
    let platform = add_provider("video-reject", &base).await;
    add_key(&platform, "k").await;
    declare(&platform, "veo-test").await;

    let (status, _) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-reject"), "reject"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-reject"), "fine"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(creations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn ambiguous_transport_failure_is_not_retried_and_stays_recorded() {
    let app = common::setup().await;
    // Closed loopback port: transport failure after the request may have left.
    let platform = add_provider("video-dead", "http://127.0.0.1:9/v1").await;
    add_key(&platform, "k").await;
    declare(&platform, "veo-test").await;

    let (status, _) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-amb"), "x"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    // The retry resolves to the recorded failed job; nothing new is started.
    let (status, bytes) = send(
        &app.app,
        create_request(&app.api_key, Some("idem-amb"), "x"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let job: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(job["status"], "failed");
}

#[tokio::test]
async fn content_is_not_available_until_completed_and_delete_cancels_active_jobs() {
    let app = common::setup().await;
    let (base, _) = start_mock().await;
    let platform = add_provider("video-slow", &base).await;
    add_key(&platform, "k").await;
    declare(&platform, "veo-test").await;

    let (_, bytes) = send(&app.app, create_request(&app.api_key, None, "slow")).await;
    let id = serde_json::from_slice::<Value>(&bytes).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, _) = send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}/content"), "GET"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, bytes) = send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}"), "DELETE"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "cancelled");
}

#[tokio::test]
async fn expired_content_is_gone() {
    let app = common::setup().await;
    let (base, _) = start_mock().await;
    let platform = add_provider("video-expiry", &base).await;
    add_key(&platform, "k").await;
    declare(&platform, "veo-test").await;

    let (_, bytes) = send(&app.app, create_request(&app.api_key, None, "done")).await;
    let id = serde_json::from_slice::<Value>(&bytes).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // Refresh to completion, then move the deadline into the past.
    send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}"), "GET"),
    )
    .await;
    {
        let conn = server::db::connection::db().lock().await;
        conn.execute(
            "UPDATE video_jobs SET expires_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
    }
    let (status, _) = send(
        &app.app,
        get_request(&app.api_key, &format!("/v1/videos/{id}/content"), "GET"),
    )
    .await;
    assert_eq!(status, StatusCode::GONE);
}

#[tokio::test]
async fn sweep_recovers_jobs_interrupted_before_acceptance() {
    let app = common::setup().await;
    {
        let conn = server::db::connection::db().lock().await;
        conn.execute(
            "INSERT INTO video_jobs (id, model_id, status, created_at, updated_at) \
             VALUES ('video_stuck', 'veo-test', 'submitting', '2000-01-01T00:00:00.000Z', '2000-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
    }
    server::services::video_jobs::sweep().await;
    let (status, bytes) = send(
        &app.app,
        get_request(&app.api_key, "/v1/videos/video_stuck", "GET"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let job: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(job["status"], "failed");
}
