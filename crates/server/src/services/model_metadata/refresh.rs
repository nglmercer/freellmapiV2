use super::{persistence, sources, types::*};
use crate::db::connection::db;
use futures::{stream, FutureExt, StreamExt};
use rusqlite::params;
use std::sync::OnceLock;
use tokio::sync::Mutex;
fn locks() -> &'static std::collections::HashMap<&'static str, Mutex<()>> {
    static LOCKS: OnceLock<std::collections::HashMap<&'static str, Mutex<()>>> = OnceLock::new();
    LOCKS.get_or_init(|| {
        sources::registry()
            .iter()
            .map(|s| (s.definition().id, Mutex::new(())))
            .collect()
    })
}
pub async fn refresh(force: bool) -> Vec<SyncResult> {
    let jobs: Vec<_> = sources::registry()
        .into_iter()
        .map(|source| refresh_source(source, force).boxed())
        .collect();
    stream::iter(jobs).buffer_unordered(2).collect().await
}
async fn refresh_source(source: Box<dyn ModelMetadataSource>, force: bool) -> SyncResult {
    let def = source.definition();
    let Some(lock) = locks().get(def.id) else {
        return SyncResult::default();
    };
    let Ok(_guard) = lock.try_lock() else {
        return SyncResult {
            source: def.id.into(),
            status: "busy".into(),
            ..Default::default()
        };
    };
    let started = chrono::Utc::now();
    let timer = std::time::Instant::now();
    let cache = {
        let conn = db().lock().await;
        let config=conn.query_row("SELECT enabled,ttl_seconds,last_success,next_attempt,etag,modified FROM metadata_sources WHERE id=?1",[def.id],|r|Ok((r.get::<_,bool>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,i64>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?))).ok();
        let Some((enabled, ttl, last, next, etag, modified)) = config else {
            return SyncResult {
                source: def.id.into(),
                status: "failed".into(),
                ..Default::default()
            };
        };
        let status = if !enabled {
            Some("disabled")
        } else if next > started.timestamp() {
            Some("backoff")
        } else if !force
            && last
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(&v).ok())
                .is_some_and(|d| (started - d.with_timezone(&chrono::Utc)).num_seconds() < ttl)
        {
            Some("cached")
        } else {
            None
        };
        if let Some(status) = status {
            return SyncResult {
                source: def.id.into(),
                status: status.into(),
                ..Default::default()
            };
        }
        let _ = conn.execute(
            "UPDATE metadata_sources SET status='refreshing' WHERE id=?1",
            [def.id],
        );
        CacheHeaders { etag, modified }
    };
    // Network, parsing and bounded concurrency are outside the database guard.
    let mut fetched = source.fetch(cache).await;
    let ingestion = if let Ok(f) = &mut fetched {
        if let Some(rows) = f.observations.take() {
            Some(persist_off_runtime(def.id, rows).await)
        } else {
            None
        }
    } else {
        None
    };
    let conn = db().lock().await;
    let mut result = match fetched {
        Ok(f) => {
            let ingested = ingestion.unwrap_or_else(|| {
                Ok(SyncResult {
                    source: def.id.into(),
                    status: "notModified".into(),
                    ..Default::default()
                })
            });
            match ingested {
                Ok(r) => {
                    let now = chrono::Utc::now().to_rfc3339();
                    let _=conn.execute("UPDATE metadata_sources SET status=?1,last_success=?2,last_error=NULL,failures=0,next_attempt=0,etag=?3,modified=?4 WHERE id=?5",params![r.status,now,f.cache.etag,f.cache.modified,def.id]);
                    if r.status == "notModified" {
                        let _ = conn.execute(
                            "UPDATE model_metadata_observations SET fetched_at=?1 WHERE source=?2",
                            params![now, def.id],
                        );
                    }
                    r
                }
                Err(_) => SyncResult {
                    source: def.id.into(),
                    status: "persistenceFailed".into(),
                    error: Some("persistenceFailed".into()),
                    ..Default::default()
                },
            }
        }
        Err(e) => {
            let failures: i64 = conn
                .query_row(
                    "SELECT failures FROM metadata_sources WHERE id=?1",
                    [def.id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let delay = if e.status == "notConfigured" {
                0
            } else {
                e.retry_after.unwrap_or_else(|| {
                    (60_i64.saturating_mul(1_i64 << failures.clamp(0, 10))).min(21600)
                        + i64::from(rand::random::<u8>())
                })
            };
            let _=conn.execute("UPDATE metadata_sources SET status=?1,last_failure=?2,last_error=?1,failures=CASE WHEN ?1='notConfigured' THEN failures ELSE failures+1 END,next_attempt=?3 WHERE id=?4",params![e.status,chrono::Utc::now().to_rfc3339(),chrono::Utc::now().timestamp()+delay,def.id]);
            SyncResult {
                source: def.id.into(),
                status: e.status.into(),
                error: Some(e.status.into()),
                ..Default::default()
            }
        }
    };
    if result.status == "persistenceFailed" {
        let _=conn.execute("UPDATE metadata_sources SET status='persistenceFailed',last_failure=?1,last_error='persistenceFailed' WHERE id=?2",params![chrono::Utc::now().to_rfc3339(),def.id]);
    }
    result.duration_ms = timer.elapsed().as_millis() as u64;
    let _=conn.execute("INSERT INTO metadata_sync_runs(source,started_at,completed_at,status,summary_json) VALUES(?1,?2,?3,?4,?5)",params![def.id,started.to_rfc3339(),chrono::Utc::now().to_rfc3339(),result.status,serde_json::to_string(&result).unwrap_or_default()]);
    result
}
pub fn start_scheduler() {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            ticker.tick().await;
            refresh(false).await;
        }
    });
}

// File-backed installations use a separate WAL writer on the blocking pool: large
// catalogs must not hold the global connection mutex or block an async executor.
async fn persist_off_runtime(
    source: &'static str,
    rows: Vec<MetadataObservation>,
) -> rusqlite::Result<SyncResult> {
    let path = {
        let conn = db().lock().await;
        conn.path()
            .filter(|p| !p.is_empty() && *p != ":memory:")
            .map(str::to_owned)
    };
    if let Some(path) = path {
        tokio::task::spawn_blocking(move || {
            let mut conn = rusqlite::Connection::open(path)?;
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
            conn.execute_batch("PRAGMA foreign_keys=ON")?;
            persistence::ingest(&mut conn, source, &rows)
        })
        .await
        .map_err(|_| rusqlite::Error::InvalidQuery)?
    } else {
        let mut conn = db().lock().await;
        persistence::ingest(&mut conn, source, &rows)
    }
}
