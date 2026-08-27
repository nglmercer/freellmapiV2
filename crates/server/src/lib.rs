//! FreeLLMAPI server — Rust port of the Bun/TypeScript Hono app.
//! Same HTTP API, same SQLite file, same encrypted-key format.

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

    /// Mirrors lib/logger.ts: level from LOG_LEVEL (default 'warn'),
    /// fully disabled when LOG_ENABLED=false.
    pub fn init() {
        let enabled = std::env::var("LOG_ENABLED").map(|v| v != "false").unwrap_or(true);
        let filter = if !enabled {
            EnvFilter::new("off")
        } else {
            let level = std::env::var("LOG_LEVEL")
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
