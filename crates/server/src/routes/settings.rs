//! Setup status, unified API-key retrieval, and key regeneration endpoints.

use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use serde_json::json;

use crate::db::connection::db;
use crate::db::unified_key::{get_unified_api_key, regenerate_unified_key};

/// `GET /setup-status` — first-time detection + the unified key.
async fn setup_status() -> Response {
    let key_count: i64 = {
        let conn = db().lock().await;
        conn.query_row("SELECT COUNT(*) FROM api_keys", [], |r| r.get(0))
            .unwrap_or(0)
    };
    let unified_key = get_unified_api_key().await;
    Json(json!({
        "isSetup": key_count > 0,
        "keyCount": key_count,
        "unifiedKey": unified_key,
    }))
    .into_response()
}

/// `GET /api-key`
async fn api_key() -> Response {
    Json(json!({ "apiKey": get_unified_api_key().await })).into_response()
}

/// `POST /api-key/regenerate`
async fn regenerate_api_key() -> Response {
    let new_key = regenerate_unified_key().await;
    Json(json!({ "apiKey": new_key })).into_response()
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/setup-status", get(setup_status))
        .route("/api-key", get(api_key))
        .route("/api-key/regenerate", post(regenerate_api_key))
}
