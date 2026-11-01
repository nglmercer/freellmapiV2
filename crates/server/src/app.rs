//! HTTP application construction and static-asset serving.

use axum::http::{header, HeaderValue, Method};
use axum::routing::{any, get, post};
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

    let admin_routes = axum::Router::new()
        .nest("/keys", routes::keys::router())
        .nest("/models", routes::models::router())
        .nest("/metadata", routes::metadata::router())
        .route("/routing/simulate", post(routes::metadata::simulate))
        .route(
            "/routing/policy",
            get(routes::metadata::get_policy).patch(routes::metadata::set_policy),
        )
        .nest("/fallback", routes::fallback::router())
        .nest("/analytics", routes::analytics::router())
        .nest("/health", routes::health::router())
        .nest("/settings", routes::settings::router())
        .merge(routes::account::session_routes())
        .nest("/providers", routes::providers::router())
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(middleware::from_fn(crate::auth::require_session));

    // API keys (or a dashboard session) protect every OpenAI-compatible endpoint, including
    // GET /v1/models. Handlers do not repeat the check.
    let proxy_routes = axum::Router::new()
        .route("/models", get(routes::proxy::list_models))
        .route("/chat/completions", post(routes::proxy::chat_completions))
        .route(
            "/completions",
            post(routes::completions::completions_handler),
        )
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(middleware::from_fn(crate::auth::require_api_access));

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
        // Better Auth owns sign-in, sign-out, session lookup and API keys.
        .route("/api/auth/{*path}", any(crate::auth::handler))
        .route("/api/account-status", get(account_status))
        .merge(routes::account::public_routes())
        // Shutdown accepts the launcher's token or a dashboard session, so it
        // sits outside the session-only admin router.
        .route("/api/shutdown", post(routes::shutdown::request_shutdown))
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

/// `GET /api/account-status` — public. Tells the dashboard whether the owner
/// account exists yet, so it can show sign-up or sign-in.
async fn account_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "hasAccount": crate::auth::account_exists().await }))
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
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::HeaderName::from_static("x-api-key"),
        ])
        .allow_credentials(true);

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
