//! OpenAI-compatible audio endpoints (`/v1/audio/*`) and the admin API that
//! declares which models serve which non-chat operation.

use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Path};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::Json;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::error::ApiError;
use crate::providers::capabilities::unsupported;
use crate::providers::capabilities::{MultipartFile, MultipartRequest};
use crate::providers::{get_provider_with_conn, Capability};
use crate::services::media::{
    candidates, provider_error_status, record_media_request, run_with_fallback, MediaCandidate,
};

/// Longest accepted speech input, in characters.
const MAX_SPEECH_INPUT_CHARS: usize = 4096;
const MAX_MODEL_ID_CHARS: usize = 200;

/// Error body in the OpenAI shape, with the status code preserved.
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

fn from_api_error(e: ApiError) -> Response {
    error_response(e.status, e.message)
}

fn required_string<'a>(body: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    match body.get(key).and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => Ok(s),
        Some(_) => Err(ApiError::new(400, format!("{key} must not be empty"))),
        None => Err(ApiError::new(
            400,
            format!("{key} is required and must be a string"),
        )),
    }
}

async fn record(
    candidate: Option<&MediaCandidate>,
    status: &str,
    started: Instant,
    error: Option<&str>,
) {
    if let Some(c) = candidate {
        let conn = db().lock().await;
        record_media_request(
            &conn,
            c,
            status,
            started.elapsed().as_millis() as i64,
            error,
        );
    }
}

/// `POST /v1/audio/speech` — text-to-speech. The upstream audio is streamed
/// through without buffering the whole file.
pub async fn speech(body: Bytes) -> Response {
    let started = Instant::now();
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error_response(400, "Malformed JSON payload"),
    };
    if !raw.is_object() {
        return error_response(400, "Expected a JSON object");
    }
    let model = match required_string(&raw, "model") {
        Ok(m) => m.to_string(),
        Err(e) => return from_api_error(e),
    };
    let input = match required_string(&raw, "input") {
        Ok(i) => i,
        Err(e) => return from_api_error(e),
    };
    if input.chars().count() > MAX_SPEECH_INPUT_CHARS {
        return error_response(
            400,
            format!("input must be at most {MAX_SPEECH_INPUT_CHARS} characters"),
        );
    }
    if let Err(e) = required_string(&raw, "voice") {
        return from_api_error(e);
    }

    let cands = {
        let conn = db().lock().await;
        match candidates(&conn, Capability::Speech, &model) {
            Ok(c) => c,
            Err(e) => return from_api_error(e),
        }
    };
    let first = cands.first().cloned();

    // Speech output is cheap to regenerate, so transport failures may fall back.
    let result = run_with_fallback(cands, true, |c| {
        let raw = raw.clone();
        async move {
            let provider = c
                .provider
                .speech()
                .ok_or_else(|| unsupported(&c.platform, Capability::Speech))?;
            provider.speech(&c.api_key, &raw).await
        }
    })
    .await;

    match result {
        Ok((stream, winner)) => {
            record(Some(&winner), "success", started, None).await;
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, stream.content_type)
                .body(Body::from_stream(stream.body))
                .unwrap_or_else(|_| error_response(500, "failed to build audio response"))
        }
        Err(err) => {
            record(first.as_ref(), "error", started, Some(&err.message)).await;
            error_response(provider_error_status(&err), err.message)
        }
    }
}

/// `POST /v1/audio/transcriptions`
pub async fn transcriptions(multipart: Multipart) -> Response {
    audio_upload(multipart, Capability::Transcription).await
}

/// `POST /v1/audio/translations`
pub async fn translations(multipart: Multipart) -> Response {
    audio_upload(multipart, Capability::Translation).await
}

/// Reads a multipart upload with exactly one `file` part and a `model` field.
async fn read_upload(mut multipart: Multipart) -> Result<(String, MultipartRequest), ApiError> {
    let mut req = MultipartRequest::default();
    let mut model: Option<String> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return Err(ApiError::new(400, format!("Invalid multipart body: {e}"))),
        };
        let name = field.name().unwrap_or_default().to_string();
        if field.file_name().is_some() {
            let filename = field.file_name().unwrap_or("upload").to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::new(400, format!("Invalid file part: {e}")))?;
            if data.is_empty() {
                return Err(ApiError::new(400, "Uploaded file is empty"));
            }
            req.files.push(MultipartFile {
                field: "file".to_string(),
                filename,
                content_type,
                data,
            });
        } else {
            let text = field
                .text()
                .await
                .map_err(|e| ApiError::new(400, format!("Invalid form field: {e}")))?;
            if name == "model" {
                model = Some(text.clone());
            }
            req.fields.push((name, text));
        }
    }
    let model = model
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| ApiError::new(400, "model is required"))?;
    if req.files.len() != 1 {
        return Err(ApiError::new(400, "Exactly one file part is required"));
    }
    Ok((model, req))
}

