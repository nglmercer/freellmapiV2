//! Authenticated local shutdown endpoint used by the desktop launcher.

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// `POST /api/shutdown` — request a graceful server shutdown. Accepts the
/// launcher's per-launch token or a signed-in dashboard session.
pub async fn request_shutdown(headers: HeaderMap) -> Response {
    if !crate::auth::shutdown_token_matches(&headers) && !crate::auth::is_signed_in(&headers).await
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "message": "Invalid or missing shutdown credentials",
                    "type": "authentication_error"
                }
            })),
        )
            .into_response();
    }
    crate::services::shutdown::request();
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "shutting_down" })),
    )
        .into_response()
}
