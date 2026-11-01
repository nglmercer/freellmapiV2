//! Durable asynchronous video jobs.
//!
//! Every job has a local record in `video_jobs` before any paid provider call
//! is made, so a restart never loses track of a job. Jobs are refreshed from
//! the provider on access and by a periodic sweeper that also expires finished
//! jobs and purges old records.

use std::sync::Arc;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::crypto::decrypt;
use crate::db::connection::db;
use crate::providers::{get_provider_with_conn, Provider};

/// How long a completed video stays downloadable.
pub const JOB_TTL_HOURS: i64 = 24;
/// Records for finished jobs are removed after this many days.
pub const RETENTION_DAYS: i64 = 7;
/// A job stuck in `submitting` this long was interrupted before the provider
/// acknowledged it.
const SUBMITTING_TIMEOUT_MINUTES: i64 = 10;
const SWEEP_INTERVAL_SECS: u64 = 60;
/// Jobs refreshed per sweep, so one sweep never floods a provider.
const REFRESH_BATCH: i64 = 20;

pub const JOB_COLS: &str = "id, idempotency_key, platform, model_id, key_id, remote_id, status, \
    error, created_at, updated_at, expires_at";

#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: String,
    pub idempotency_key: Option<String>,
    pub platform: Option<String>,
    pub model_id: String,
    pub key_id: Option<i64>,
    pub remote_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

impl JobRow {
    pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            idempotency_key: r.get(1)?,
            platform: r.get(2)?,
            model_id: r.get(3)?,
            key_id: r.get(4)?,
            remote_id: r.get(5)?,
            status: r.get(6)?,
            error: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
            expires_at: r.get(10)?,
        })
    }

    /// Still in progress at the provider (or about to be).
    pub fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "submitting" | "queued" | "in_progress"
        )
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn cutoff(minutes_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes_ago))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn load(conn: &Connection, id: &str) -> Option<JobRow> {
    conn.query_row(
        &format!("SELECT {JOB_COLS} FROM video_jobs WHERE id = ?1"),
        rusqlite::params![id],
        JobRow::from_row,
    )
    .ok()
}

pub fn load_by_idempotency_key(conn: &Connection, key: &str) -> Option<JobRow> {
    conn.query_row(
        &format!("SELECT {JOB_COLS} FROM video_jobs WHERE idempotency_key = ?1"),
        rusqlite::params![key],
        JobRow::from_row,
    )
    .ok()
}

/// Whether `expires_at` has passed.
pub fn is_expired(job: &JobRow) -> bool {
    job.status == "expired"
        || job
            .expires_at
            .as_deref()
            .is_some_and(|at| at < now().as_str())
}

/// Public JSON shape for a job.
pub fn to_json(job: &JobRow) -> Value {
    json!({
        "id": job.id,
        "object": "video",
        "model": job.model_id,
        "status": job.status,
        "error": job.error,
        "created_at": job.created_at,
        "updated_at": job.updated_at,
        "expires_at": job.expires_at,
    })
}