async fn audio_upload(multipart: Multipart, capability: Capability) -> Response {
    let started = Instant::now();
    let (model, req) = match read_upload(multipart).await {
        Ok(v) => v,
        Err(e) => return from_api_error(e),
    };
    let cands = {
        let conn = db().lock().await;
        match candidates(&conn, capability, &model) {
            Ok(c) => c,
            Err(e) => return from_api_error(e),
        }
    };
    let first = cands.first().cloned();

    let result = run_with_fallback(cands, true, |c| {
        let req = req.clone();
        async move {
            let provider = c
                .provider
                .transcription()
                .ok_or_else(|| unsupported(&c.platform, capability))?;
            if capability == Capability::Translation {
                provider.translate(&c.api_key, req).await
            } else {
                provider.transcribe(&c.api_key, req).await
            }
        }
    })
    .await;

    match result {
        Ok((resp, winner)) => {
            record(Some(&winner), "success", started, None).await;
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, resp.content_type)
                .body(Body::from(resp.body))
                .unwrap_or_else(|_| error_response(500, "failed to build response"))
        }
        Err(err) => {
            record(first.as_ref(), "error", started, Some(&err.message)).await;
            error_response(provider_error_status(&err), err.message)
        }
    }
}

/// Public `/v1` audio routes. Upload limits are applied by the caller.
pub fn v1_router() -> axum::Router {
    axum::Router::new()
        .route("/audio/speech", post(speech))
        .route("/audio/transcriptions", post(transcriptions))
        .route("/audio/translations", post(translations))
}

// ---- Admin: declare which models serve which operation ----

fn media_model_json(row: &(i64, String, String, String, i64, i64)) -> Value {
    json!({
        "id": row.0,
        "platform": row.1,
        "modelId": row.2,
        "capability": row.3,
        "priority": row.4,
        "enabled": row.5 != 0,
    })
}

async fn list_media_models() -> Response {
    let conn = db().lock().await;
    let mut stmt = match conn.prepare(
        "SELECT id, platform, model_id, capability, priority, enabled FROM media_models \
         ORDER BY capability, priority, id",
    ) {
        Ok(s) => s,
        Err(e) => return error_response(500, e.to_string()),
    };
    let rows: Vec<(i64, String, String, String, i64, i64)> = match stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .and_then(|it| it.collect())
    {
        Ok(rows) => rows,
        Err(e) => return error_response(500, e.to_string()),
    };
    Json(json!({ "data": rows.iter().map(media_model_json).collect::<Vec<_>>() })).into_response()
}

async fn create_media_model(body: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error_response(400, "Malformed JSON payload"),
    };
    let platform = match required_string(&raw, "platform") {
        Ok(p) => p.to_string(),
        Err(e) => return from_api_error(e),
    };
    let model_id = match required_string(&raw, "modelId") {
        Ok(m) if m.chars().count() <= MAX_MODEL_ID_CHARS => m.to_string(),
        Ok(_) => return error_response(400, "modelId is too long"),
        Err(e) => return from_api_error(e),
    };
    let capability_name = match required_string(&raw, "capability") {
        Ok(c) => c,
        Err(e) => return from_api_error(e),
    };
    let Some(capability) = Capability::parse(capability_name).filter(|c| *c != Capability::Chat)
    else {
        return error_response(
            400,
            format!("capability must be one of the non-chat operations, got '{capability_name}'"),
        );
    };
    let priority = raw.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
    let enabled = raw.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    let conn = db().lock().await;
    let provider = match get_provider_with_conn(&conn, &platform) {
        Some(p) => p,
        None => return error_response(400, format!("Unknown provider '{platform}'")),
    };
    if !provider.capabilities().contains(capability) {
        return error_response(
            400,
            format!(
                "Provider '{platform}' does not support '{}'",
                capability.as_str()
            ),
        );
    }
    if let Err(e) = conn.execute(
        "INSERT INTO media_models (platform, model_id, capability, priority, enabled) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(platform, model_id, capability) DO UPDATE SET \
           priority = excluded.priority, enabled = excluded.enabled",
        rusqlite::params![
            platform,
            model_id,
            capability.as_str(),
            priority,
            enabled as i64
        ],
    ) {
        return error_response(500, e.to_string());
    }
    let row = conn
        .query_row(
            "SELECT id, platform, model_id, capability, priority, enabled FROM media_models \
             WHERE platform = ?1 AND model_id = ?2 AND capability = ?3",
            rusqlite::params![platform, model_id, capability.as_str()],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .ok();
    match row {
        Some(row) => (StatusCode::CREATED, Json(media_model_json(&row))).into_response(),
        None => error_response(500, "media model was not stored"),
    }
}

async fn delete_media_model(Path(id): Path<i64>) -> Response {
    let conn = db().lock().await;
    match conn.execute(
        "DELETE FROM media_models WHERE id = ?1",
        rusqlite::params![id],
    ) {
        Ok(0) => error_response(404, "Media model not found"),
        Ok(_) => Json(json!({ "success": true })).into_response(),
        Err(e) => error_response(500, e.to_string()),
    }
}

/// Admin routes, mounted under `/api/media-models` behind the session guard.
pub fn admin_router() -> axum::Router {
    axum::Router::new()
        .route("/", get(list_media_models).post(create_media_model))
        .route("/{id}", delete(delete_media_model))
}
