//! FreeLLMAPI server library and HTTP application.

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod env;
pub mod error;
pub mod media;
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

    /// Initializes tracing from `[logging]` in config.toml. `enabled = false`
    /// turns logging off; `level` takes a `tracing` filter string.
    pub fn init() {
        let logging = &crate::config::get().logging;
        let filter = if !logging.enabled {
            EnvFilter::new("off")
        } else {
            let level = logging
                .level
                .clone()
                .unwrap_or_else(|| "warn".to_string())
                .to_lowercase();
            EnvFilter::try_new(&level).unwrap_or_else(|_| EnvFilter::new("warn"))
        };
        let _ = fmt::Subscriber::builder()
            .with_env_filter(filter)
            .with_target(false)
            .try_init();
    }
}
