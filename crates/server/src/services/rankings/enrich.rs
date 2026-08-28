//! Ranking enrichment, canonical identity resolution, and source provenance.
//!
//! The refresh flow intentionally releases SQLite before every HTTP request:
//! read local state → fetch sources → match in memory → persist in a short
//! transaction. Source failures never erase the last known-good values.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::connection::db;
use crate::db::seed::{UNRANKED_INTELLIGENCE, UNRANKED_SPEED};
use crate::services::rankings::match_ids::{
    canonical_id_for_local, identity, publisher_from_model_id, ModelIdentity,
};
use crate::services::rankings::sources::artificial_analysis::ArtificialAnalysisProvider;
use crate::services::rankings::types::{ExternalBenchmark, RankingSourceProvider};
use crate::services::telemetry::PerformanceSnapshot;

pub const STALE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceRefreshSummary {
    pub fetched: i64,
    pub matched: i64,
    pub unmatched_source: i64,
    pub unmatched_local: i64,
    pub quality_updated: i64,
    pub speed_updated: i64,
    pub last_success: Option<String>,
    pub last_failure: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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
    pub source_models: i64,
    pub local_models: i64,
    pub matched_models: i64,
    /// Backwards-friendly aggregate for callers that only need the number of
    /// local models without any matched source.
    pub unmatched: i64,
    pub unmatched_local: i64,
    pub unmatched_source: i64,
    pub updated_quality: i64,
    pub updated_speed: i64,
    pub sources: BTreeMap<String, SourceRefreshSummary>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").map(|date| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(date, chrono::Utc)
            })
        })
        .ok()
}

fn cache_is_fresh(timestamp: &str) -> bool {
    let Some(updated) = parse_timestamp(timestamp) else {
        return false;
    };
    chrono::Utc::now()
        .signed_duration_since(updated)
        .num_milliseconds()
        <= STALE_AFTER_MS
}

fn source_priority(source: Option<&str>) -> i32 {
    match source {
        Some("manual") => 0,
        Some("artificial-analysis") => 1,
        Some("livebench") => 2,
        Some("local-evaluation") => 3,
        Some("local-observed") => 0,
        Some(_) => 4,
        None => i32::MAX,
    }
}

#[derive(Clone, Debug)]
struct LocalModel {
    id: i64,
    platform: String,
    model_id: String,
    canonical_model_id: i64,
    intelligence_score: Option<f64>,
    intelligence_rank: i64,
    external_speed_tps: Option<f64>,
    observed_speed_tps: Option<f64>,
    speed_tokens_per_sec: Option<f64>,
    speed_rank: i64,
    quality_source: Option<String>,
    speed_source: Option<String>,
    ranking_confidence: Option<f64>,
    quality_confidence: Option<f64>,
    speed_confidence: Option<f64>,
    quality_updated_at: Option<String>,
    external_speed_updated_at: Option<String>,
    observed_speed_updated_at: Option<String>,
    last_ranked_at: Option<String>,
    performance: Option<PerformanceSnapshot>,
}

#[derive(Clone, Debug)]
struct WorkingModel {
    local: LocalModel,
    intelligence_score: Option<f64>,
    quality_source: Option<String>,
    quality_confidence: Option<f64>,
    quality_updated_at: Option<String>,
    external_speed_tps: Option<f64>,
    external_speed_source: Option<String>,
    external_speed_confidence: Option<f64>,
    external_speed_updated_at: Option<String>,
    observed_speed_tps: Option<f64>,
    observed_speed_confidence: Option<f64>,
    observed_speed_updated_at: Option<String>,
}

impl WorkingModel {
    fn new(local: LocalModel) -> Self {
        let telemetry_speed = local
            .performance
            .as_ref()
            .and_then(|performance| performance.ewma_output_tps);
        let telemetry_updated_at = local
            .performance
            .as_ref()
            .map(|performance| performance.updated_at.clone());
        let quality_source = local
            .quality_source
            .clone()
            .or_else(|| local.ranking_source_fallback());
        let external_speed_source = local
            .speed_source
            .clone()
            .filter(|source| source != "local-observed")
            .or_else(|| quality_source.clone());
        let observed_speed_confidence = local
            .performance
            .as_ref()
            .and_then(|performance| performance.ewma_output_tps_confidence)
            .or(local.speed_confidence);
        Self {
            intelligence_score: local.intelligence_score,
            quality_source,
            quality_confidence: local.quality_confidence.or(local.ranking_confidence),
            quality_updated_at: local.quality_updated_at.clone(),
            external_speed_tps: local.external_speed_tps.or_else(|| {
                (local.observed_speed_tps.is_none())
                    .then_some(local.speed_tokens_per_sec)
                    .flatten()
            }),
            external_speed_source,
            external_speed_confidence: local.speed_confidence.or(local.ranking_confidence),
            external_speed_updated_at: local.external_speed_updated_at.clone(),
            // Prefer the provider/model EWMA when runtime telemetry exists;
            // the model column is a compatibility snapshot of the latest raw
            // observation and must not outweigh the rolling aggregate.
            observed_speed_tps: telemetry_speed.or(local.observed_speed_tps),
            observed_speed_confidence,
            observed_speed_updated_at: local
                .observed_speed_updated_at
                .clone()
                .or(telemetry_updated_at),
            local,
        }
    }

    fn effective_speed(&self) -> Option<f64> {
        self.observed_speed_tps.or(self.external_speed_tps)
    }

    fn effective_speed_source(&self) -> Option<String> {
        if self.observed_speed_tps.is_some() {
            Some("local-observed".to_string())
        } else {
            self.external_speed_source.clone()
        }
    }

    fn speed_confidence(&self) -> Option<f64> {
        if self.observed_speed_tps.is_some() {
            self.observed_speed_confidence
        } else {
            self.external_speed_confidence
        }
    }

    fn ranking_confidence(&self) -> Option<f64> {
        match (self.quality_confidence, self.speed_confidence()) {
            (Some(quality), Some(speed)) => Some(quality.min(speed)),
            (Some(quality), None) => Some(quality),
            (None, Some(speed)) => Some(speed),
            (None, None) => None,
        }
    }
}

trait RankingSourceFallback {
    fn ranking_source_fallback(&self) -> Option<String>;
}

impl RankingSourceFallback for LocalModel {
    fn ranking_source_fallback(&self) -> Option<String> {
        self.quality_source
            .clone()
            .or_else(|| self.speed_source.clone())
    }
}

fn load_performance(
    conn: &Connection,
    platform: &str,
    model_id: &str,
) -> Option<PerformanceSnapshot> {
    conn.query_row(
        "SELECT platform, model_id, sample_count, success_count, failure_count,
                rate_limit_count, timeout_count, server_error_count, ewma_latency_ms,
                ewma_ttft_ms, ewma_output_tps, ewma_output_tps_confidence,
                last_success_at, last_failure_at, updated_at
         FROM model_performance WHERE platform = ?1 AND model_id = ?2",
        rusqlite::params![platform, model_id],
        crate::services::telemetry::performance_from_row,
    )
    .ok()
}

