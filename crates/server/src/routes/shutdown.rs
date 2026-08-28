//! Authenticated local shutdown endpoint used by the desktop launcher.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// `POST /api/shutdown` — request a graceful server shutdown.
pub async fn request_shutdown() -> Response {
    crate::services::shutdown::request();
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "shutting_down" })),
    )
        .into_response()
}
