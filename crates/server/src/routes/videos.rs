//! Asynchronous video generation: `/v1/videos` and its status, content and
//! delete endpoints. Jobs are durable; see `services::video_jobs`.

use axum::body::{Body, Bytes};
use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::providers::capabilities::unsupported;
use crate::providers::Capability;
use crate::services::media::{candidates, provider_error_status, run_with_fallback};
use crate::services::video_jobs::{self, to_json};

const MAX_PROMPT_CHARS: usize = 4000;
const MAX_IDEMPOTENCY_KEY_CHARS: usize = 200;

fn error_response(status: u16, message: impl Into<String>) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let kind = if status < 500 {
        "invalid_request_error"
    } else {
        "api_error"
    };
    (
        code,
        Json(json!({ "error": { "message": message.into(), "type": kind } })),
    )
        .into_response()
}

fn new_job_id() -> String {
    let mut bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    format!("video_{}", hex::encode(bytes))
}

/// `POST /v1/videos` — creates an asynchronous generation job.
///
/// With an `Idempotency-Key` header, a retry returns the existing job (status
/// 200) instead of starting another paid generation.
pub async fn create_video(headers: HeaderMap, body: Bytes) -> Response {
    let idempotency_key = match headers.get("idempotency-key") {
        None => None,
        Some(value) => match value.to_str() {
            Ok(k) if !k.trim().is_empty() && k.chars().count() <= MAX_IDEMPOTENCY_KEY_CHARS => {
                Some(k.trim().to_string())
            }
            _ => {
                return error_response(
                    400,
                    format!("Idempotency-Key must be 1-{MAX_IDEMPOTENCY_KEY_CHARS} characters"),
                )
            }
        },
    };

    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error_response(400, "Malformed JSON payload"),
    };
    if !raw.is_object() {
        return error_response(400, "Expected a JSON object");
    }
    let Some(model) = raw
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
    else {
        return error_response(400, "model is required and must be a string");
    };
    let Some(prompt) = raw.get("prompt").and_then(|v| v.as_str()) else {
        return error_response(400, "prompt is required and must be a string");
    };
    if prompt.trim().is_empty() || prompt.chars().count() > MAX_PROMPT_CHARS {
        return error_response(
            400,
            format!("prompt must be 1-{MAX_PROMPT_CHARS} characters"),
        );
    }
    // Image-to-video: a reference image must be a valid image URL or data URL.
    if let Some(reference) = raw.get("input_reference").and_then(|v| v.as_str()) {
        if let Err(e) = crate::media::validate_image_url(reference) {
            return error_response(400, format!("input_reference: {e}"));
        }
    }

    // Replay: the same key always resolves to the same job.
    if let Some(key) = &idempotency_key {
        let conn = db().lock().await;
        if let Some(existing) = video_jobs::load_by_idempotency_key(&conn, key) {
            return (StatusCode::OK, Json(to_json(&existing))).into_response();
        }
    }

    let cands = {
        let conn = db().lock().await;
        match candidates(&conn, Capability::VideoGeneration, &model) {
            Ok(c) => c,
            Err(e) => return error_response(e.status, e.message),
        }
    };

    // Record the job before any paid call, so a crash after acceptance leaves
    // a trace that recovery can find.
    let job_id = new_job_id();
    let created = video_jobs::now();
    {
        let conn = db().lock().await;
        let inserted = conn.execute(
            "INSERT INTO video_jobs (id, idempotency_key, model_id, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'submitting', ?4, ?4)",
            rusqlite::params![job_id, idempotency_key, model, created],
        );
        if let Err(e) = inserted {
            // A concurrent request with the same key won the race.
            if let Some(key) = &idempotency_key {
                if let Some(existing) = video_jobs::load_by_idempotency_key(&conn, key) {
                    return (StatusCode::OK, Json(to_json(&existing))).into_response();
                }
            }
            return error_response(500, e.to_string());
        }
    }

    // Ambiguous failures (no HTTP status) never fall back: the provider may
    // already have started a paid job.
    let result = run_with_fallback(cands, false, |c| {
        let raw = raw.clone();
        async move {
            let video = c
                .provider
                .video()
                .ok_or_else(|| unsupported(&c.platform, Capability::VideoGeneration))?;
            video.create(&c.api_key, &raw).await
        }
    })
    .await;

    match result {
        Ok((upstream, winner)) => {
            let conn = db().lock().await;
            let expires = if upstream.status == "completed" {
                Some(video_jobs::expiry_from_now())
            } else {
                None
            };
            let _ = conn.execute(
                "UPDATE video_jobs SET platform = ?1, key_id = ?2, remote_id = ?3, status = ?4, \
                 error = ?5, expires_at = ?6, updated_at = ?7 WHERE id = ?8",
                rusqlite::params![
                    winner.platform,
                    winner.key_id,
                    upstream.remote_id,
                    upstream.status,
                    upstream.error,
                    expires,
                    video_jobs::now(),
                    job_id
                ],
            );
            let job = video_jobs::load(&conn, &job_id);
            match job {
                Some(job) => (StatusCode::CREATED, Json(to_json(&job))).into_response(),
                None => error_response(500, "video job was not stored"),
            }
        }
        Err(err) => {
            let conn = db().lock().await;
            if err.status.is_some() {
                // Explicit rejection: the provider did not create a job, so the
                // record is removed and the same key may be retried.
                let _ = conn.execute(
                    "DELETE FROM video_jobs WHERE id = ?1",
                    rusqlite::params![job_id],
                );
            } else {
                // Ambiguous: keep the record as failed so a retry with the same
                // key cannot start a second paid job.
                let _ = conn.execute(
                    "UPDATE video_jobs SET status = 'failed', error = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![err.message, video_jobs::now(), job_id],
                );
            }
            error_response(provider_error_status(&err), err.message)
        }
    }
}

