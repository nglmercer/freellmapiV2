//! Optional `.env` loading and project-root discovery.
//!
//! The binary can run from anywhere, so the project root is located by
//! walking up from the current directory and looking for the client plus the
//! legacy data directory or a workspace `Cargo.toml`. The tray launcher sets
//! `FREELLMAPI_CONFIG_DIR` so generated configuration stays with its app data.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PROJECT_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn project_root() -> PathBuf {
    if let Some(p) = PROJECT_ROOT.get() {
        return p.clone();
    }
    if let Some(path) = std::env::var_os("FREELLMAPI_CONFIG_DIR") {
        let root = PathBuf::from(path);
        let _ = PROJECT_ROOT.set(root.clone());
        return root;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut cur: Option<&Path> = Some(cwd.as_path());
    while let Some(dir) = cur {
        candidates.push(dir.to_path_buf());
        cur = dir.parent();
    }
    // First ancestor with the client and legacy data directories, or a
    // workspace Cargo.toml, wins. Fall back to CWD.
    let root = candidates
        .into_iter()
        .find(|dir| {
            (dir.join("client").is_dir() && dir.join("server/data").is_dir())
                || dir.join("Cargo.toml").exists()
                    && std::fs::read_to_string(dir.join("Cargo.toml"))
                        .map(|s| s.contains("[workspace]"))
                        .unwrap_or(false)
        })
        .unwrap_or(cwd);
    let _ = PROJECT_ROOT.set(root.clone());
    root
}

/// Overrides the root — used by tests to work in temp directories.
pub fn set_project_root(path: PathBuf) {
    let _ = PROJECT_ROOT.set(path);
}

pub fn get_port() -> u16 {
    env_string("PORT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001)
}

pub fn get_bind_address() -> String {
    env_string("BIND_ADDRESS").unwrap_or_else(|| "127.0.0.1".to_string())
}

/// Env lookup: real env var first, then the loaded .env file.
pub fn env_string(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// Loads `<root>/.env` when it exists. The file is optional: every setting
/// has a default, and secrets are generated and stored in the database when
/// they are not supplied.
pub fn init() {
    let env_path = project_root().join(".env");
    if env_path.exists() {
        dotenvy::from_path(&env_path).ok();
    }
}
