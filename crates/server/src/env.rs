//! Environment loading, validation, and project-root discovery.
//!
//! The binary can run from anywhere, so the project root is located by
//! walking up from the current directory and looking for the client plus the
//! legacy data directory or a workspace `Cargo.toml`. The tray launcher sets
//! `FREELLMAPI_CONFIG_DIR` so generated configuration stays with its app data.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PROJECT_ROOT: OnceLock<PathBuf> = OnceLock::new();
const ADMIN_API_KEY_PLACEHOLDER: &str = "replace-with-a-long-random-admin-key";

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

    let is_valid_hex64 = |v: &str| v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit());

    // Regex equivalent of /^ENCRYPTION_KEY=(.*)$/m
    let key_line_re = key_line_re();

    if !env_path.exists() {
        let written = if example_path.exists() {
            std::fs::read_to_string(&example_path).ok().map(|example| {
                key_line_re
                    .replace_all(&example, format!("ENCRYPTION_KEY={random_key}"))
                    .to_string()
            })
        } else {
            None
        };
        let content =
            written.unwrap_or_else(|| format!("ENCRYPTION_KEY={random_key}\nPORT=3001\n"));
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
        let replaced =
            key_line_re.replace_all(&env_content, format!("ENCRYPTION_KEY={random_key}"));
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

/// Ensures the separate dashboard credential exists in the local .env.
/// Keeping it out of the unified key makes accidental exposure of a proxy key
/// insufficient to mutate server configuration.
fn ensure_admin_api_key() {
    let root = project_root();
    let env_path = root.join(".env");
    let example_path = root.join(".env.example");
    let random_key = {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
        hex::encode(bytes)
    };
    let line_re = admin_key_line_re();

    if !env_path.exists() {
        let content = std::fs::read_to_string(&example_path)
            .ok()
            .map(|example| {
                line_re
                    .replace_all(&example, format!("ADMIN_API_KEY={random_key}"))
                    .to_string()
            })
            .unwrap_or_default();
        let content = if content.is_empty() {
            format!("ADMIN_API_KEY={random_key}\n")
        } else if line_re.is_match(&content) {
            content
        } else {
            format!("{content}\nADMIN_API_KEY={random_key}\n")
        };
        std::fs::write(&env_path, content).ok();
        tracing::info!("[ENV] Created .env with generated admin API key");
        return;
    }

    let Ok(env_content) = std::fs::read_to_string(&env_path) else {
        return;
    };
    let has_valid = line_re
        .captures(&env_content)
        .and_then(|captures| captures.get(1))
        .map(|value| is_valid_admin_api_key(value.as_str()))
        .unwrap_or(false);
    if has_valid {
        return;
    }

    let replaced = line_re.replace_all(&env_content, format!("ADMIN_API_KEY={random_key}"));
    if replaced.as_ref() == env_content.as_str() {
        std::fs::write(
            &env_path,
            format!("{}\nADMIN_API_KEY={random_key}\n", env_content.trim_end()),
        )
        .ok();
    } else {
        std::fs::write(&env_path, replaced.as_ref()).ok();
    }
    tracing::info!("[ENV] Updated .env with generated admin API key");
}

/// Admin credentials must be long enough to be useful as a bearer secret and
/// must be valid HTTP token characters because the tray sends this value in a
/// raw `Authorization` header during graceful shutdown.
pub fn is_valid_admin_api_key(value: &str) -> bool {
    value.len() >= 32 && value != ADMIN_API_KEY_PLACEHOLDER && value.bytes().all(is_http_token_byte)
}

fn is_http_token_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}

fn key_line_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?m)^ENCRYPTION_KEY=(.*)$").expect("valid regex"))
}

fn admin_key_line_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?m)^ADMIN_API_KEY=(.*)$").expect("valid regex"))
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

/// Ensure an encryption key, load `.env`, and validate the resulting config.
pub fn init() {
    ensure_encryption_key();
    ensure_admin_api_key();
    let env_path = project_root().join(".env");
    if env_path.exists() {
        dotenvy::from_path(env_path.clone()).ok();
    }
    validate_encryption_key();
}

#[cfg(test)]
mod tests {
    use super::is_valid_admin_api_key;

    #[test]
    fn admin_key_accepts_header_safe_values() {
        assert!(is_valid_admin_api_key(&"a".repeat(64)));
        assert!(is_valid_admin_api_key(&format!("{}-._~", "a".repeat(28))));
    }

    #[test]
    fn admin_key_rejects_header_injection_and_invalid_values() {
        let key = "a".repeat(64);
        assert!(!is_valid_admin_api_key(&format!("{key} with-space")));
        assert!(!is_valid_admin_api_key(&format!(
            "{key}\r\nX-Injected: yes"
        )));
        assert!(!is_valid_admin_api_key(&format!("{key}\t")));
        assert!(!is_valid_admin_api_key(
            "replace-with-a-long-random-admin-key"
        ));
        assert!(!is_valid_admin_api_key(&"é".repeat(32)));
    }
}
