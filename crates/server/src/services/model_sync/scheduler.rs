//! Port of `server/src/services/model-sync/scheduler.ts`.

use std::sync::atomic::{AtomicBool, Ordering};

use super::sync::sync_models;

const SYNC_INTERVAL_MS: u64 = 6 * 60 * 60 * 1000;

pub async fn run_initial_sync() {
    tracing::info!("[ModelSync] Running initial sync...");
    match sync_models().await {
        Ok(result) => {
            tracing::info!(
                "[ModelSync] Initial sync done: {} added (active), {} stored (disabled), \
                 {} updated, {} free→paid, {} paid→free",
                result.added,
                result.stored_disabled,
                result.updated,
                result.free_to_paid,
                result.paid_to_free
            );
        }
        Err(err) => {
            tracing::error!("[ModelSync] Initial sync failed: {err}");
        }
    }
}

pub fn start_sync_scheduler() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::info!(
        "[ModelSync] Scheduler started (every {}h)",
        SYNC_INTERVAL_MS / 3600000
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(SYNC_INTERVAL_MS));
        ticker.tick().await; // skip immediate first tick (setInterval semantics)
        loop {
            ticker.tick().await;
            tracing::info!("[ModelSync] Running scheduled sync...");
            match sync_models().await {
                Ok(result) => {
                    tracing::info!(
                        "[ModelSync] Sync done: {} added, {} updated, {} disabled",
                        result.added,
                        result.updated,
                        result.disabled
                    );
                }
                Err(err) => {
                    tracing::error!("[ModelSync] Scheduled sync failed: {err}");
                }
            }
        }
    });
}
