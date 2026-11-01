//! Benchmark-source credentials, set from the dashboard instead of `.env`.

use axum::extract::Json as JsonBody;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::services::integrations;

#[derive(Deserialize)]
struct KeyInput {
    key: String,
}

/// `GET /integrations/artificial-analysis`
async fn get_artificial_analysis() -> Response {
    Json(integrations::artificial_analysis_status()).into_response()
}

/// `PUT /integrations/artificial-analysis` — stores the key encrypted.
async fn put_artificial_analysis(JsonBody(input): JsonBody<KeyInput>) -> Response {
    if input.key.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Enter an Artificial Analysis API key." })),
        )
            .into_response();
    }
    match integrations::save_artificial_analysis_key(&input.key).await {
        Ok(()) => Json(integrations::artificial_analysis_status()).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response(),
    }
}

/// `DELETE /integrations/artificial-analysis`
async fn delete_artificial_analysis() -> Response {
    match integrations::clear_artificial_analysis_key().await {
        Ok(()) => Json(integrations::artificial_analysis_status()).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response(),
    }
}

pub fn router() -> axum::Router {
    axum::Router::new().route(
        "/artificial-analysis",
        get(get_artificial_analysis)
            .put(put_artificial_analysis)
            .delete(delete_artificial_analysis),
    )
}