fn ensure_identity_registry(conn: &mut Connection) -> rusqlite::Result<Vec<LocalModel>> {
    let tx = conn.transaction()?;
    let rows: Vec<(i64, String, String, String)> = tx
        .prepare("SELECT id, platform, model_id, display_name FROM models")?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (id, platform, model_id, display_name) in &rows {
        let canonical_id = canonical_id_for_local(platform, model_id);
        let publisher = publisher_from_model_id(model_id);
        tx.execute(
            "INSERT INTO canonical_models (canonical_id, display_name, publisher)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(canonical_id) DO UPDATE SET
               display_name = COALESCE(canonical_models.display_name, excluded.display_name),
               publisher = COALESCE(canonical_models.publisher, excluded.publisher),
               updated_at = datetime('now')",
            rusqlite::params![canonical_id, display_name, publisher],
        )?;
        let canonical_db_id: i64 = tx.query_row(
            "SELECT id FROM canonical_models WHERE canonical_id = ?1",
            rusqlite::params![canonical_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE models SET canonical_model_id = ?1 WHERE id = ?2",
            rusqlite::params![canonical_db_id, id],
        )?;

        let provider_source = format!("provider:{platform}");
        for alias in [model_id.clone(), identity(platform, model_id).bare] {
            tx.execute(
                "INSERT OR IGNORE INTO model_aliases
                 (canonical_model_id, source, source_model_id, confidence, verified)
                 VALUES (?1, ?2, ?3, 1.0, 1)",
                rusqlite::params![canonical_db_id, provider_source, alias],
            )?;
        }
    }
    tx.commit()?;

    let mut stmt = conn.prepare(
        "SELECT m.id, m.platform, m.model_id, m.canonical_model_id,
                m.intelligence_score, m.intelligence_rank,
                m.external_speed_tps, m.observed_speed_tps,
                m.speed_tokens_per_sec, m.speed_rank, m.quality_source, m.speed_source,
                m.ranking_confidence, m.quality_confidence, m.speed_confidence,
                m.quality_updated_at, m.external_speed_updated_at,
                m.observed_speed_updated_at, m.last_ranked_at
         FROM models m ORDER BY m.id",
    )?;
    let mut models = Vec::new();
    for row in stmt.query_map([], |row| {
        let platform: String = row.get(1)?;
        let model_id: String = row.get(2)?;
        let canonical_model_id = row.get::<_, Option<i64>>(3)?.unwrap_or(0);
        Ok(LocalModel {
            id: row.get(0)?,
            platform: platform.clone(),
            model_id: model_id.clone(),
            canonical_model_id,
            intelligence_score: row.get(4)?,
            intelligence_rank: row.get(5)?,
            external_speed_tps: row.get(6)?,
            observed_speed_tps: row.get(7)?,
            speed_tokens_per_sec: row.get(8)?,
            speed_rank: row.get(9)?,
            quality_source: row.get(10)?,
            speed_source: row.get(11)?,
            ranking_confidence: row.get(12)?,
            quality_confidence: row.get(13)?,
            speed_confidence: row.get(14)?,
            quality_updated_at: row.get(15)?,
            external_speed_updated_at: row.get(16)?,
            observed_speed_updated_at: row.get(17)?,
            last_ranked_at: row.get(18)?,
            performance: None,
        })
    })? {
        let mut model = row?;
        model.performance = load_performance(conn, &model.platform, &model.model_id);
        models.push(model);
    }
    Ok(models)
}

#[derive(Default)]
struct SourceLookup {
    full: HashMap<String, Option<usize>>,
    bare: HashMap<String, Option<usize>>,
    canonical: HashMap<String, Option<usize>>,
    canonical_models: HashMap<i64, Option<usize>>,
    ambiguous_registry_keys: HashSet<String>,
}

fn add_lookup<K: Eq + Hash>(map: &mut HashMap<K, Option<usize>>, key: K, index: usize) {
    match map.get_mut(&key) {
        None => {
            map.insert(key, Some(index));
        }
        Some(existing) if *existing == Some(index) => {}
        Some(existing) => *existing = None,
    }
}

impl SourceLookup {
    fn new(
        benchmarks: &[ExternalBenchmark],
        canonical_registry: &HashMap<String, Option<i64>>,
    ) -> Self {
        let mut lookup = Self::default();
        for (index, benchmark) in benchmarks.iter().enumerate() {
            for source_key in [
                Some(benchmark.source_id.as_str()),
                benchmark.source_slug.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                let id = identity("source", source_key);
                add_lookup(&mut lookup.full, id.lower.clone(), index);
                add_lookup(&mut lookup.bare, id.bare_lower.clone(), index);
                add_lookup(&mut lookup.canonical, id.canonical_lower.clone(), index);
                for registry_key in [
                    source_key.to_ascii_lowercase(),
                    id.bare_lower.clone(),
                    id.canonical_lower.clone(),
                ] {
                    match canonical_registry.get(&registry_key) {
                        Some(Some(canonical_model_id)) => {
                            add_lookup(&mut lookup.canonical_models, *canonical_model_id, index);
                        }
                        Some(None) => {
                            lookup.ambiguous_registry_keys.insert(registry_key);
                        }
                        None => {}
                    }
                }
            }
        }
        lookup
    }

    fn find(&self, local: &ModelIdentity, canonical_model_id: i64) -> Option<(usize, f64)> {
        let exact = self
            .full
            .get(&local.lower)
            .and_then(|index| index.map(|index| (index, 1.0)));
        let canonical_registry = self
            .canonical_models
            .get(&canonical_model_id)
            .and_then(|index| index.map(|index| (index, 0.90)));
        let normalized = if self.ambiguous_registry_keys.contains(&local.bare_lower)
            || self
                .ambiguous_registry_keys
                .contains(&local.canonical_lower)
        {
            None
        } else {
            self.bare
                .get(&local.bare_lower)
                .and_then(|index| index.map(|index| (index, 0.92)))
                .or_else(|| {
                    self.canonical
                        .get(&local.canonical_lower)
                        .and_then(|index| index.map(|index| (index, 0.85)))
                })
        };
        exact.or(canonical_registry).or(normalized)
    }
}

fn load_canonical_registry(conn: &Connection) -> rusqlite::Result<HashMap<String, Option<i64>>> {
    let mut stmt = conn.prepare("SELECT canonical_id, id FROM canonical_models")?;
    let mut registry = HashMap::new();
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (canonical_id, database_id) = row?;
        let normalized = identity("canonical", &canonical_id);
        for key in [
            canonical_id.to_ascii_lowercase(),
            normalized.bare_lower,
            normalized.canonical_lower,
        ] {
            match registry.get_mut(&key) {
                None => {
                    registry.insert(key, Some(database_id));
                }
                Some(existing) if *existing == Some(database_id) => {}
                Some(existing) => *existing = None,
            }
        }
    }
    Ok(registry)
}

