//! HTTP routes for the owner account: creating it, joining with a join code,
//! and managing that code from the dashboard.

use axum::extract::Json as JsonBody;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::account::{self, AccountError};

#[derive(Deserialize)]
pub struct CreateInput {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct JoinInput {
    code: String,
}

/// `POST /api/account/create` — creates the owner account. Public, but refused
/// once the account exists.
pub async fn create(uri: Uri, JsonBody(input): JsonBody<CreateInput>) -> Response {
    match account::create_owner(&input.username, &input.password, is_https(&uri)).await {
        Ok(result) => signed_in(result),
        Err(error) => error_response(&error),
    }
}

/// `POST /api/account/join` — signs the browser in with a join code.
pub async fn join(uri: Uri, JsonBody(input): JsonBody<JoinInput>) -> Response {
    match account::join_with_code(&input.code, is_https(&uri)).await {
        Ok(result) => signed_in(result),
        Err(error) => error_response(&error),
    }
}

/// `GET /api/account/join-code` — whether a join code is set.
async fn join_code_status() -> Response {
    Json(json!({ "enabled": account::join_code_enabled().await })).into_response()
}

/// `POST /api/account/join-code` — generates a new join code. Replaces any
/// previous code. The code is returned once and never shown again.
async fn generate_join_code() -> Response {
    let code = account::rotate_join_code().await;
    no_store(Json(json!({ "enabled": true, "code": code })).into_response())
}

/// `DELETE /api/account/join-code` — revokes the join code.
async fn revoke_join_code() -> Response {
    account::revoke_join_code().await;
    Json(json!({ "enabled": false })).into_response()
}

/// Public account routes, mounted outside the session guard.
pub fn public_routes() -> axum::Router {
    axum::Router::new()
        .route("/api/account/create", post(create))
        .route("/api/account/join", post(join))
}

/// Join-code management. Mounted in the admin router, so its paths are relative
/// to `/api`.
pub fn session_routes() -> axum::Router {
    axum::Router::new().route(
        "/account/join-code",
        get(join_code_status)
            .post(generate_join_code)
            .delete(revoke_join_code),
    )
}

fn is_https(uri: &Uri) -> bool {
    uri.scheme_str() == Some("https")
}

/// Sets the session cookie and returns the signed-in user.
fn signed_in(result: better_auth::AuthResult) -> Response {
    let cookie = result.cookie.to_set_cookie_header();
    let mut response = Json(json!({
        "user": { "id": result.user.id, "name": result.user.name }
    }))
    .into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    no_store(response)
}

fn error_response(error: &AccountError) -> Response {
    let (status, kind) = match error {
        AccountError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
        AccountError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
        AccountError::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication_error"),
        AccountError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    };
    if let AccountError::Internal(detail) = error {
        tracing::error!("[Account] {detail}");
    }
    let body = Json(json!({ "error": { "message": error.message(), "type": kind } }));
    no_store((status, body).into_response())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
