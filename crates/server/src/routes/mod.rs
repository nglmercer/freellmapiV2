//! HTTP route handlers wired into the application router.

pub mod analytics;
pub mod completions;
pub mod fallback;
pub mod health;
pub mod keys;
pub mod middleware;
pub mod models;
pub mod providers;
pub mod proxy;
pub(crate) mod ranking_json;
pub mod settings;
pub mod stream_handler;