/// Provider and decrypted key for a job that has been accepted upstream.
pub fn credentials(conn: &Connection, job: &JobRow) -> Option<(Arc<dyn Provider>, String)> {
    let platform = job.platform.as_deref()?;
    let key_id = job.key_id?;
    let provider = get_provider_with_conn(conn, platform)?;
    let (encrypted, iv, tag): (String, String, String) = conn
        .query_row(
            "SELECT encrypted_key, iv, auth_tag FROM api_keys WHERE id = ?1",
            rusqlite::params![key_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok()?;
    let api_key = decrypt(&encrypted, &iv, &tag).ok()?;
    Some((provider, api_key))
}

/// Asks the provider for the job's current state and stores it. Transport
/// failures leave the stored state unchanged; the next access retries.
pub async fn refresh(job_id: &str) -> Option<JobRow> {
    let (job, creds) = {
        let conn = db().lock().await;
        let job = load(&conn, job_id)?;
        if !job.is_active() || job.remote_id.is_none() {
            return Some(job);
        }
        let creds = credentials(&conn, &job);
        (job, creds)
    };
    let (Some((provider, api_key)), Some(remote_id)) = (creds, job.remote_id.clone()) else {
        return Some(job);
    };
    let Some(video) = provider.video() else {
        return Some(job);
    };
    match video.status(&api_key, &remote_id).await {
        Ok(upstream) => {
            let conn = db().lock().await;
            let expires = if upstream.status == "completed" {
                Some(expiry_from_now())
            } else {
                job.expires_at.clone()
            };
            let _ = conn.execute(
                "UPDATE video_jobs SET status = ?1, error = ?2, expires_at = ?3, updated_at = ?4 \
                 WHERE id = ?5",
                rusqlite::params![upstream.status, upstream.error, expires, now(), job_id],
            );
            load(&conn, job_id)
        }
        Err(err) => {
            tracing::warn!(job = %job_id, "video status refresh failed: {}", err.message);
            Some(job)
        }
    }
}

pub fn expiry_from_now() -> String {
    (chrono::Utc::now() + chrono::Duration::hours(JOB_TTL_HOURS))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// One maintenance pass: recover interrupted submissions, expire finished
/// jobs, purge old records, and refresh a bounded batch of active jobs.
pub async fn sweep() {
    let to_refresh: Vec<String> = {
        let conn = db().lock().await;
        let _ = conn.execute(
            "UPDATE video_jobs SET status = 'failed', updated_at = ?1, error = \
             'Interrupted before the provider accepted the job; retry with a new Idempotency-Key' \
             WHERE status = 'submitting' AND updated_at < ?2",
            rusqlite::params![now(), cutoff(SUBMITTING_TIMEOUT_MINUTES)],
        );
        let _ = conn.execute(
            "UPDATE video_jobs SET status = 'expired', updated_at = ?1 \
             WHERE status = 'completed' AND expires_at IS NOT NULL AND expires_at < ?1",
            rusqlite::params![now()],
        );
        let retention = (chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let _ = conn.execute(
            "DELETE FROM video_jobs WHERE status IN ('expired', 'failed', 'cancelled', 'deleted') \
             AND updated_at < ?1",
            rusqlite::params![retention],
        );
        let mut stmt = match conn.prepare(
            "SELECT id FROM video_jobs WHERE status IN ('queued', 'in_progress') \
             AND remote_id IS NOT NULL ORDER BY updated_at ASC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        stmt.query_map(rusqlite::params![REFRESH_BATCH], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };
    for id in to_refresh {
        refresh(&id).await;
    }
}

/// Starts the background sweeper. Runs once immediately, which recovers jobs
/// left in flight by a previous process, then periodically.
pub fn spawn_sweeper() {
    tokio::spawn(async {
        loop {
            sweep().await;
            tokio::time::sleep(std::time::Duration::from_secs(SWEEP_INTERVAL_SECS)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: &str, expires_at: Option<String>) -> JobRow {
        JobRow {
            id: "video_x".into(),
            idempotency_key: None,
            platform: None,
            model_id: "m".into(),
            key_id: None,
            remote_id: None,
            status: status.into(),
            error: None,
            created_at: now(),
            updated_at: now(),
            expires_at,
        }
    }

    #[test]
    fn only_submitting_queued_and_in_progress_are_active() {
        assert!(job("submitting", None).is_active());
        assert!(job("queued", None).is_active());
        assert!(job("in_progress", None).is_active());
        assert!(!job("completed", None).is_active());
        assert!(!job("failed", None).is_active());
    }

    #[test]
    fn expiry_uses_the_stored_deadline() {
        let past = (chrono::Utc::now() - chrono::Duration::minutes(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        assert!(is_expired(&job("completed", Some(past))));
        assert!(!is_expired(&job("completed", Some(expiry_from_now()))));
        assert!(is_expired(&job("expired", None)));
    }
}
