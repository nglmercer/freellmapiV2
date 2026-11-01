//! Setup status for the dashboard. API keys are managed through Better Auth
//! (`/api/auth/api-key/*`), and dashboard sign-in through `/api/auth/*`.

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde_json::json;

use crate::db::connection::db;

/// `GET /setup-status` — whether any provider keys have been added.
async fn setup_status() -> Response {
    let key_count: i64 = {
        let conn = db().lock().await;
        conn.query_row("SELECT COUNT(*) FROM api_keys", [], |r| r.get(0))
            .unwrap_or(0)
    };
    let mut response = Json(json!({
        "isSetup": key_count > 0,
        "keyCount": key_count,
    }))
    .into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

pub fn router() -> axum::Router {
    axum::Router::new().route("/setup-status", get(setup_status))
}
