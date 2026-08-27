//! Port of `server/src/routes/` — HTTP handlers. Submodules are wired into
//! the router in `app.rs`.

pub mod analytics;
pub mod completions;
pub mod fallback;
pub mod health;
pub mod keys;
pub mod middleware;
pub mod models;
pub mod providers;
pub mod proxy;
pub mod settings;
pub mod stream_handler;
