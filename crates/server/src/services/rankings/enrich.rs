//! Model ranking enrichment from external benchmark sources.
//!
//! Enrichments enrich the local models table with intelligence and speed
//! rankings fetched from external benchmark sources.
//!
//! Design contract:
//!   - All ranks come from real API responses. No string matching on model
//!     names, no inference from pricing/context/version tags.
//!   - Models that the source doesn't know about stay at
//!     UNRANKED_INTELLIGENCE / UNRANKED_SPEED.
//!   - Refresh is incremental: only rows where last_ranked_at is older than
//!     the source's published age (or null) are updated.

use serde::{Deserialize, Serialize};

use crate::db::connection::db;
use crate::db::seed::{UNRANKED_INTELLIGENCE, UNRANKED_SPEED};
use crate::services::rankings::match_ids::identity;
use crate::services::rankings::sources::artificial_analysis::fetch_aa_lookup;
use crate::services::rankings::types::SourceBenchmark;

const STALE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EnrichResult {
    pub scanned: i64,
    pub updated: i64,
    pub skipped: i64,
    pub source: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub errors: Vec<String>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Convert a real intelligence_score to a 1-N ordinal rank within the local
/// set of enriched rows. Higher intelligence_index = better. Sort descending,
/// find position.
fn rank_from_score(score: f64, all_scores: &[f64]) -> i64 {
    let mut sorted = all_scores.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    match sorted.iter().position(|s| *s == score) {
        None => UNRANKED_INTELLIGENCE,
        Some(idx) => idx as i64 + 1,
    }
}

/// Speed uses an inverse scale: faster = better. Sort by tokens-per-second
/// descending so position 0 = fastest.
fn rank_from_speed(tps: f64, all_tps: &[f64]) -> i64 {
    let mut sorted = all_tps.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    match sorted.iter().position(|s| *s == tps) {
        None => UNRANKED_SPEED,
        Some(idx) => idx as i64 + 1,
    }
}

/// Size label is derived from the percentile position in the cohort using
/// quartile labels so the UI can group consistently.
fn size_label_from_score(score: f64, all_scores: &[f64]) -> String {
    if all_scores.is_empty() {
        return String::new();
    }
    let mut sorted = all_scores.to_vec();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let pos = match sorted.iter().position(|s| *s == score) {
        Some(p) => p,
        None => return String::new(),
    };
    let pct = pos as f64 / (sorted.len() as f64 - 1.0).max(1.0);
    if pct <= 0.10 {
        "Frontier".to_string()
    } else if pct <= 0.35 {
        "Large".to_string()
    } else if pct <= 0.70 {
        "Medium".to_string()
    } else {
        "Small".to_string()
    }
}

/// Public entry point. Resolves the active source(s), scans the models
/// table, updates rows whose ids match a published benchmark.
pub async fn enrich_rankings() -> EnrichResult {
    let started_at = now_iso();
    let t0 = chrono::Utc::now().timestamp_millis();
    let mut errors: Vec<String> = Vec::new();

    // fetchAllLookups — artificial-analysis is the only source today.
    let mut lookups = Vec::new();
    if let Some(aa) = fetch_aa_lookup().await {
        lookups.push(("artificial-analysis".to_string(), aa));
    }
    if lookups.is_empty() {
        return EnrichResult {
            scanned: 0,
            updated: 0,
            skipped: 0,
            source: "none".to_string(),
            started_at,
            finished_at: now_iso(),
            duration_ms: chrono::Utc::now().timestamp_millis() - t0,
            errors: vec!["No ranking source returned data. Check network/keys.".to_string()],
        };
    }

    let mut conn = db().lock().await;
    let all: Vec<(i64, String, String, Option<String>)> = conn
        .prepare("SELECT id, platform, model_id, last_ranked_at FROM models")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();

    let cutoff = {
        let ms = chrono::Utc::now().timestamp_millis() - STALE_AFTER_MS;
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
            .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_default()
    };

    // Phase 1: resolve every row to a real benchmark (or null). This is O(n)
    // with strict equality lookups — no scoring / fuzzy matching.
    struct Resolved {
        id: i64,
        benchmark: SourceBenchmark,
        source: String,
    }
    let mut resolved: Vec<Resolved> = Vec::new();
    for (id, platform, model_id, last_ranked_at) in &all {
        if last_ranked_at
            .as_ref()
            .is_some_and(|t| t.as_str() > cutoff.as_str())
        {
            continue;
        }
        let ident = identity(platform, model_id);
        for (source, lookup) in &lookups {
            if let Some(benchmark) = lookup.find(&ident) {
                resolved.push(Resolved {
                    id: *id,
                    benchmark,
                    source: source.clone(),
                });
                break; // first source wins
            }
        }
    }

    // Phase 2: derive ordinal ranks from the cohort's actual scores. The
    // ranks are not stored in any source — they're computed locally so they
    // always reflect the relative ordering of the *enriched* cohort.
    let all_scores: Vec<f64> = resolved
        .iter()
        .map(|r| r.benchmark.intelligence_score)
        .collect();
    let all_tps: Vec<f64> = resolved
        .iter()
        .map(|r| r.benchmark.speed_tokens_per_sec)
        .collect();

    struct Update {
        id: i64,
        source: String,
        intelligence_score: f64,
        speed_tokens_per_sec: f64,
        intelligence_rank: i64,
        speed_rank: i64,
        size_label: String,
        last_ranked_at: String,
    }
    let last_ranked_at = now_iso();
    let updates: Vec<Update> = resolved
        .iter()
        .map(|r| Update {
            id: r.id,
            source: r.source.clone(),
            intelligence_score: r.benchmark.intelligence_score,
            speed_tokens_per_sec: r.benchmark.speed_tokens_per_sec,
            intelligence_rank: rank_from_score(r.benchmark.intelligence_score, &all_scores),
            speed_rank: rank_from_speed(r.benchmark.speed_tokens_per_sec, &all_tps),
            size_label: size_label_from_score(r.benchmark.intelligence_score, &all_scores),
            last_ranked_at: last_ranked_at.clone(),
        })
        .collect();

    // Phase 3: persist. Wrap in a transaction so a partial failure doesn't
    // leave the table with mismatched score/rank pairs.
    let mut updated = 0i64;
    let tx_result: rusqlite::Result<()> = (|| {
        let tx = conn.transaction()?;
        for u in &updates {
            tx.execute(
                "UPDATE models SET intelligence_score = ?1, speed_tokens_per_sec = ?2, \
                 intelligence_rank = ?3, speed_rank = ?4, size_label = ?5, ranking_source = ?6, \
                 last_ranked_at = ?7 WHERE id = ?8",
                rusqlite::params![
                    u.intelligence_score,
                    u.speed_tokens_per_sec,
                    u.intelligence_rank,
                    u.speed_rank,
                    u.size_label,
                    u.source,
                    u.last_ranked_at,
                    u.id
                ],
            )?;
            updated += 1;
        }
        tx.commit()?;
        Ok(())
    })();
    if let Err(err) = tx_result {
        errors.push(err.to_string());
    }

    drop(conn);

    EnrichResult {
        scanned: all.len() as i64,
        updated,
        skipped: all.len() as i64 - resolved.len() as i64,
        source: lookups
            .iter()
            .map(|(s, _)| s.as_str())
            .collect::<Vec<_>>()
            .join("+"),
        started_at,
        finished_at: now_iso(),
        duration_ms: chrono::Utc::now().timestamp_millis() - t0,
        errors,
    }
}

/// Reset every row back to UNRANKED. Used by tests and the manual override
/// endpoint when the user wants to force a full re-enrichment.
pub async fn reset_rankings() -> i64 {
    let conn = db().lock().await;
    conn.execute(
        "UPDATE models SET intelligence_rank = ?1, speed_rank = ?2, intelligence_score = NULL, \
         speed_tokens_per_sec = NULL, ranking_source = NULL, last_ranked_at = NULL",
        rusqlite::params![UNRANKED_INTELLIGENCE, UNRANKED_SPEED],
    )
    .map(|c| c as i64)
    .unwrap_or(0)
}
