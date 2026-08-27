//! FreeLLMAPI server library and HTTP application.
//! The public API and SQLite/encryption formats remain backwards compatible.

pub mod app;
pub mod db;
pub mod env;
pub mod error;
pub mod providers;
pub mod routes;
pub mod services;
pub mod types;

// `lib/` folder (crypto) — declared with a path to avoid colliding with the
// crate root file itself.
#[path = "lib/crypto.rs"]
pub mod crypto;

pub mod logger {
    use tracing_subscriber::{fmt, EnvFilter};

    /// Initializes tracing. `RUST_LOG` is preferred, with `LOG_LEVEL` kept as
    /// a backwards-compatible alias; `LOG_ENABLED=false` disables logging.
    pub fn init() {
        let enabled = std::env::var("LOG_ENABLED")
            .map(|v| v != "false")
            .unwrap_or(true);
        let filter = if !enabled {
            EnvFilter::new("off")
        } else {
            let level = std::env::var("RUST_LOG")
                .or_else(|_| std::env::var("LOG_LEVEL"))
                .map(|v| v.to_lowercase())
                .unwrap_or_else(|_| "warn".to_string());
            EnvFilter::try_new(&level).unwrap_or_else(|_| EnvFilter::new("warn"))
        };
        let _ = fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_target(false)
            .try_init();
    }
}
