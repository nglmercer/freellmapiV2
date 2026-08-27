//! Port of `server/src/env.ts`.
//!
//! The TS version resolves paths relative to its own module file
//! (`server/src/env.ts` → repo root). The Rust binary can run from anywhere,
//! so we locate the project root by walking up from the CWD looking for
//! markers (`client/` and `server/`, or a workspace `Cargo.toml`), then do
//! the same `.env` ensure/load/validate dance.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PROJECT_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn project_root() -> PathBuf {
    if let Some(p) = PROJECT_ROOT.get() {
        return p.clone();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut cur: Option<&Path> = Some(cwd.as_path());
    while let Some(dir) = cur {
        candidates.push(dir.to_path_buf());
        cur = dir.parent();
    }
    // First ancestor with both client/ and server/ directories, or a
    // workspace Cargo.toml, wins. Fall back to CWD.
    let root = candidates
        .into_iter()
        .find(|dir| {
            (dir.join("client").is_dir() && dir.join("server").is_dir())
                || dir
                    .join("Cargo.toml")
                    .exists()
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

/// Ensures `<root>/.env` exists with a valid 64-hex-char ENCRYPTION_KEY,
/// seeding from `.env.example` when possible (verbatim behavior of env.ts).
fn ensure_encryption_key() {
    let root = project_root();
    let env_path = root.join(".env");
    let example_path = root.join(".env.example");

    let random_key = {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
        hex::encode(bytes)
    };

    let is_valid_hex64 = |v: &str| {
        v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit())
    };

    // Regex equivalent of /^ENCRYPTION_KEY=(.*)$/m
    let key_line_re = key_line_re();

    if !env_path.exists() {
        let written = if example_path.exists() {
            std::fs::read_to_string(&example_path)
                .ok()
                .map(|example| {
                    key_line_re.replace_all(&example, format!("ENCRYPTION_KEY={random_key}"))
                        .to_string()
                })
        } else {
            None
        };
        let content = written.unwrap_or_else(|| format!("ENCRYPTION_KEY={random_key}\nPORT=3001\n"));
        std::fs::write(&env_path, content).ok();
        tracing::info!("[ENV] Created .env with generated encryption key");
        return;
    }

    // .env exists — check if ENCRYPTION_KEY is present and valid.
    let Ok(env_content) = std::fs::read_to_string(&env_path) else {
        return;
    };
    let has_valid = key_line_re
        .captures(&env_content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .map(|v| !v.is_empty() && is_valid_hex64(&v))
        .unwrap_or(false);

    if !has_valid {
        let replaced = key_line_re.replace_all(&env_content, format!("ENCRYPTION_KEY={random_key}"));
        if replaced.as_ref() == env_content.as_str() {
            // No existing ENCRYPTION_KEY line — append
            std::fs::write(
                &env_path,
                format!("{env_content}\nENCRYPTION_KEY={random_key}\n"),
            )
            .ok();
        } else {
            std::fs::write(&env_path, replaced.as_ref()).ok();
        }
        tracing::info!("[ENV] Updated .env with generated encryption key");
    }
}

fn key_line_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?m)^ENCRYPTION_KEY=(.*)$").expect("valid regex")
    })
}

/// `env.validateEncryptionKey()` — panics if missing/invalid.
pub fn validate_encryption_key() {
    let key = std::env::var("ENCRYPTION_KEY").ok();
    let Some(key) = key else {
        panic!("ENCRYPTION_KEY environment variable is required");
    };
    if key.len() != 64 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
        panic!("ENCRYPTION_KEY must be a 64-character hexadecimal string");
    }
}

pub fn get_port() -> u16 {
    env_string("PORT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001)
}

/// Env lookup: real env var first, then the loaded .env file.
pub fn env_string(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// Initialize: ensure .env encryption key, load .env, validate.
/// Mirrors the module-load side effects of `env.ts`.
pub fn init() {
    ensure_encryption_key();
    let env_path = project_root().join(".env");
    if env_path.exists() {
        dotenvy::from_path(env_path.clone()).ok();
    }
    validate_encryption_key();
}
