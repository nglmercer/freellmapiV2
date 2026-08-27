//! Shared error type — the Rust equivalent of Hono's `HTTPException`.
//! Renders as plain text with the given status, matching the TS error
//! handler (`c.text(message, status)`).

use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::from_u16(self.status)
                .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
            self.message,
        )
            .into_response()
    }
}
