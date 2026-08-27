//! Shared presentation helpers for ranking fields.
//!
//! SQLite keeps the legacy rank columns non-null for backwards compatibility.
//! These helpers are the single API boundary that turns those columns into
//! nullable, provenance-rich values.

use serde_json::{json, Value};

use crate::db::schema::{intelligence_rank_value, speed_rank_value};
use crate::services::rankings::enrich::STALE_AFTER_MS;
use crate::services::rankings::routing::reliability_score;

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

pub fn is_stale(timestamp: Option<&str>) -> bool {
    let Some(timestamp) = timestamp else {
        return false;
    };
    let updated = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|date| date.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S").map(|date| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(date, chrono::Utc)
            })
        });
    let Ok(updated) = updated else {
        return false;
    };
    chrono::Utc::now()
        .signed_duration_since(updated)
        .num_milliseconds()
        > STALE_AFTER_MS
}

fn freshness(timestamp: Option<&str>) -> &'static str {
    match timestamp {
        None => "unknown",
        Some(timestamp) if is_stale(Some(timestamp)) => "stale",
        Some(_) => "fresh",
    }
}

pub fn quality(
    score: Option<f64>,
    raw_rank: i64,
    source: Option<&str>,
    confidence: Option<f64>,
    updated_at: Option<&str>,
) -> Value {
    let score = finite(score);
    let rank = intelligence_rank_value(raw_rank, score);
    let source = score.is_some().then_some(source).flatten();
    let updated_at = score.is_some().then_some(updated_at).flatten();
    let confidence = if score.is_some() {
        finite(confidence).unwrap_or(0.0)
    } else {
        0.0
    };
    json!({
        "score": score,
        "rank": rank,
        "source": source,
        "confidence": confidence,
        "updatedAt": updated_at,
        "status": freshness(updated_at),
        "ranked": score.is_some(),
    })
}

pub fn speed(
    tokens_per_sec: Option<f64>,
    raw_rank: i64,
    source: Option<&str>,
    confidence: Option<f64>,
    updated_at: Option<&str>,
    sample_count: i64,
) -> Value {
    let tokens_per_sec = finite(tokens_per_sec);
    let rank = speed_rank_value(raw_rank, tokens_per_sec);
    let source = tokens_per_sec.is_some().then_some(source).flatten();
    let updated_at = tokens_per_sec.is_some().then_some(updated_at).flatten();
    let confidence = if tokens_per_sec.is_some() {
        finite(confidence).unwrap_or(0.0)
    } else {
        0.0
    };
    json!({
        "tokensPerSecond": tokens_per_sec,
        "rank": rank,
        "source": source,
        "sampleCount": sample_count,
        "confidence": confidence,
        "updatedAt": updated_at,
        "status": freshness(updated_at),
        "ranked": tokens_per_sec.is_some(),
    })
}

pub fn reliability(success_count: i64, sample_count: i64, rate_limit_count: i64) -> Value {
    let success_rate = reliability_score(success_count, sample_count);
    let rate_limit_rate = if sample_count > 0 {
        Some((rate_limit_count.max(0) as f64 / sample_count as f64).min(1.0))
    } else {
        None
    };
    json!({
        "successRate": success_rate,
        "sampleCount": sample_count,
        "rateLimitRate": rate_limit_rate,
    })
}

pub fn effective_speed(
    observed_speed_tps: Option<f64>,
    external_speed_tps: Option<f64>,
    compatibility_speed_tps: Option<f64>,
) -> Option<f64> {
    finite(observed_speed_tps)
        .or_else(|| finite(external_speed_tps))
        .or_else(|| finite(compatibility_speed_tps))
}

pub fn overall_confidence(
    quality_confidence: Option<f64>,
    speed_confidence: Option<f64>,
    ranking_confidence: Option<f64>,
) -> f64 {
    let has_metric_confidence = quality_confidence.is_some() || speed_confidence.is_some();
    finite(quality_confidence)
        .into_iter()
        .chain(finite(speed_confidence))
        .reduce(f64::min)
        .or_else(|| has_metric_confidence.then(|| finite(ranking_confidence).unwrap_or(0.0)))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_sentinels_are_null_at_the_api_boundary() {
        let ranked_quality = quality(
            Some(55.8),
            12,
            Some("artificial-analysis"),
            Some(0.98),
            None,
        );
        assert_eq!(ranked_quality["rank"], json!(12));

        let unknown_quality = quality(
            None,
            99,
            Some("legacy-source"),
            Some(0.98),
            Some("2026-01-01T00:00:00Z"),
        );
        assert_eq!(unknown_quality["score"], Value::Null);
        assert_eq!(unknown_quality["rank"], Value::Null);
        assert_eq!(unknown_quality["source"], Value::Null);
        assert_eq!(unknown_quality["updatedAt"], Value::Null);
        assert_eq!(unknown_quality["confidence"], json!(0.0));
        assert_eq!(unknown_quality["ranked"], json!(false));

        let unknown_speed = speed(
            None,
            10,
            Some("artificial-analysis"),
            Some(0.98),
            Some("2026-01-01T00:00:00Z"),
            0,
        );
        assert_eq!(unknown_speed["tokensPerSecond"], Value::Null);
        assert_eq!(unknown_speed["rank"], Value::Null);
        assert_eq!(unknown_speed["source"], Value::Null);
        assert_eq!(unknown_speed["updatedAt"], Value::Null);
        assert_eq!(unknown_speed["confidence"], json!(0.0));
        assert_eq!(unknown_speed["ranked"], json!(false));
    }

    #[test]
    fn observed_speed_takes_precedence_over_external_speed() {
        assert_eq!(
            effective_speed(Some(120.0), Some(80.0), Some(40.0)),
            Some(120.0)
        );
        assert_eq!(effective_speed(None, Some(80.0), None), Some(80.0));
        assert_eq!(effective_speed(None, None, None), None);
    }

    #[test]
    fn unknown_metrics_have_zero_overall_confidence() {
        assert_eq!(overall_confidence(None, None, Some(0.98)), 0.0);
    }
}
