//! Benchmark-source credentials set from the dashboard.
//!
//! The Artificial Analysis key is stored encrypted in the settings table, the
//! same way provider keys are, so no configuration file is needed. A copy lives in
//! memory so synchronous callers can check it. `ARTIFICIAL_ANALYSIS_API_KEY`
//! remains an optional override for deployments that prefer the environment.

use std::sync::RwLock;

use serde_json::{json, Value};

use crate::crypto;
use crate::db::connection::db;

const AA_SETTING: &str = "integrations.artificial_analysis";
const AA_ENV: &str = "ARTIFICIAL_ANALYSIS_API_KEY";

static AA_KEY: RwLock<Option<String>> = RwLock::new(None);

/// Where the active Artificial Analysis key comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    Dashboard,
    Environment,
    None,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Environment => "environment",
            Self::None => "none",
        }
    }
}

fn cached() -> Option<String> {
    AA_KEY
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn usable(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The key the benchmark sources should use, if any.
pub fn artificial_analysis_key() -> Option<String> {
    usable(cached()).or_else(|| usable(crate::env::env_string(AA_ENV)))
}

pub fn artificial_analysis_source() -> KeySource {
    if usable(cached()).is_some() {
        KeySource::Dashboard
    } else if usable(crate::env::env_string(AA_ENV)).is_some() {
        KeySource::Environment
    } else {
        KeySource::None
    }
}

pub fn artificial_analysis_configured() -> bool {
    artificial_analysis_key().is_some()
}

/// Masks a secret for display: first and last four characters only.
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "••••••••".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}••••••••{tail}")
}

/// Status JSON for the dashboard. It never includes the key itself.
pub fn artificial_analysis_status() -> Value {
    let key = artificial_analysis_key();
    json!({
        "id": "artificial-analysis",
        "configured": key.is_some(),
        "source": artificial_analysis_source().as_str(),
        "maskedKey": key.as_deref().map(mask),
    })
}

/// Stores the key encrypted and makes it active immediately.
pub async fn save_artificial_analysis_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("key must not be empty".into());
    }
    let (encrypted, iv, auth_tag) = crypto::encrypt(key);
    let value = json!({ "encrypted": encrypted, "iv": iv, "authTag": auth_tag }).to_string();
    {
        let conn = db().lock().await;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![AA_SETTING, value],
        )
        .map_err(|e| e.to_string())?;
    }
    *AA_KEY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(key.to_string());
    Ok(())
}

/// Removes the stored key. An environment override, if set, still applies.
pub async fn clear_artificial_analysis_key() -> Result<(), String> {
    {
        let conn = db().lock().await;
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![AA_SETTING],
        )
        .map_err(|e| e.to_string())?;
    }
    *AA_KEY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    Ok(())
}

/// Loads the stored key into memory. Call after the database is ready.
pub async fn load_from_db() {
    let stored: Option<String> = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![AA_SETTING],
            |row| row.get(0),
        )
        .ok()
    };
    let key = stored.and_then(|raw| {
        let parsed: Value = serde_json::from_str(&raw).ok()?;
        crypto::decrypt(
            parsed.get("encrypted")?.as_str()?,
            parsed.get("iv")?.as_str()?,
            parsed.get("authTag")?.as_str()?,
        )
        .ok()
    });
    *AA_KEY
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = key;
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn mask_hides_the_middle_of_long_keys() {
        assert_eq!(mask("abcd1234efgh5678"), "abcd••••••••5678");
        assert_eq!(mask("short"), "••••••••");
    }
}
