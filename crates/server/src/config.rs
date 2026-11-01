//! Non-secret runtime settings, read from `config.toml`.
//!
//! The file is optional: every setting has a default, so a release build runs
//! without any configuration. The file is looked up at
//! `FREELLMAPI_CONFIG` if set, otherwise at `<project root>/config.toml`.
//!
//! Secrets are never read from this file. Provider API keys, the encryption
//! key and the auth signing secret are stored encrypted in the database (or
//! supplied through the dashboard).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

const DEFAULT_PORT: u16 = 3001;
const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1";
const DEFAULT_V1_MAX_BODY_BYTES: usize = 20 * 1024 * 1024;
const DEFAULT_V1_MEDIA_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MEDIA_MAX_CONCURRENCY: usize = 16;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub dashboard: DashboardConfig,
    pub limits: LimitsConfig,
    pub media: MediaConfig,
    pub sources: SourcesConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    /// HTTP port. `0` asks the OS for a free port (used by tests).
    pub port: u16,
    pub bind_address: String,
    /// Directory with the built dashboard. Defaults to `client/dist`.
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct DatabaseConfig {
    /// SQLite file. Defaults to `data/freeapi.db` under the project root.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct LoggingConfig {
    pub enabled: bool,
    /// `tracing` filter, for example `info` or `info,tower_http=debug`.
    pub level: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct DashboardConfig {
    /// Extra origins allowed to call the API from a browser (for example
    /// `http://pi.local:3001`). Local Vite origins are always allowed.
    pub origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct LimitsConfig {
    /// Request body limit for `/v1` chat and embeddings, in bytes.
    pub v1_max_body_bytes: usize,
    /// Request body limit for `/v1` audio, image and video routes, in bytes.
    pub v1_media_max_body_bytes: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct MediaConfig {
    /// Upstream media calls allowed in flight at once.
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct SourcesConfig {
    /// Public Hugging Face repository ids used for metadata lookups.
    pub huggingface_metadata_repos: Vec<String>,
    /// Overrides the artificial-analysis endpoint (testing or mirrors).
    pub artificial_analysis_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            bind_address: DEFAULT_BIND_ADDRESS.to_string(),
            static_dir: None,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: None,
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            v1_max_body_bytes: DEFAULT_V1_MAX_BODY_BYTES,
            v1_media_max_body_bytes: DEFAULT_V1_MEDIA_MAX_BODY_BYTES,
        }
    }
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            max_concurrency: DEFAULT_MEDIA_MAX_CONCURRENCY,
        }
    }
}

impl Config {
    /// Parses and validates TOML text. Errors name the offending field.
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.server.bind_address.trim().is_empty() {
            return Err("server.bind_address must not be empty".to_string());
        }
        if self.limits.v1_max_body_bytes == 0 || self.limits.v1_media_max_body_bytes == 0 {
            return Err("limits.* body sizes must be greater than zero".to_string());
        }
        if self.media.max_concurrency == 0 {
            return Err("media.max_concurrency must be greater than zero".to_string());
        }
        for origin in &self.dashboard.origins {
            if origin.trim().is_empty() || origin.trim() == "*" {
                return Err("dashboard.origins must list explicit origins, not '*'".to_string());
            }
        }
        Ok(())
    }
}

/// Location of the configuration file.
pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("FREELLMAPI_CONFIG") {
        return PathBuf::from(path);
    }
    crate::env::project_root().join("config.toml")
}

/// Loads the file at `path`. A missing file yields the defaults.
pub fn load_from(path: &Path) -> Result<Config, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Config::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Loads the configuration once. Call early at startup; an invalid file stops
/// the process with a message that names the file and the problem.
pub fn init() {
    let config = match load_from(&config_path()) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("invalid configuration: {message}");
            std::process::exit(1);
        }
    };
    let _ = CONFIG.set(config);
}

/// The active configuration. Loads with defaults if [`init`] was not called.
pub fn get() -> &'static Config {
    CONFIG.get_or_init(|| load_from(&config_path()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_uses_all_defaults() {
        let c = Config::parse("").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.server.port, 3001);
        assert_eq!(c.server.bind_address, "127.0.0.1");
        assert_eq!(c.limits.v1_max_body_bytes, 20 * 1024 * 1024);
        assert_eq!(c.media.max_concurrency, 16);
        assert!(c.logging.enabled);
    }

    #[test]
    fn parses_every_section() {
        let c = Config::parse(
            r#"
            [server]
            port = 8080
            bind_address = "0.0.0.0"
            static_dir = "/srv/dashboard"

            [database]
            path = "/var/lib/freellmapi/freeapi.db"

            [logging]
            enabled = true
            level = "info,tower_http=debug"

            [dashboard]
            origins = ["http://pi.local:3001"]

            [limits]
            v1_max_body_bytes = 1048576
            v1_media_max_body_bytes = 2097152

            [media]
            max_concurrency = 4

            [sources]
            huggingface_metadata_repos = ["org/model"]
            artificial_analysis_url = "https://mirror.example.test/aa"
            "#,
        )
        .unwrap();
        assert_eq!(c.server.port, 8080);
        assert_eq!(c.server.static_dir, Some(PathBuf::from("/srv/dashboard")));
        assert_eq!(c.dashboard.origins, vec!["http://pi.local:3001"]);
        assert_eq!(c.media.max_concurrency, 4);
        assert_eq!(c.sources.huggingface_metadata_repos, vec!["org/model"]);
    }

    #[test]
    fn unknown_keys_are_rejected_so_typos_do_not_pass_silently() {
        let err = Config::parse("[server]\nprot = 3000\n").unwrap_err();
        assert!(err.contains("prot"), "{err}");
    }

    #[test]
    fn secrets_are_not_accepted_in_the_file() {
        // A key placed in the file is a configuration error, not a silent no-op.
        assert!(Config::parse("encryption_key = \"x\"\n").is_err());
        assert!(Config::parse("[sources]\nartificial_analysis_api_key = \"x\"\n").is_err());
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert!(Config::parse("[media]\nmax_concurrency = 0\n").is_err());
        assert!(Config::parse("[limits]\nv1_max_body_bytes = 0\n").is_err());
        assert!(Config::parse("[dashboard]\norigins = [\"*\"]\n").is_err());
        assert!(Config::parse("[server]\nbind_address = \"\"\n").is_err());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = std::env::temp_dir().join("freellmapi-missing-config-test.toml");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_from(&path).unwrap(), Config::default());
    }
}
