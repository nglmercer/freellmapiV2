//! Disk cache mirroring `getmodelsapi/src/utils/cache.ts`.
//!
//! Same directory (`.cache` under the process cwd), same `{data, timestamp,
//! ttl}` on-disk format, same key sanitization, so Rust and TS caches interop.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache directory relative to the process cwd (TS uses `process.cwd()`).
pub const CACHE_DIR: &str = ".cache";
pub const ONE_HOUR: i64 = 3_600_000;
pub const FIVE_MINUTES: i64 = 300_000;

/// On-disk entry format (identical to the TS `CacheEntry<T>`).
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub data: T,
    pub timestamp: i64,
    pub ttl: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cache_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(CACHE_DIR)
}

/// Mirror of `getCachePath`: sanitize the key to `[a-z0-9-_.]` chars, cap at
/// 200 chars, and append `.json`.
pub fn get_cache_path(key: &str) -> PathBuf {
    let safe_key: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(200)
        .collect();
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{safe_key}.json"))
}

/// Mirror of `cacheGet<T>`: read + parse + TTL check.
pub fn cache_get<T: DeserializeOwned>(key: &str) -> Option<T> {
    let path = get_cache_path(key);
    let raw = fs::read_to_string(path).ok()?;
    let entry: CacheEntry<T> = serde_json::from_str(&raw).ok()?;
    if now_ms() - entry.timestamp > entry.ttl {
        return None; // expired
    }
    Some(entry.data)
}

/// Mirror of `cacheSet<T>`: write entry, fail silently on IO errors.
pub fn cache_set<T: Serialize>(key: &str, data: &T, ttl_ms: i64) {
    let path = get_cache_path(key);
    let entry = CacheEntry {
        data,
        timestamp: now_ms(),
        ttl: ttl_ms,
    };
    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = fs::write(path, json);
    }
}

/// Mirror of `clearCache`: delete every `.json` file in the cache dir and
/// return how many were removed. (TS also only clears the disk cache.)
pub fn clear_cache() -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(cache_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            if name.ends_with(".json") && fs::remove_file(entry.path()).is_ok() {
                count += 1;
            }
        }
    }
    count
}
