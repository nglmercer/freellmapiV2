//! HTTP application construction and static-asset serving.

use axum::routing::get;
use axum::Json;
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::{ServeDir, ServeFile};

/// Serve the built client from `client/dist`, or from `STATIC_DIR` when set.
fn dist_dir() -> PathBuf {
    if let Some(p) = crate::env::env_string("STATIC_DIR") {
        return PathBuf::from(p);
    }
    crate::env::project_root().join("client/dist")
}

pub fn create_app() -> axum::Router {
    use crate::routes;
    use axum::routing::post;

    // Middleware - Allow all origins by default (as requested)
    let app = axum::Router::new()
        // Health check
        .route(
            "/api/ping",
            get(|| async {
                Json(serde_json::json!({
                    "status": "ok",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }))
            }),
        )
        // API routes
        .nest("/api/keys", routes::keys::router())
        .nest("/api/models", routes::models::router())
        .nest("/api/fallback", routes::fallback::router())
        .nest("/api/analytics", routes::analytics::router())
        .nest("/api/health", routes::health::router())
        .nest("/api/settings", routes::settings::router())
        .nest("/api/providers", routes::providers::router())
        // OpenAI-compatible proxy
        .nest(
            "/v1",
            axum::Router::new()
                .route("/models", get(routes::proxy::list_models))
                .route("/chat/completions", post(routes::proxy::chat_completions))
                .route(
                    "/completions",
                    post(routes::completions::completions_handler),
                ),
        );

    app.layer(CorsLayer::permissive())
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1MB, like bodyLimit
        .fallback_service(
            // Serve client static files — SPA fallback included (paths
            // without an extension rewrite to /index.html).
            ServeDir::new(dist_dir()).fallback(ServeFile::new(dist_dir().join("index.html"))),
        )
}

/// Render an application error as plain text with the given status.
pub fn error_text(status: u16, message: &str) -> axum::response::Response {
    use axum::response::IntoResponse;
    (
        axum::http::StatusCode::from_u16(status)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        message.to_string(),
    )
        .into_response()
}
