//! Port of `server/src/services/health.ts`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::decrypt;
use crate::db::connection::db;
use crate::db::schema::ApiKeyRow;
use crate::providers::get_provider;

const CHECK_INTERVAL_MS: u64 = 5 * 60 * 1000; // 5 minutes
const CONSECUTIVE_FAILURES_TO_DISABLE: i64 = 3;

fn failure_count() -> &'static Mutex<HashMap<i64, i64>> {
    static FC: OnceLock<Mutex<HashMap<i64, i64>>> = OnceLock::new();
    FC.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_status(conn: &Connection, key_id: i64, status: &str) {
    conn.execute(
        "UPDATE api_keys SET status = ?1, last_checked_at = datetime('now') WHERE id = ?2",
        rusqlite::params![status, key_id],
    )
    .ok();
}

pub async fn check_key_health(key_id: i64) -> String {
    let row = {
        let conn = db().lock().await;
        conn.query_row(
            &format!(
                "SELECT {} FROM api_keys WHERE id = ?1",
                crate::db::schema::API_KEY_COLS
            ),
            rusqlite::params![key_id],
            ApiKeyRow::from_row,
        )
        .ok()
    };
    let Some(row) = row else {
        return "error".to_string();
    };
    // get_provider locks the DB for custom-provider lookup — run after the
    // guard above is released.
    let Some(provider) = get_provider(&row.platform).await else {
        return "error".to_string();
    };

    let api_key = match decrypt(&row.encrypted_key, &row.iv, &row.auth_tag) {
        Ok(k) => k,
        Err(_) => {
            // Decryption failed — the encryption key has changed since this
            // key was stored. The key data is unrecoverable; mark as invalid
            // and auto-disable.
            tracing::error!("[Health] Key {key_id} decryption failed — encryption key mismatch");
            let count = {
                let mut fc = failure_count().lock().unwrap();
                let c = fc.get(&key_id).copied().unwrap_or(0) + 1;
                fc.insert(key_id, c);
                c
            };

            {
                let conn = db().lock().await;
                set_status(&conn, key_id, "invalid");
                if count >= CONSECUTIVE_FAILURES_TO_DISABLE {
                    conn.execute(
                        "UPDATE api_keys SET enabled = 0 WHERE id = ?1",
                        rusqlite::params![key_id],
                    )
                    .ok();
                    tracing::info!(
                        "[Health] Auto-disabled key {key_id} after {count} consecutive decryption failures"
                    );
                    failure_count().lock().unwrap().remove(&key_id);
                }
            }
            return "invalid".to_string();
        }
    };

    match provider.validate_key(&api_key).await {
        Ok(is_valid) => {
            let status = if is_valid { "healthy" } else { "invalid" };
            {
                let conn = db().lock().await;
                set_status(&conn, key_id, status);
            }

            if is_valid {
                failure_count().lock().unwrap().remove(&key_id);
            } else {
                let count = {
                    let mut fc = failure_count().lock().unwrap();
                    let c = fc.get(&key_id).copied().unwrap_or(0) + 1;
                    fc.insert(key_id, c);
                    c
                };

                if count >= CONSECUTIVE_FAILURES_TO_DISABLE {
                    let conn = db().lock().await;
                    conn.execute(
                        "UPDATE api_keys SET enabled = 0 WHERE id = ?1",
                        rusqlite::params![key_id],
                    )
                    .ok();
                    tracing::info!(
                        "[Health] Auto-disabled key {key_id} after {count} consecutive failures"
                    );
                }
            }

            status.to_string()
        }
        Err(err) => {
            // Transport errors (DNS/timeout/TLS) — provider unreachable, not
            // necessarily a bad key. Mark status='error' but do NOT increment
            // the failure counter — auto-disable is reserved for confirmed
            // 401/403 (returned by validateKey as false).
            tracing::error!("[Health] Key {key_id} transport error: {}", err.message);
            let conn = db().lock().await;
            set_status(&conn, key_id, "error");
            "error".to_string()
        }
    }
}

pub async fn check_all_keys() {
    let keys: Vec<i64> = {
        let conn = db().lock().await;
        let mut stmt = conn
            .prepare("SELECT id, platform FROM api_keys WHERE enabled = 1")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };

    tracing::info!("[Health] Checking {} keys...", keys.len());

    for key in keys {
        check_key_health(key).await;
    }

    tracing::info!("[Health] Check complete.");
}

/// `startHealthChecker()` — periodic loop until the process exits.
pub fn start_health_checker() {
    tokio::spawn(async move {
        tracing::info!(
            "[Health] Starting health checker (every {}s)",
            CHECK_INTERVAL_MS / 1000
        );
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(CHECK_INTERVAL_MS));
        // Consume the immediate first tick (setInterval has no immediate fire).
        ticker.tick().await;
        loop {
            ticker.tick().await;
            check_all_keys().await;
        }
    });
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub failure_count: Vec<(i64, i64)>,
}

pub fn snapshot_health_state() -> HealthSnapshot {
    let fc = failure_count().lock().unwrap();
    HealthSnapshot {
        failure_count: fc.iter().map(|(k, v)| (*k, *v)).collect(),
    }
}

pub fn restore_health_state(snapshot: HealthSnapshot) {
    let mut fc = failure_count().lock().unwrap();
    fc.clear();
    for (k, v) in snapshot.failure_count {
        fc.insert(k, v);
    }
}
