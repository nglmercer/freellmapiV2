//! Persistence and restoration of in-memory runtime state.

use serde::{Deserialize, Serialize};

use crate::db::connection::db_path;
use crate::services::{health, ratelimit, router};

#[derive(Serialize, Deserialize)]
struct FullSnapshot {
    version: u32,
    #[serde(rename = "savedAt")]
    saved_at: i64,
    #[serde(rename = "rateLimit")]
    rate_limit: ratelimit::RateLimitSnapshot,
    router: router::RouterSnapshot,
    health: health::HealthSnapshot,
}

fn state_file() -> std::path::PathBuf {
    let path = db_path();
    let db = std::path::Path::new(&path);
    db.parent()
        .unwrap_or(std::path::Path::new("."))
        .join("runtime-state.json")
}

pub fn save_runtime_state() -> Result<(), String> {
    let snapshot = FullSnapshot {
        version: 1,
        saved_at: chrono::Utc::now().timestamp_millis(),
        rate_limit: ratelimit::snapshot_rate_limit_state(),
        router: router::snapshot_router_state(),
        health: health::snapshot_health_state(),
    };

    let file = state_file();
    let tmp = file.with_extension("json.tmp");
    let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn restore_runtime_state() -> bool {
    let file = state_file();
    if !file.exists() {
        tracing::info!("[StatePersistence] No saved runtime state found");
        return false;
    }

    let result: Result<bool, String> = (|| {
        let raw = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let snapshot: FullSnapshot = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

        if snapshot.version != 1 {
            tracing::warn!("[StatePersistence] Unknown state version, skipping restore");
            return Ok(false);
        }

        let age_minutes =
            (chrono::Utc::now().timestamp_millis() - snapshot.saved_at) as f64 / 60_000.0;
        if age_minutes > 60.0 {
            tracing::info!(
                "[StatePersistence] Saved state is {}min old, discarding stale state",
                age_minutes.round()
            );
            std::fs::remove_file(&file).ok();
            return Ok(false);
        }

        ratelimit::restore_rate_limit_state(snapshot.rate_limit);
        router::restore_router_state(snapshot.router);
        health::restore_health_state(snapshot.health);

        tracing::info!(
            "[StatePersistence] Restored runtime state (saved {:.1}min ago)",
            age_minutes
        );
        Ok(true)
    })();

    match result {
        Ok(restored) => restored,
        Err(err) => {
            tracing::error!("[StatePersistence] Failed to restore state: {err}");
            false
        }
    }
}

pub fn clear_runtime_state() {
    let file = state_file();
    if file.exists() {
        std::fs::remove_file(&file).ok();
    }
}

/// `startPeriodicSave(30_000)`
pub fn start_periodic_save(interval_ms: u64) {
    static STARTED: std::sync::OnceLock<std::sync::Mutex<bool>> = std::sync::OnceLock::new();
    let started = STARTED.get_or_init(|| std::sync::Mutex::new(false));
    {
        let mut s = started.lock().unwrap();
        if *s {
            return;
        }
        *s = true;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
        ticker.tick().await; // skip immediate first tick
        loop {
            ticker.tick().await;
            if let Err(e) = save_runtime_state() {
                tracing::error!("[StatePersistence] Periodic save failed: {e}");
            }
        }
    });
}
