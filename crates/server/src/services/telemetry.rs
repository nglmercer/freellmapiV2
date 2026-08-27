//! Provider/model runtime telemetry.
//!
//! Runtime performance belongs to a provider/model pair, not to the canonical
//! model quality record. Updates use an EWMA so a provider's recent behavior
//! can recover from old failures and old latency does not dominate forever.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::db::connection::db;

const EWMA_ALPHA: f64 = 0.20;

type PreviousPerformance = (
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

#[derive(Clone, Debug)]
pub enum ObservationOutcome {
    Success,
    Failure {
        status_code: Option<u16>,
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct RuntimeObservation {
    pub outcome: ObservationOutcome,
    pub latency_ms: Option<i64>,
    pub ttft_ms: Option<i64>,
    pub output_tokens: Option<i64>,
    pub generation_duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceSnapshot {
    pub platform: String,
    pub model_id: String,
    pub sample_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub rate_limit_count: i64,
    pub timeout_count: i64,
    pub server_error_count: i64,
    pub ewma_latency_ms: Option<f64>,
    pub ewma_ttft_ms: Option<f64>,
    pub ewma_output_tps: Option<f64>,
    pub ewma_output_tps_confidence: Option<f64>,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub updated_at: String,
}

pub fn output_tokens_per_second(
    output_tokens: Option<i64>,
    generation_duration_ms: Option<i64>,
    total_latency_ms: Option<i64>,
) -> (Option<f64>, f64) {
    let Some(tokens) = output_tokens.filter(|tokens| *tokens > 0) else {
        return (None, 0.0);
    };
    let (duration_ms, confidence) = match generation_duration_ms.filter(|duration| *duration > 0) {
        Some(duration) => (duration, 1.0),
        None => match total_latency_ms.filter(|duration| *duration > 0) {
            Some(duration) => (duration, 0.60),
            None => return (None, 0.0),
        },
    };
    (
        Some(tokens as f64 / (duration_ms as f64 / 1000.0)),
        confidence,
    )
}

fn ewma(previous: Option<f64>, current: Option<f64>) -> Option<f64> {
    match (previous, current) {
        (Some(previous), Some(current)) => {
            Some(previous * (1.0 - EWMA_ALPHA) + current * EWMA_ALPHA)
        }
        (None, Some(current)) => Some(current),
        (previous, None) => previous,
    }
}

fn classify_failure(outcome: &ObservationOutcome) -> (i64, i64, i64) {
    let ObservationOutcome::Failure {
        status_code,
        message,
    } = outcome
    else {
        return (0, 0, 0);
    };
    let lower = message.to_ascii_lowercase();
    let rate_limit = i64::from(
        *status_code == Some(429) || lower.contains("429") || lower.contains("rate limit"),
    );
    let timeout = i64::from(
        lower.contains("timeout") || lower.contains("timed out") || lower.contains("etimedout"),
    );
    let server_error = i64::from(status_code.is_some_and(|status| (500..=599).contains(&status)));
    (rate_limit, timeout, server_error)
}

/// Record one provider/model attempt. This function only acquires the SQLite
/// lock for local SQL and is called after the provider HTTP future completes.
pub async fn record_observation(
    platform: &str,
    model_id: &str,
    observation: RuntimeObservation,
) -> rusqlite::Result<()> {
    let success = i64::from(matches!(observation.outcome, ObservationOutcome::Success));
    let failure = i64::from(success == 0);
    let (rate_limit, timeout, server_error) = classify_failure(&observation.outcome);
    let (output_tps, tps_confidence) = output_tokens_per_second(
        observation.output_tokens,
        observation.generation_duration_ms,
        observation.latency_ms,
    );
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let conn = db().lock().await;
    let previous: Option<PreviousPerformance> = conn
        .query_row(
            "SELECT sample_count, success_count, failure_count, rate_limit_count, timeout_count, \
                    server_error_count, ewma_latency_ms, ewma_ttft_ms, ewma_output_tps, \
                    ewma_output_tps_confidence
             FROM model_performance WHERE platform = ?1 AND model_id = ?2",
            rusqlite::params![platform, model_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .ok();
    let old = previous.unwrap_or((0, 0, 0, 0, 0, 0, None, None, None, None));
    let latency = observation
        .latency_ms
        .filter(|value| *value >= 0)
        .map(|value| value as f64);
    let ttft = observation
        .ttft_ms
        .filter(|value| *value >= 0)
        .map(|value| value as f64);
    let confidence = match output_tps {
        Some(_) => old
            .9
            .map(|old| old * (1.0 - EWMA_ALPHA) + tps_confidence * EWMA_ALPHA)
            .or(Some(tps_confidence)),
        None => old.9,
    };
    let last_success = success == 1;

    conn.execute(
        "INSERT INTO model_performance (
             platform, model_id, sample_count, success_count, failure_count,
             rate_limit_count, timeout_count, server_error_count,
             ewma_latency_ms, ewma_ttft_ms, ewma_output_tps,
             ewma_output_tps_confidence, last_success_at, last_failure_at, updated_at
         ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(platform, model_id) DO UPDATE SET
             sample_count = model_performance.sample_count + 1,
             success_count = model_performance.success_count + excluded.success_count,
             failure_count = model_performance.failure_count + excluded.failure_count,
             rate_limit_count = model_performance.rate_limit_count + excluded.rate_limit_count,
             timeout_count = model_performance.timeout_count + excluded.timeout_count,
             server_error_count = model_performance.server_error_count + excluded.server_error_count,
             ewma_latency_ms = ?15,
             ewma_ttft_ms = ?16,
             ewma_output_tps = ?17,
             ewma_output_tps_confidence = ?18,
             last_success_at = CASE WHEN ?19 = 1 THEN ?14 ELSE model_performance.last_success_at END,
             last_failure_at = CASE WHEN ?19 = 0 THEN ?14 ELSE model_performance.last_failure_at END,
             updated_at = ?14",
        rusqlite::params![
            platform,
            model_id,
            success,
            failure,
            rate_limit,
            timeout,
            server_error,
            latency,
            ttft,
            output_tps,
            confidence,
            if last_success { Some(now.clone()) } else { None },
            if last_success { None::<String> } else { Some(now.clone()) },
            now,
            ewma(old.6, latency),
            ewma(old.7, ttft),
            ewma(old.8, output_tps),
            confidence,
            success,
        ],
    )?;
    if let Some(output_tps) = output_tps {
        // A new provider-local observation supersedes the previous speed
        // cohort rank. Keep the legacy sentinel in storage until the next
        // ranking refresh recalculates the rank against the full cohort; the
        // API boundary will expose this as rank = null in the meantime.
        conn.execute(
            "UPDATE models SET observed_speed_tps = ?1,
                    observed_speed_updated_at = ?2, speed_source = 'local-observed',
                    speed_rank = ?3
             WHERE platform = ?4 AND model_id = ?5",
            rusqlite::params![
                output_tps,
                now,
                crate::db::seed::UNRANKED_SPEED_PENDING,
                platform,
                model_id,
            ],
        )?;
    }
    Ok(())
}

pub fn performance_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PerformanceSnapshot> {
    Ok(PerformanceSnapshot {
        platform: row.get(0)?,
        model_id: row.get(1)?,
        sample_count: row.get(2)?,
        success_count: row.get(3)?,
        failure_count: row.get(4)?,
        rate_limit_count: row.get(5)?,
        timeout_count: row.get(6)?,
        server_error_count: row.get(7)?,
        ewma_latency_ms: row.get(8)?,
        ewma_ttft_ms: row.get(9)?,
        ewma_output_tps: row.get(10)?,
        ewma_output_tps_confidence: row.get(11)?,
        last_success_at: row.get(12)?,
        last_failure_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

pub fn load_performance(
    conn: &Connection,
    platform: &str,
    model_id: &str,
) -> rusqlite::Result<Option<PerformanceSnapshot>> {
    conn.query_row(
        "SELECT platform, model_id, sample_count, success_count, failure_count,
                rate_limit_count, timeout_count, server_error_count, ewma_latency_ms,
                ewma_ttft_ms, ewma_output_tps, ewma_output_tps_confidence,
                last_success_at, last_failure_at, updated_at
         FROM model_performance WHERE platform = ?1 AND model_id = ?2",
        rusqlite::params![platform, model_id],
        performance_from_row,
    )
    .optional()
}

pub fn reliability(successes: i64, samples: i64) -> Option<f64> {
    crate::services::rankings::routing::reliability_score(successes, samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_uses_generation_duration_when_available() {
        assert_eq!(
            output_tokens_per_second(Some(100), Some(2_000), Some(5_000)),
            (Some(50.0), 1.0)
        );
        assert_eq!(
            output_tokens_per_second(Some(100), None, Some(5_000)),
            (Some(20.0), 0.60)
        );
    }

    #[test]
    fn tps_does_not_divide_by_zero_or_invent_missing_usage() {
        assert_eq!(
            output_tokens_per_second(Some(0), Some(1_000), Some(1_000)),
            (None, 0.0)
        );
        assert_eq!(
            output_tokens_per_second(None, Some(1_000), Some(1_000)),
            (None, 0.0)
        );
        assert_eq!(
            output_tokens_per_second(Some(10), Some(0), Some(0)),
            (None, 0.0)
        );
    }

    #[test]
    fn failure_classes_are_independent() {
        assert_eq!(
            classify_failure(&ObservationOutcome::Failure {
                status_code: Some(429),
                message: "quota".into()
            }),
            (1, 0, 0)
        );
        assert_eq!(
            classify_failure(&ObservationOutcome::Failure {
                status_code: None,
                message: "Request timeout".into()
            }),
            (0, 1, 0)
        );
        assert_eq!(
            classify_failure(&ObservationOutcome::Failure {
                status_code: Some(503),
                message: "unavailable".into()
            }),
            (0, 0, 1)
        );
    }
}