#[derive(Default)]
struct SourceAliases {
    /// Curated/manual aliases are authoritative and may intentionally block
    /// an otherwise tempting normalized match.
    authoritative: HashMap<String, Option<i64>>,
    /// Automatically inferred aliases are only hints. A stale or over-broad
    /// hint must never make a safe deterministic match impossible.
    hints: HashMap<String, HashSet<i64>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SourceMatchKind {
    Deterministic,
    Hint,
}

#[derive(Clone, Copy, Debug)]
struct SourceMatch {
    benchmark_index: usize,
    confidence: f64,
    kind: SourceMatchKind,
}

fn source_aliases(conn: &Connection, source: &str) -> rusqlite::Result<SourceAliases> {
    let mut stmt = conn.prepare(
        "SELECT lower(source_model_id), canonical_model_id, source, verified
         FROM model_aliases
         WHERE source = ?1 OR source = 'manual'
         ORDER BY CASE WHEN source = 'manual' THEN 0 ELSE 1 END,
                  CASE WHEN verified != 0 THEN 0 ELSE 1 END, id ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![source], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<(String, i64, String, i64)>>>()?;
    let mut aliases = SourceAliases::default();
    for (source_model_id, canonical_model_id, alias_source, verified) in rows {
        if alias_source == "manual" || verified != 0 {
            match aliases.authoritative.get_mut(&source_model_id) {
                None => {
                    aliases
                        .authoritative
                        .insert(source_model_id, Some(canonical_model_id));
                }
                Some(existing) if *existing == Some(canonical_model_id) => {}
                Some(existing) => *existing = None,
            }
        } else {
            aliases
                .hints
                .entry(source_model_id)
                .or_default()
                .insert(canonical_model_id);
        }
    }
    Ok(aliases)
}

fn find_source_match(
    local: &LocalModel,
    local_identity: &ModelIdentity,
    benchmark: &ExternalBenchmark,
    benchmark_index: usize,
    lookup: &SourceLookup,
    aliases: &SourceAliases,
    allow_hint: bool,
) -> Option<SourceMatch> {
    let source_id = benchmark.source_id.to_ascii_lowercase();
    let source_keys = [Some(source_id), benchmark.source_slug.clone()]
        .into_iter()
        .flatten()
        .map(|key| key.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut hinted = false;
    for source_key in source_keys {
        match aliases.authoritative.get(&source_key) {
            Some(Some(canonical_model_id)) if *canonical_model_id == local.canonical_model_id => {
                return Some(SourceMatch {
                    benchmark_index,
                    confidence: 1.0,
                    kind: SourceMatchKind::Deterministic,
                });
            }
            // A conflicting reviewed alias is explicitly unresolved. Do not
            // fall through to normalized string matching for that key.
            Some(None) => return None,
            // An alias that points at a different canonical model is also a
            // decisive non-match. Falling through here could merge a sibling
            // variant merely because their normalized strings happen to
            // collide.
            Some(Some(_)) => return None,
            None => {}
        }
        if aliases
            .hints
            .get(&source_key)
            .is_some_and(|canonical_ids| canonical_ids.contains(&local.canonical_model_id))
        {
            hinted = true;
        }
    }
    if let Some((index, confidence)) = lookup.find(local_identity, local.canonical_model_id) {
        return (index == benchmark_index).then_some(SourceMatch {
            benchmark_index: index,
            confidence,
            kind: SourceMatchKind::Deterministic,
        });
    }
    (allow_hint && hinted).then_some(SourceMatch {
        benchmark_index,
        confidence: 0.98,
        kind: SourceMatchKind::Hint,
    })
}

/// Assign competition ranks (`1, 2, 2, 4`) once for a cohort. The model ID
/// is part of the deterministic tie break; ranks are not derived by searching
/// for equal floating-point values in an unsorted array.
pub fn assign_competition_ranks(entries: &[(i64, f64)]) -> HashMap<i64, i64> {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|(left_id, left), (right_id, right)| {
        right.total_cmp(left).then(left_id.cmp(right_id))
    });
    let mut ranks = HashMap::new();
    let mut previous: Option<f64> = None;
    let mut rank = 0_i64;
    for (position, (id, score)) in sorted.into_iter().enumerate() {
        if previous.is_none_or(|value| value.total_cmp(&score) != std::cmp::Ordering::Equal) {
            rank = position as i64 + 1;
            previous = Some(score);
        }
        ranks.insert(id, rank);
    }
    ranks
}

fn valid_score(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

type ManualOverride = (i64, Option<f64>, Option<f64>, f64, String);

fn apply_manual_overrides(
    conn: &Connection,
    working: &mut HashMap<i64, WorkingModel>,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT canonical_model_id, intelligence_score, speed_tokens_per_sec, confidence, fetched_at
         FROM model_benchmarks WHERE source = 'manual'",
    )?;
    let overrides: Vec<ManualOverride> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (canonical_id, quality, speed, confidence, fetched_at) in overrides {
        for model in working
            .values_mut()
            .filter(|model| model.local.canonical_model_id == canonical_id)
        {
            if let Some(quality) = valid_score(quality) {
                model.intelligence_score = Some(quality);
                model.quality_source = Some("manual".to_string());
                model.quality_confidence = Some(confidence);
                model.quality_updated_at = Some(fetched_at.clone());
            }
            if let Some(speed) = valid_score(speed) {
                model.external_speed_tps = Some(speed);
                model.external_speed_source = Some("manual".to_string());
                model.external_speed_confidence = Some(confidence);
                model.external_speed_updated_at = Some(fetched_at.clone());
            }
        }
    }
    Ok(())
}

fn persist_alias(
    tx: &rusqlite::Transaction<'_>,
    canonical_model_id: i64,
    source: &str,
    source_model_id: &str,
    confidence: f64,
    kind: SourceMatchKind,
) -> rusqlite::Result<()> {
    if kind == SourceMatchKind::Deterministic {
        // A current deterministic match may replace an old inferred hint, but
        // a reviewed/manual alias is immutable from the automatic refresh
        // path. The existing `verified` value is intentionally not included
        // in the update set.
        tx.execute(
            "INSERT INTO model_aliases
             (canonical_model_id, source, source_model_id, confidence, verified)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(source, source_model_id) DO UPDATE SET
               canonical_model_id = excluded.canonical_model_id,
               confidence = excluded.confidence
             WHERE model_aliases.verified = 0",
            rusqlite::params![canonical_model_id, source, source_model_id, confidence],
        )?;
    } else {
        tx.execute(
            "INSERT OR IGNORE INTO model_aliases
             (canonical_model_id, source, source_model_id, confidence, verified)
             VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params![canonical_model_id, source, source_model_id, confidence],
        )?;
    }
    Ok(())
}

fn persist_refresh(
    conn: &mut Connection,
    local_models: &[LocalModel],
    working: &HashMap<i64, WorkingModel>,
    source_results: &[FetchedSource],
    matches: &[MatchedBenchmark],
    unmatched_source_ids: &[(String, String)],
    now: &str,
) -> rusqlite::Result<(i64, i64, i64)> {
    let tx = conn.transaction()?;

    // Keep the complete source response separately from canonical matches.
    // `model_benchmarks` intentionally stores only matched canonical models,
    // but a full snapshot lets newly discovered local models benefit from a
    // fresh cache without another network request.
    for source in source_results
        .iter()
        .filter(|source| source.error.is_none() && !source.from_cache)
    {
        tx.execute(
            "DELETE FROM ranking_source_models WHERE source = ?1",
            rusqlite::params![source.name],
        )?;
        for benchmark in &source.benchmarks {
            let fetched_at = benchmark
                .fetched_at
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            tx.execute(
                "INSERT INTO ranking_source_models
                 (source, source_model_id, source_model_slug, intelligence_score,
                  speed_tokens_per_sec, fetched_at, raw_updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(source, source_model_id) DO UPDATE SET
                   source_model_slug = excluded.source_model_slug,
                   intelligence_score = excluded.intelligence_score,
                   speed_tokens_per_sec = excluded.speed_tokens_per_sec,
                   fetched_at = excluded.fetched_at,
                   raw_updated_at = excluded.raw_updated_at",
                rusqlite::params![
                    source.name,
                    benchmark.source_id,
                    benchmark.source_slug,
                    benchmark.intelligence_score,
                    benchmark.speed_tokens_per_sec,
                    fetched_at,
                    benchmark.raw_updated_at,
                ],
            )?;
        }
    }

    let mut seen_benchmarks: HashSet<(i64, String)> = HashSet::new();
    for matched in matches {
        let key = (matched.local.canonical_model_id, matched.source.clone());
        if !seen_benchmarks.insert(key) {
            continue;
        }
        let benchmark = &matched.benchmark;
        let fetched_at = benchmark
            .fetched_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        tx.execute(
            "INSERT INTO model_benchmarks
             (canonical_model_id, source, source_model_id, source_model_slug,
              intelligence_score, speed_tokens_per_sec, confidence, fetched_at, raw_updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(canonical_model_id, source) DO UPDATE SET
               source_model_id = excluded.source_model_id,
               source_model_slug = excluded.source_model_slug,
               intelligence_score = COALESCE(excluded.intelligence_score, model_benchmarks.intelligence_score),
               speed_tokens_per_sec = COALESCE(excluded.speed_tokens_per_sec, model_benchmarks.speed_tokens_per_sec),
               confidence = excluded.confidence,
               fetched_at = excluded.fetched_at,
               raw_updated_at = excluded.raw_updated_at",
            rusqlite::params![
                matched.local.canonical_model_id,
                matched.source,
                benchmark.source_id,
                benchmark.source_slug,
                benchmark.intelligence_score,
                benchmark.speed_tokens_per_sec,
                matched.confidence,
                fetched_at,
                benchmark.raw_updated_at,
            ],
        )?;
        for alias in [
            Some(benchmark.source_id.clone()),
            benchmark.source_slug.clone(),
        ]
        .into_iter()
        .flatten()
        {
            persist_alias(
                &tx,
                matched.local.canonical_model_id,
                &matched.source,
                &alias,
                matched.confidence,
                matched.kind,
            )?;
            tx.execute(
                "UPDATE ranking_unmatched
                 SET resolved = 1
                 WHERE source = ?1 AND source_model_id = ?2",
                rusqlite::params![matched.source, alias],
            )?;
        }
    }

    for (source, source_model_id) in unmatched_source_ids {
        tx.execute(
            "INSERT INTO ranking_unmatched
             (source, source_model_id, local_candidate, seen_at,
              first_seen_at, last_seen_at, seen_count, resolved)
             VALUES (?1, ?2, NULL, ?3, ?3, ?3, 1, 0)
             ON CONFLICT(source, source_model_id) DO UPDATE SET
               seen_at = excluded.seen_at,
               last_seen_at = excluded.last_seen_at,
               seen_count = ranking_unmatched.seen_count + 1,
               resolved = 0",
            rusqlite::params![source, source_model_id, now],
        )?;
    }

    for source in source_results {
        match &source.error {
            None => {
                if source.from_cache {
                    tx.execute(
                        "INSERT INTO ranking_source_status
                         (source, enabled, last_success, last_failure, model_count, last_error)
                         VALUES (?1, ?2, ?3, NULL, ?4, NULL)
                         ON CONFLICT(source) DO UPDATE SET
                           enabled = excluded.enabled, last_success = excluded.last_success,
                           last_failure = NULL, model_count = excluded.model_count,
                           last_error = NULL",
                        rusqlite::params![
                            source.name,
                            i64::from(source.configured),
                            source.cached_last_success.as_deref().unwrap_or(now),
                            source.benchmarks.len() as i64,
                        ],
                    )?;
                    continue;
                }
                tx.execute(
                    "INSERT INTO ranking_source_status
                     (source, enabled, last_success, last_failure, model_count, last_error)
                     VALUES (?1, 1, ?2, NULL, ?3, NULL)
                     ON CONFLICT(source) DO UPDATE SET
                       enabled = 1, last_success = excluded.last_success,
                       last_failure = NULL, model_count = excluded.model_count, last_error = NULL",
                    rusqlite::params![source.name, now, source.benchmarks.len() as i64],
                )?;
            }
            Some(error) => {
                tx.execute(
                    "INSERT INTO ranking_source_status
                     (source, enabled, last_failure, model_count, last_error)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(source) DO UPDATE SET
                       enabled = excluded.enabled, last_failure = excluded.last_failure,
                       last_error = excluded.last_error",
                    rusqlite::params![
                        source.name,
                        i64::from(source.configured),
                        now,
                        source.benchmarks.len() as i64,
                        error
                    ],
                )?;
            }
        }
    }

    // A canonical model has one quality rank even when several providers
    // expose it. Pick the highest-precedence source deterministically, then
    // use stable provider/model IDs as the tie-break for inconsistent legacy
    // rows.
    let mut quality_candidates: BTreeMap<i64, (i32, String, String, String, f64)> = BTreeMap::new();
    for model in working.values() {
        let Some(score) = valid_score(model.intelligence_score) else {
            continue;
        };
        let candidate = (
            source_priority(model.quality_source.as_deref()),
            model.quality_source.clone().unwrap_or_default(),
            model.local.platform.clone(),
            model.local.model_id.clone(),
            score,
        );
        let canonical_id = model.local.canonical_model_id;
        let should_replace = quality_candidates
            .get(&canonical_id)
            .map(|existing| {
                (&candidate.0, &candidate.1, &candidate.2, &candidate.3)
                    < (&existing.0, &existing.1, &existing.2, &existing.3)
            })
            .unwrap_or(true);
        if should_replace {
            quality_candidates.insert(canonical_id, candidate);
        }
    }
    let quality_entries: HashMap<i64, f64> = quality_candidates
        .into_iter()
        .map(|(canonical_id, (_, _, _, _, score))| (canonical_id, score))
        .collect();
    let quality_ranks = assign_competition_ranks(
        &quality_entries
            .iter()
            .map(|(id, score)| (*id, *score))
            .collect::<Vec<_>>(),
    );
    let speed_entries: Vec<(i64, f64)> = working
        .values()
        .filter_map(|model| {
            valid_score(model.effective_speed()).map(|speed| (model.local.id, speed))
        })
        .collect();
    let speed_ranks = assign_competition_ranks(&speed_entries);

    let mut changed = 0_i64;
    let mut quality_updated = 0_i64;
    let mut speed_updated = 0_i64;
    for model in local_models {
        let Some(next) = working.get(&model.id) else {
            continue;
        };
        let intelligence_rank = next
            .intelligence_score
            .and_then(|_| quality_ranks.get(&next.local.canonical_model_id).copied())
            .unwrap_or(UNRANKED_INTELLIGENCE);
        let effective_speed = next.effective_speed();
        let speed_rank = effective_speed
            .and_then(|_| speed_ranks.get(&next.local.id).copied())
            .unwrap_or(UNRANKED_SPEED);
        let speed_source = next.effective_speed_source();
        let ranking_source = match (&next.quality_source, &speed_source) {
            (Some(quality), Some(speed)) if quality == speed => Some(quality.clone()),
            (Some(quality), Some(speed)) => Some(format!("quality:{quality};speed:{speed}")),
            (Some(quality), None) => Some(quality.clone()),
            (None, Some(speed)) => Some(speed.clone()),
            (None, None) => None,
        };
        let ranking_confidence = next.ranking_confidence();
        let quality_updated_at = next.quality_updated_at.clone();
        let external_speed_updated_at = next.external_speed_updated_at.clone();
        let observed_speed_updated_at = next.observed_speed_updated_at.clone();
        let last_ranked_at = [
            quality_updated_at.clone(),
            external_speed_updated_at.clone(),
            observed_speed_updated_at.clone(),
            model.last_ranked_at.clone(),
        ]
        .into_iter()
        .flatten()
        .max();
        let changed_row = model.intelligence_score != next.intelligence_score
            || model.external_speed_tps != next.external_speed_tps
            || model.observed_speed_tps != next.observed_speed_tps
            || model.speed_tokens_per_sec != effective_speed
            || model.intelligence_rank != intelligence_rank
            || model.speed_rank != speed_rank
            || model.quality_source != next.quality_source
            || model.speed_source != speed_source
            || model.ranking_confidence != ranking_confidence;
        if changed_row {
            changed += 1;
        }
        if matches.iter().any(|matched| {
            matched.local.id == model.id && matched.benchmark.intelligence_score.is_some()
        }) {
            quality_updated += 1;
        }
        if matches.iter().any(|matched| {
            matched.local.id == model.id && matched.benchmark.speed_tokens_per_sec.is_some()
        }) {
            speed_updated += 1;
        }
        tx.execute(
            "UPDATE models SET intelligence_score = ?1, intelligence_rank = ?2,
                    external_speed_tps = ?3, observed_speed_tps = ?4,
                    speed_tokens_per_sec = ?5, speed_rank = ?6,
                    quality_source = ?7, speed_source = ?8, ranking_source = ?9,
                    ranking_confidence = ?10, quality_confidence = ?11, speed_confidence = ?12,
                    quality_updated_at = ?13, external_speed_updated_at = ?14,
                    observed_speed_updated_at = ?15, last_ranked_at = ?16
             WHERE id = ?17",
            rusqlite::params![
                next.intelligence_score,
                intelligence_rank,
                next.external_speed_tps,
                next.observed_speed_tps,
                effective_speed,
                speed_rank,
                next.quality_source,
                speed_source,
                ranking_source,
                ranking_confidence,
                next.quality_confidence,
                next.speed_confidence(),
                quality_updated_at,
                external_speed_updated_at,
                observed_speed_updated_at,
                last_ranked_at,
                model.id,
            ],
        )?;
    }
    tx.commit()?;
    Ok((changed, quality_updated, speed_updated))
}

/// Load a successful source snapshot when it is younger than the freshness
/// cutoff. A malformed timestamp is intentionally treated as unusable so a
/// normal sync repairs it with a real fetch.
fn load_fresh_cached_source(
    conn: &Connection,
    source: &str,
) -> rusqlite::Result<Option<(Vec<ExternalBenchmark>, String)>> {
    let source_status: Option<(Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT last_success, model_count
             FROM ranking_source_status WHERE source = ?1",
            rusqlite::params![source],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let status_timestamp = source_status.as_ref().and_then(|status| status.0.clone());
    let timestamp = status_timestamp
        .or_else(|| {
            conn.query_row(
                "SELECT MAX(fetched_at) FROM ranking_source_models WHERE source = ?1",
                rusqlite::params![source],
                |row| row.get(0),
            )
            .ok()
            .flatten()
        })
        .or_else(|| {
            conn.query_row(
                "SELECT MAX(fetched_at) FROM model_benchmarks WHERE source = ?1",
                rusqlite::params![source],
                |row| row.get(0),
            )
            .ok()
            .flatten()
        });
    let Some(timestamp) = timestamp else {
        return Ok(None);
    };
    if !cache_is_fresh(&timestamp) {
        return Ok(None);
    }

    let load_rows = |table: &str| -> rusqlite::Result<Vec<ExternalBenchmark>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT source_model_id, source_model_slug, intelligence_score,
                    speed_tokens_per_sec, fetched_at, raw_updated_at
             FROM {table} WHERE source = ?1 ORDER BY id"
        ))?;
        let mut benchmarks = Vec::new();
        for row in stmt.query_map(rusqlite::params![source], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })? {
            let (source_id, source_slug, quality, speed, fetched_at, raw_updated_at) = row?;
            let Some(fetched_at) = parse_timestamp(&fetched_at) else {
                continue;
            };
            benchmarks.push(ExternalBenchmark {
                source_id,
                source_slug,
                intelligence_score: quality,
                speed_tokens_per_sec: speed,
                fetched_at,
                raw_updated_at,
            });
        }
        Ok(benchmarks)
    };

    let benchmarks = load_rows("ranking_source_models").unwrap_or_default();

    if benchmarks.is_empty() {
        // An empty successful response is still a valid cache entry. A
        // non-zero model count with no full snapshot rows indicates an older
        // database, whose matched-only rows must not be treated as complete.
        if source_status.as_ref().and_then(|status| status.1) == Some(0) {
            Ok(Some((benchmarks, timestamp)))
        } else {
            Ok(None)
        }
    } else {
        Ok(Some((benchmarks, timestamp)))
    }
}

struct FetchedSource {
    name: String,
    benchmarks: Vec<ExternalBenchmark>,
    error: Option<String>,
    configured: bool,
    from_cache: bool,
    cached_last_success: Option<String>,
}

struct MatchedBenchmark {
    local: LocalModel,
    benchmark: ExternalBenchmark,
    source: String,
    confidence: f64,
    kind: SourceMatchKind,
}

fn apply_match(
    working: &mut WorkingModel,
    source: &str,
    benchmark: &ExternalBenchmark,
    confidence: f64,
    now: &str,
) -> (bool, bool) {
    let mut quality_changed = false;
    let mut speed_changed = false;
    let incoming_priority = source_priority(Some(source));
    if let Some(score) = valid_score(benchmark.intelligence_score) {
        let existing_priority = source_priority(working.quality_source.as_deref());
        if working.intelligence_score.is_none() || incoming_priority <= existing_priority {
            working.intelligence_score = Some(score);
            working.quality_source = Some(source.to_string());
            working.quality_confidence = Some(confidence);
            working.quality_updated_at = Some(now.to_string());
            quality_changed = true;
        }
    }
    if let Some(speed) = valid_score(benchmark.speed_tokens_per_sec) {
        let existing_priority = source_priority(working.external_speed_source.as_deref());
        if working.external_speed_tps.is_none() || incoming_priority <= existing_priority {
            working.external_speed_tps = Some(speed);
            working.external_speed_source = Some(source.to_string());
            working.external_speed_confidence = Some(confidence);
            working.external_speed_updated_at = Some(now.to_string());
            speed_changed = true;
        }
    }
    (quality_changed, speed_changed)
}

/// Refresh all configured ranking sources. Missing credentials are reported
/// as a source failure and do not change known-good model values.
pub async fn enrich_rankings() -> EnrichResult {
    enrich_rankings_with_force(true).await
}

/// Refresh rankings for a scheduled/model sync. A successful source snapshot
/// younger than the cutoff is reused; administrative refreshes call
/// enrich_rankings and always fetch.
pub async fn enrich_rankings_if_stale() -> EnrichResult {
    enrich_rankings_with_force(false).await
}

async fn enrich_rankings_with_force(force: bool) -> EnrichResult {
    let started_at = now_iso();
    let started_ms = chrono::Utc::now().timestamp_millis();

    let local_models = {
        let mut conn = db().lock().await;
        match ensure_identity_registry(&mut conn) {
            Ok(models) => models,
            Err(error) => {
                return EnrichResult {
                    scanned: 0,
                    updated: 0,
                    skipped: 0,
                    source: "none".to_string(),
                    started_at,
                    finished_at: now_iso(),
                    duration_ms: chrono::Utc::now().timestamp_millis() - started_ms,
                    errors: vec![format!("identity registry: {error}")],
                    ..EnrichResult::default()
                };
            }
        }
    };

    // No SQLite guard is alive while this vector of source futures executes.
    let providers: Vec<Box<dyn RankingSourceProvider>> =
        vec![Box::new(ArtificialAnalysisProvider::from_env())];
    let mut source_results = Vec::new();
    for provider in providers {
        let name = provider.name().to_string();
        let configured = name != "artificial-analysis"
            || crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY")
                .is_some_and(|key| !key.trim().is_empty());

        if !force {
            let cached = {
                let conn = db().lock().await;
                load_fresh_cached_source(&conn, &name)
            };
            match cached {
                Ok(Some((benchmarks, last_success))) => {
                    source_results.push(FetchedSource {
                        name,
                        benchmarks,
                        error: None,
                        configured,
                        from_cache: true,
                        cached_last_success: Some(last_success),
                    });
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(source = %name, %error, "Unable to read ranking cache");
                }
            }
        }

        match provider.fetch().await {
            Ok(benchmarks) => source_results.push(FetchedSource {
                name,
                benchmarks,
                error: None,
                configured,
                from_cache: false,
                cached_last_success: None,
            }),
            Err(error) => source_results.push(FetchedSource {
                name,
                benchmarks: Vec::new(),
                error: Some(error.to_string()),
                configured,
                from_cache: false,
                cached_last_success: None,
            }),
        }
    }

    let mut working: HashMap<i64, WorkingModel> = local_models
        .iter()
        .cloned()
        .map(|model| (model.id, WorkingModel::new(model)))
        .collect();
    let mut errors = Vec::new();
    let mut summaries = BTreeMap::new();
    let mut matches = Vec::new();
    let mut unmatched_source_ids = Vec::new();
    let now = now_iso();
    let mut matched_local_ids = HashSet::new();

    {
        let conn = db().lock().await;
        let canonical_registry = load_canonical_registry(&conn).unwrap_or_default();
        for source in &source_results {
            let lookup = SourceLookup::new(&source.benchmarks, &canonical_registry);
            let aliases = source_aliases(&conn, &source.name).unwrap_or_default();
            // Resolve every deterministic identity before considering inferred
            // aliases. This prevents a stale hint for one canonical model from
            // consuming a benchmark that now has deterministic evidence for a
            // different model later in the local-model list.
            let mut deterministic_matches: HashMap<usize, HashSet<i64>> = HashMap::new();
            for local in &local_models {
                let local_identity = identity(&local.platform, &local.model_id);
                if let Some(source_match) =
                    source
                        .benchmarks
                        .iter()
                        .enumerate()
                        .find_map(|(index, benchmark)| {
                            find_source_match(
                                local,
                                &local_identity,
                                benchmark,
                                index,
                                &lookup,
                                &aliases,
                                false,
                            )
                        })
                {
                    deterministic_matches
                        .entry(source_match.benchmark_index)
                        .or_default()
                        .insert(local.canonical_model_id);
                }
            }
            let mut matched_local_for_source = HashSet::new();
            let mut matched_indices = HashSet::new();
            for local in &local_models {
                let local_identity = identity(&local.platform, &local.model_id);
                let Some(source_match) =
                    source
                        .benchmarks
                        .iter()
                        .enumerate()
                        .find_map(|(index, benchmark)| {
                            let source_match = find_source_match(
                                local,
                                &local_identity,
                                benchmark,
                                index,
                                &lookup,
                                &aliases,
                                true,
                            )?;
                            if source_match.kind == SourceMatchKind::Hint
                                && deterministic_matches
                                    .get(&source_match.benchmark_index)
                                    .is_some_and(|canonical_ids| {
                                        !canonical_ids.contains(&local.canonical_model_id)
                                    })
                            {
                                None
                            } else {
                                Some(source_match)
                            }
                        })
                else {
                    continue;
                };
                let benchmark = source.benchmarks[source_match.benchmark_index].clone();
                if let Some(next) = working.get_mut(&local.id) {
                    // Preserve the source snapshot age when a fresh cached
                    // benchmark is reused. This keeps per-model freshness
                    // honest instead of making a 23-hour-old result look
                    // newly fetched on every scheduled sync.
                    let benchmark_updated_at = benchmark
                        .fetched_at
                        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                    apply_match(
                        next,
                        &source.name,
                        &benchmark,
                        source_match.confidence,
                        &benchmark_updated_at,
                    );
                }
                matched_local_ids.insert(local.id);
                matched_local_for_source.insert(local.id);
                matched_indices.insert(source_match.benchmark_index);
                matches.push(MatchedBenchmark {
                    local: local.clone(),
                    benchmark,
                    source: source.name.clone(),
                    confidence: source_match.confidence,
                    kind: source_match.kind,
                });
            }
            if !source.from_cache {
                for (index, benchmark) in source.benchmarks.iter().enumerate() {
                    if !matched_indices.contains(&index) {
                        unmatched_source_ids
                            .push((source.name.clone(), benchmark.source_id.clone()));
                        tracing::debug!(
                            source = %source.name,
                            source_model_id = %benchmark.source_id,
                            "unmatched ranking source model"
                        );
                    }
                }
            }
            let unmatched_local = local_models
                .len()
                .saturating_sub(matched_local_for_source.len());
            for local in local_models
                .iter()
                .filter(|local| !matched_local_for_source.contains(&local.id))
            {
                tracing::debug!(
                    source = %source.name,
                    local_model_id = %format!("{}:{}", local.platform, local.model_id),
                    "local model did not match ranking source"
                );
            }
            let source_matches = matched_indices.len() as i64;
            let source_unmatched = source
                .benchmarks
                .len()
                .saturating_sub(matched_indices.len()) as i64;
            let (quality_updated, speed_updated) = matches
                .iter()
                .filter(|matched| matched.source == source.name)
                .fold((0_i64, 0_i64), |(quality, speed), matched| {
                    (
                        quality + i64::from(matched.benchmark.intelligence_score.is_some()),
                        speed + i64::from(matched.benchmark.speed_tokens_per_sec.is_some()),
                    )
                });
            summaries.insert(
                source.name.clone(),
                SourceRefreshSummary {
                    fetched: source.benchmarks.len() as i64,
                    matched: source_matches,
                    unmatched_source: source_unmatched,
                    unmatched_local: unmatched_local as i64,
                    quality_updated,
                    speed_updated,
                    last_success: source
                        .cached_last_success
                        .clone()
                        .or_else(|| source.error.is_none().then(|| now.clone())),
                    last_failure: source.error.as_ref().map(|_| now.clone()),
                    error: source.error.clone(),
                },
            );
            if let Some(error) = &source.error {
                errors.push(format!("{}: {error}", source.name));
            }
        }
        // Manual benchmark records are a source with the highest precedence.
        if let Err(error) = apply_manual_overrides(&conn, &mut working) {
            errors.push(format!("manual overrides: {error}"));
        }
    }

    // Persist even when every external source failed: `persist_refresh` uses
    // the in-memory last-known values, records the failure status, and still
    // applies explicit manual overrides. It never replaces a known metric
    // with `NULL` from an unavailable source.
    let (updated, updated_quality, updated_speed) = {
        let mut conn = db().lock().await;
        match persist_refresh(
            &mut conn,
            &local_models,
            &working,
            &source_results,
            &matches,
            &unmatched_source_ids,
            &now,
        ) {
            Ok(result) => result,
            Err(error) => {
                errors.push(format!("persist rankings: {error}"));
                (0, 0, 0)
            }
        }
    };

    let source_names = source_results
        .iter()
        .filter(|source| source.error.is_none())
        .map(|source| source.name.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let matched_models = matched_local_ids.len() as i64;
    let source_models = source_results
        .iter()
        .map(|source| source.benchmarks.len() as i64)
        .sum::<i64>();
    let unmatched_source = source_results
        .iter()
        .map(|source| {
            summaries
                .get(&source.name)
                .map(|summary| summary.unmatched_source)
                .unwrap_or(0)
        })
        .sum::<i64>();
    let unmatched_local = local_models.len().saturating_sub(matched_local_ids.len()) as i64;

    EnrichResult {
        scanned: local_models.len() as i64,
        updated,
        skipped: unmatched_local,
        source: if source_names.is_empty() {
            "none".to_string()
        } else {
            source_names
        },
        started_at,
        finished_at: now_iso(),
        duration_ms: chrono::Utc::now().timestamp_millis() - started_ms,
        errors,
        source_models,
        local_models: local_models.len() as i64,
        matched_models,
        unmatched: unmatched_local,
        unmatched_local,
        unmatched_source,
        updated_quality,
        updated_speed,
        sources: summaries,
    }
}

/// Explicit administrative reset. It clears published quality/external speed
/// but retains provider telemetry and canonical identities.
pub async fn reset_rankings() -> i64 {
    let conn = db().lock().await;
    conn.execute(
        "UPDATE models SET intelligence_rank = ?1, intelligence_score = NULL,
                quality_source = NULL, quality_confidence = NULL, quality_updated_at = NULL,
                external_speed_tps = NULL, external_speed_updated_at = NULL,
                speed_source = CASE WHEN observed_speed_tps IS NULL THEN NULL ELSE 'local-observed' END,
                speed_tokens_per_sec = observed_speed_tps,
                speed_rank = CASE WHEN observed_speed_tps IS NULL THEN ?2 ELSE speed_rank END,
                ranking_source = CASE WHEN observed_speed_tps IS NULL THEN NULL ELSE 'local-observed' END,
                ranking_confidence = CASE WHEN observed_speed_tps IS NULL THEN NULL ELSE speed_confidence END,
                last_ranked_at = observed_speed_updated_at",
        rusqlite::params![UNRANKED_INTELLIGENCE, UNRANKED_SPEED],
    )
    .map(|count| count as i64)
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::rankings::routing::reliability_score;
    use crate::services::rankings::types::SourceBenchmark;

    #[test]
    fn rank_assignment_is_linear_after_one_sort() {
        let ranks = assign_competition_ranks(&[(1, 90.0), (2, 80.0), (3, 80.0), (4, 70.0)]);
        assert_eq!(ranks[&1], 1);
        assert_eq!(ranks[&2], 2);
        assert_eq!(ranks[&3], 2);
        assert_eq!(ranks[&4], 4);
    }

    #[test]
    fn source_benchmark_keeps_partial_metrics() {
        let benchmark = SourceBenchmark {
            intelligence_score: Some(52.8),
            speed_tokens_per_sec: None,
        };
        assert_eq!(benchmark.intelligence_score, Some(52.8));
        assert_eq!(benchmark.speed_tokens_per_sec, None);
    }

    #[test]
    fn curated_provider_duplicate_resolves_through_canonical_registry() {
        let benchmark = ExternalBenchmark {
            source_id: "aa-llama".to_string(),
            source_slug: Some("llama-3.3-70b-instruct".to_string()),
            intelligence_score: Some(52.8),
            speed_tokens_per_sec: None,
            fetched_at: chrono::Utc::now(),
            raw_updated_at: None,
        };
        let registry = HashMap::from([("llama-3.3-70b-instruct".to_string(), Some(7_i64))]);
        let lookup = SourceLookup::new(&[benchmark], &registry);
        let resolved = lookup.find(&identity("groq", "llama-3.3-70b-versatile"), 7);
        assert_eq!(resolved, Some((0, 0.90)));
    }

    #[test]
    fn ambiguous_publisher_registry_does_not_use_bare_source_slug() {
        let benchmark = ExternalBenchmark {
            source_id: "aa-gpt".to_string(),
            source_slug: Some("gpt-4o".to_string()),
            intelligence_score: Some(55.8),
            speed_tokens_per_sec: None,
            fetched_at: chrono::Utc::now(),
            raw_updated_at: None,
        };
        let registry = HashMap::from([("gpt-4o".to_string(), None)]);
        let lookup = SourceLookup::new(&[benchmark], &registry);
        assert!(lookup
            .find(&identity("openai", "openai/gpt-4o"), 8)
            .is_none());
    }

    #[test]
    fn unverified_alias_is_a_hint_and_does_not_veto_normalized_matching() {
        let local = LocalModel {
            id: 1,
            platform: "openrouter".to_string(),
            model_id: "openai/gpt-4o".to_string(),
            canonical_model_id: 8,
            intelligence_score: None,
            intelligence_rank: UNRANKED_INTELLIGENCE,
            external_speed_tps: None,
            observed_speed_tps: None,
            speed_tokens_per_sec: None,
            speed_rank: UNRANKED_SPEED,
            quality_source: None,
            speed_source: None,
            ranking_confidence: None,
            quality_confidence: None,
            speed_confidence: None,
            quality_updated_at: None,
            external_speed_updated_at: None,
            observed_speed_updated_at: None,
            last_ranked_at: None,
            performance: None,
        };
        let benchmark = ExternalBenchmark {
            source_id: "gpt-4o".to_string(),
            source_slug: None,
            intelligence_score: Some(55.0),
            speed_tokens_per_sec: None,
            fetched_at: chrono::Utc::now(),
            raw_updated_at: None,
        };
        let lookup = SourceLookup::new(std::slice::from_ref(&benchmark), &HashMap::new());
        let aliases = SourceAliases {
            authoritative: HashMap::new(),
            hints: HashMap::from([("gpt-4o".to_string(), HashSet::from([999_i64]))]),
        };

        let matched = find_source_match(
            &local,
            &identity(&local.platform, &local.model_id),
            &benchmark,
            0,
            &lookup,
            &aliases,
            true,
        )
        .expect("normalized identity should match");
        assert_eq!(matched.benchmark_index, 0);
        assert_eq!(matched.confidence, 0.92);
        assert_eq!(matched.kind, SourceMatchKind::Deterministic);
    }

    #[test]
    fn deterministic_alias_replaces_unverified_hint_but_not_verified_alias() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE model_aliases (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               canonical_model_id INTEGER NOT NULL,
               source TEXT NOT NULL,
               source_model_id TEXT NOT NULL,
               confidence REAL NOT NULL,
               verified INTEGER NOT NULL DEFAULT 0,
               UNIQUE(source, source_model_id)
             );",
        )
        .unwrap();

        {
            let tx = conn.transaction().unwrap();
            persist_alias(
                &tx,
                7,
                "artificial-analysis",
                "hinted-model",
                0.98,
                SourceMatchKind::Hint,
            )
            .unwrap();
            persist_alias(
                &tx,
                8,
                "artificial-analysis",
                "hinted-model",
                0.92,
                SourceMatchKind::Hint,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        {
            let tx = conn.transaction().unwrap();
            persist_alias(
                &tx,
                9,
                "artificial-analysis",
                "hinted-model",
                0.92,
                SourceMatchKind::Deterministic,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let replaced: (i64, f64, i64) = conn
            .query_row(
                "SELECT canonical_model_id, confidence, verified
                 FROM model_aliases
                 WHERE source = 'artificial-analysis' AND source_model_id = 'hinted-model'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(replaced, (9, 0.92, 0));

        conn.execute(
            "INSERT INTO model_aliases
             (canonical_model_id, source, source_model_id, confidence, verified)
             VALUES (7, 'artificial-analysis', 'verified-model', 1.0, 1)",
            [],
        )
        .unwrap();
        {
            let tx = conn.transaction().unwrap();
            persist_alias(
                &tx,
                10,
                "artificial-analysis",
                "verified-model",
                1.0,
                SourceMatchKind::Deterministic,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let verified: (i64, f64, i64) = conn
            .query_row(
                "SELECT canonical_model_id, confidence, verified
                 FROM model_aliases
                 WHERE source = 'artificial-analysis' AND source_model_id = 'verified-model'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(verified, (7, 1.0, 1));
    }

    #[test]
    fn stale_cutoff_is_one_day() {
        assert_eq!(STALE_AFTER_MS, 86_400_000);
    }

    #[test]
    fn fresh_source_cache_is_reused_but_stale_cache_is_not() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ranking_source_status (
               source TEXT PRIMARY KEY,
               last_success TEXT,
               model_count INTEGER
             );
             CREATE TABLE ranking_source_models (
               id INTEGER PRIMARY KEY,
               source TEXT NOT NULL,
               source_model_id TEXT NOT NULL,
               source_model_slug TEXT,
               intelligence_score REAL,
               speed_tokens_per_sec REAL,
               fetched_at TEXT NOT NULL,
               raw_updated_at TEXT
             );
             CREATE TABLE model_benchmarks (
               id INTEGER PRIMARY KEY,
               source TEXT NOT NULL,
               source_model_id TEXT NOT NULL,
               source_model_slug TEXT,
               intelligence_score REAL,
               speed_tokens_per_sec REAL,
               fetched_at TEXT NOT NULL,
               raw_updated_at TEXT
             );",
        )
        .unwrap();
        let fresh = now_iso();
        conn.execute(
            "INSERT INTO ranking_source_status (source, last_success, model_count)
             VALUES ('artificial-analysis', ?1, 2)",
            rusqlite::params![fresh],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ranking_source_models
             (source, source_model_id, source_model_slug, intelligence_score,
              speed_tokens_per_sec, fetched_at)
             VALUES ('artificial-analysis', 'id-1', 'model-1', 55.0, 100.0, ?1)",
            rusqlite::params![fresh],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ranking_source_models
             (source, source_model_id, source_model_slug, fetched_at)
             VALUES ('artificial-analysis', 'id-unmatched', 'new-model', ?1)",
            rusqlite::params![fresh],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO model_benchmarks
             (source, source_model_id, source_model_slug, intelligence_score,
              speed_tokens_per_sec, fetched_at)
             VALUES ('artificial-analysis', 'id-1', 'model-1', 55.0, 100.0, ?1)",
            rusqlite::params![now_iso()],
        )
        .unwrap();

        let cached = load_fresh_cached_source(&conn, "artificial-analysis")
            .unwrap()
            .expect("fresh cache");
        assert_eq!(cached.0.len(), 2);

        let stale = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        conn.execute(
            "UPDATE ranking_source_status SET last_success = ?1
             WHERE source = 'artificial-analysis'",
            rusqlite::params![stale],
        )
        .unwrap();
        assert!(load_fresh_cached_source(&conn, "artificial-analysis")
            .unwrap()
            .is_none());
    }

    #[test]
    fn reliability_is_not_perfect_after_one_sample() {
        assert!(reliability_score(1, 1).unwrap() < 1.0);
    }
}
