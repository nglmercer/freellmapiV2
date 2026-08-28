//! HTTP application construction and static-asset serving.

use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::Json;
use std::path::PathBuf;
use tower_http::cors::{AllowOrigin, CorsLayer};
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
    use axum::middleware;
    use axum::routing::post;

    let admin_routes = axum::Router::new()
        .nest("/keys", routes::keys::router())
        .nest("/models", routes::models::router())
        .nest("/fallback", routes::fallback::router())
        .nest("/analytics", routes::analytics::router())
        .nest("/health", routes::health::router())
        .nest("/settings", routes::settings::router())
        .nest("/providers", routes::providers::router())
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(middleware::from_fn(routes::middleware::admin_auth));

    // The unified key protects every OpenAI-compatible endpoint, including
    // GET /v1/models. Handler-level checks remain defense in depth.
    let proxy_routes = axum::Router::new()
        .route("/models", get(routes::proxy::list_models))
        .route("/chat/completions", post(routes::proxy::chat_completions))
        .route(
            "/completions",
            post(routes::completions::completions_handler),
        )
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(middleware::from_fn(routes::middleware::unified_api_auth));

    axum::Router::new()
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
        .nest("/api", admin_routes)
        .nest("/v1", proxy_routes)
        .layer(dashboard_cors())
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1MB, like bodyLimit
        .fallback_service(
            // Serve client static files — SPA fallback included (paths
            // without an extension rewrite to /index.html).
            ServeDir::new(dist_dir()).fallback(ServeFile::new(dist_dir().join("index.html"))),
        )
}

/// By default the dashboard is same-origin only. Cross-origin dashboard
/// access must be explicitly opted into with a comma-separated allowlist.
fn dashboard_cors() -> CorsLayer {
    let mut layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    let origins = crate::env::env_string("DASHBOARD_ORIGINS")
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|origin| !origin.is_empty() && *origin != "*")
        .filter_map(|origin| match origin.parse::<HeaderValue>() {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(%error, "Ignoring invalid DASHBOARD_ORIGINS entry");
                None
            }
        })
        .collect::<Vec<_>>();

    if !origins.is_empty() {
        layer = layer.allow_origin(AllowOrigin::list(origins));
    }
    layer
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