/// `GET /v1/videos/{id}` — job status, refreshed from the provider when active.
pub async fn get_video(Path(id): Path<String>) -> Response {
    let exists = {
        let conn = db().lock().await;
        video_jobs::load(&conn, &id).is_some()
    };
    if !exists {
        return error_response(404, "Video job not found");
    }
    match video_jobs::refresh(&id).await {
        Some(job) => Json(to_json(&job)).into_response(),
        None => error_response(404, "Video job not found"),
    }
}

/// `GET /v1/videos/{id}/content` — streams the finished video.
pub async fn get_video_content(Path(id): Path<String>) -> Response {
    let Some(job) = video_jobs::refresh(&id).await else {
        return error_response(404, "Video job not found");
    };
    if video_jobs::is_expired(&job) {
        return error_response(410, "Video content has expired");
    }
    if job.status != "completed" {
        return error_response(409, format!("Video is not ready (status: {})", job.status));
    }
    let creds = {
        let conn = db().lock().await;
        video_jobs::credentials(&conn, &job)
    };
    let Some((provider, api_key)) = creds else {
        return error_response(503, "Provider or key for this video is no longer available");
    };
    let (Some(video), Some(remote_id)) = (provider.video(), job.remote_id.clone()) else {
        return error_response(400, "Provider does not support video retrieval");
    };
    match video.content(&api_key, &remote_id).await {
        Ok(stream) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, stream.content_type)
            .body(Body::from_stream(stream.body))
            .unwrap_or_else(|_| error_response(500, "failed to build video response")),
        Err(err) => error_response(provider_error_status(&err), err.message),
    }
}

/// `DELETE /v1/videos/{id}` — cancels an active job or removes a finished one.
/// Idempotent: repeating the call on a deleted job succeeds.
pub async fn delete_video(Path(id): Path<String>) -> Response {
    let job = {
        let conn = db().lock().await;
        match video_jobs::load(&conn, &id) {
            Some(job) => job,
            None => return error_response(404, "Video job not found"),
        }
    };
    if matches!(
        job.status.as_str(),
        "deleted" | "cancelled" | "expired" | "failed"
    ) {
        return Json(json!({ "id": id, "object": "video", "deleted": true, "status": job.status }))
            .into_response();
    }

    let creds = {
        let conn = db().lock().await;
        video_jobs::credentials(&conn, &job)
    };
    if let (Some((provider, api_key)), Some(remote_id)) = (creds, job.remote_id.clone()) {
        if let Some(video) = provider.video() {
            if let Err(err) = video.delete(&api_key, &remote_id).await {
                // A running job must be cancelled upstream before it is marked so.
                if job.is_active() {
                    return error_response(provider_error_status(&err), err.message);
                }
                tracing::warn!(job = %id, "best-effort upstream video delete failed: {}", err.message);
            }
        } else if job.is_active() {
            return error_response(400, "Provider does not support cancelling video jobs");
        }
    } else if job.is_active() && job.remote_id.is_some() {
        return error_response(503, "Provider or key for this video is no longer available");
    }

    let new_status = if job.is_active() {
        "cancelled"
    } else {
        "deleted"
    };
    let conn = db().lock().await;
    let _ = conn.execute(
        "UPDATE video_jobs SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![new_status, video_jobs::now(), id],
    );
    Json(json!({ "id": id, "object": "video", "deleted": true, "status": new_status }))
        .into_response()
}

/// Public `/v1` video routes. Upload limits are applied by the caller.
pub fn v1_router() -> axum::Router {
    axum::Router::new()
        .route("/videos", post(create_video))
        .route("/videos/{id}", get(get_video).delete(delete_video))
        .route("/videos/{id}/content", get(get_video_content))
}
