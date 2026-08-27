//! Fallback-chain management endpoints.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::routes::ranking_json;
use crate::services::router::{get_all_penalties, route_availability};

pub fn router() -> axum::Router {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/", get(get_fallback))
        .route("/", put(update_fallback))
        .route("/sort/{preset}", post(sort_fallback))
        .route("/strategy", get(get_strategy).post(set_strategy))
        .route("/token-usage", get(token_usage))
}

// ─────────────────────────────────────────────────────────────────────
// GET / — the fallback chain with dynamic penalties.
// ─────────────────────────────────────────────────────────────────────

struct FallbackItem {
    model_db_id: i64,
    priority: i64,
    manual_priority: Option<i64>,
    enabled: i64,
    platform: String,
    model_id: String,
    display_name: String,
    intelligence_rank: i64,
    speed_rank: i64,
    intelligence_score: Option<f64>,
    speed_tokens_per_sec: Option<f64>,
    ranking_source: Option<String>,
    last_ranked_at: Option<String>,
    size_label: String,
    rpm_limit: Option<i64>,
    rpd_limit: Option<i64>,
    monthly_token_budget: String,
    free_tier: i64,
    external_speed_tps: Option<f64>,
    observed_speed_tps: Option<f64>,
    quality_source: Option<String>,
    speed_source: Option<String>,
    ranking_confidence: Option<f64>,
    quality_confidence: Option<f64>,
    speed_confidence: Option<f64>,
    quality_updated_at: Option<String>,
    external_speed_updated_at: Option<String>,
    observed_speed_updated_at: Option<String>,
    canonical_model_id: Option<i64>,
    sample_count: i64,
    success_count: i64,
    rate_limit_count: i64,
    ewma_output_tps: Option<f64>,
    ewma_output_tps_confidence: Option<f64>,
    performance_updated_at: Option<String>,
}

fn fallback_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FallbackItem> {
    Ok(FallbackItem {
        model_db_id: row.get(0)?,
        priority: row.get(1)?,
        manual_priority: row.get(2)?,
        enabled: row.get::<_, i64>(3).unwrap_or(1),
        platform: row.get(4)?,
        model_id: row.get(5)?,
        display_name: row.get(6)?,
        intelligence_rank: row.get(7)?,
        speed_rank: row.get(8)?,
        intelligence_score: row.get(9)?,
        speed_tokens_per_sec: row.get(10)?,
        ranking_source: row.get(11)?,
        last_ranked_at: row.get(12)?,
        size_label: row.get::<_, String>(13).unwrap_or_default(),
        rpm_limit: row.get(14)?,
        rpd_limit: row.get(15)?,
        monthly_token_budget: row.get::<_, String>(16).unwrap_or_default(),
        free_tier: row.get::<_, i64>(17).unwrap_or(0),
        external_speed_tps: row.get(18)?,
        observed_speed_tps: row.get(19)?,
        quality_source: row.get(20)?,
        speed_source: row.get(21)?,
        ranking_confidence: row.get(22)?,
        quality_confidence: row.get(23)?,
        speed_confidence: row.get(24)?,
        quality_updated_at: row.get(25)?,
        external_speed_updated_at: row.get(26)?,
        observed_speed_updated_at: row.get(27)?,
        canonical_model_id: row.get(28)?,
        sample_count: row.get::<_, Option<i64>>(29)?.unwrap_or(0),
        success_count: row.get::<_, Option<i64>>(30)?.unwrap_or(0),
        rate_limit_count: row.get::<_, Option<i64>>(31)?.unwrap_or(0),
        ewma_output_tps: row.get(32)?,
        ewma_output_tps_confidence: row.get(33)?,
        performance_updated_at: row.get(34)?,
    })
}

async fn get_fallback() -> Response {
    let (rows, key_counts, availability_by_model) = {
        let conn = db().lock().await;
        let rows: Vec<FallbackItem> = conn
            .prepare(
                "SELECT fc.model_db_id, fc.priority, fc.manual_priority, fc.enabled, \
                 m.platform, m.model_id, m.display_name, m.intelligence_rank, m.speed_rank, m.intelligence_score, \
                 m.speed_tokens_per_sec, m.ranking_source, m.last_ranked_at, m.size_label, \
                 m.rpm_limit, m.rpd_limit, m.monthly_token_budget, m.free_tier, \
                 m.external_speed_tps, m.observed_speed_tps, m.quality_source, m.speed_source, \
                 m.ranking_confidence, m.quality_confidence, m.speed_confidence, \
                 m.quality_updated_at, m.external_speed_updated_at, m.observed_speed_updated_at, \
                 m.canonical_model_id, mp.sample_count, mp.success_count, mp.rate_limit_count, \
                 mp.ewma_output_tps, mp.ewma_output_tps_confidence, mp.updated_at \
                 FROM fallback_config fc INNER JOIN models m ON m.id = fc.model_db_id \
                 LEFT JOIN model_performance mp ON mp.platform = m.platform AND mp.model_id = m.model_id \
                 ORDER BY fc.priority ASC",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map([], fallback_item_from_row)
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        let key_counts: Vec<(String, i64)> = conn
            .prepare("SELECT platform, COUNT(*) FROM api_keys WHERE enabled = 1 GROUP BY platform")
            .ok()
            .and_then(|mut s| {
                s.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)?)))
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        let availability_by_model: HashMap<i64, f64> = rows
            .iter()
            .map(|row| {
                let availability =
                    if crate::providers::get_provider_with_conn(&conn, &row.platform).is_some() {
                        route_availability(&conn, &row.platform, &row.model_id)
                    } else {
                        0.0
                    };
                (row.model_db_id, availability)
            })
            .collect();
        (rows, key_counts, availability_by_model)
    };

    let key_count_map: HashMap<String, i64> = key_counts.into_iter().collect();
    let penalty_map: HashMap<i64, (i64, i64)> = get_all_penalties()
        .into_iter()
        .map(|p| (p.model_db_id, (p.count, p.penalty)))
        .collect();
    let balanced_scores: HashMap<i64, f64> = crate::services::rankings::routing::score_candidates(
        &rows
            .iter()
            .map(|row| {
                let (_, penalty) = penalty_map.get(&row.model_db_id).copied().unwrap_or((0, 0));
                crate::services::rankings::routing::RoutingCandidate {
                    model_db_id: row.model_db_id,
                    manual_priority: row.manual_priority.unwrap_or(row.priority),
                    quality: row.intelligence_score,
                    speed: ranking_json::effective_speed(
                        row.ewma_output_tps.or(row.observed_speed_tps),
                        row.external_speed_tps,
                        row.speed_tokens_per_sec,
                    ),
                    reliability: crate::services::rankings::routing::reliability_score(
                        row.success_count,
                        row.sample_count,
                    ),
                    availability: availability_by_model
                        .get(&row.model_db_id)
                        .copied()
                        .unwrap_or(0.0),
                    penalty,
                }
            })
            .collect::<Vec<_>>(),
        crate::services::rankings::routing::RoutingStrategy::Balanced,
    )
    .into_iter()
    .filter_map(|candidate| candidate.score.map(|score| (candidate.model_db_id, score)))
    .collect();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            let (hits, penalty) = penalty_map.get(&r.model_db_id).copied().unwrap_or((0, 0));
            let effective_speed = ranking_json::effective_speed(
                r.ewma_output_tps.or(r.observed_speed_tps),
                r.external_speed_tps,
                r.speed_tokens_per_sec,
            );
            let quality_source = r.quality_source.as_deref().or(r.ranking_source.as_deref());
            let speed_source = if r.ewma_output_tps.is_some() {
                Some("local-observed")
            } else {
                r.speed_source.as_deref().or(r.ranking_source.as_deref())
            };
            let quality = ranking_json::quality(
                r.intelligence_score,
                r.intelligence_rank,
                quality_source,
                r.quality_confidence,
                r.quality_updated_at
                    .as_deref()
                    .or(r.last_ranked_at.as_deref()),
            );
            let speed = ranking_json::speed(
                effective_speed,
                r.speed_rank,
                speed_source,
                r.ewma_output_tps_confidence.or(r.speed_confidence),
                r.performance_updated_at
                    .as_deref()
                    .or(r.observed_speed_updated_at.as_deref())
                    .or(r.external_speed_updated_at.as_deref())
                    .or(r.last_ranked_at.as_deref()),
                r.sample_count,
            );
            json!({
                "modelDbId": r.model_db_id,
                "priority": r.priority,
                "manualPriority": r.manual_priority,
                "effectivePriority": r.priority + penalty,
                "penalty": penalty,
                "rateLimitHits": hits,
                "enabled": r.enabled != 0,
                "platform": r.platform,
                "modelId": r.model_id,
                "displayName": r.display_name,
                "intelligenceRank": quality["rank"],
                "speedRank": speed["rank"],
                "intelligenceScore": r.intelligence_score,
                "speedTokensPerSec": effective_speed,
                "rankingSource": r.ranking_source,
                "lastRankedAt": r.last_ranked_at,
                "quality": quality,
                "speed": speed,
                "reliability": ranking_json::reliability(
                    r.success_count,
                    r.sample_count,
                    r.rate_limit_count,
                ),
                "rankingConfidence": ranking_json::overall_confidence(
                    r.quality_confidence,
                    r.ewma_output_tps_confidence.or(r.speed_confidence),
                    r.ranking_confidence,
                ),
                "ranked": effective_speed.is_some() || r.intelligence_score.is_some(),
                "qualityRanked": r.intelligence_score.is_some(),
                "speedRanked": effective_speed.is_some(),
                "locallyMeasured": r.sample_count > 0,
                "routing": {
                    "balancedScore": balanced_scores.get(&r.model_db_id).copied(),
                },
                "canonicalModelId": r.canonical_model_id,
                "sizeLabel": r.size_label,
                "rpmLimit": r.rpm_limit,
                "rpdLimit": r.rpd_limit,
                "monthlyTokenBudget": r.monthly_token_budget,
                "freeTier": r.free_tier != 0,
                "keyCount": key_count_map.get(&r.platform).copied().unwrap_or(0),
            })
        })
        .collect();

    Json(json!(data)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// PUT / — full replacement of the fallback chain.
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct UpdateEntry {
    model_db_id: f64,
    priority: f64,
    enabled: bool,
}

/// Zod's `getParsedType` (the `received` value in invalid_type issues):
/// JSON null reports as "null", not JS `typeof null` ("object").
fn typeof_value(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Validate the body against
/// `z.array(z.object({ modelDbId: z.number(), priority: z.number(), enabled: z.boolean() }))`.
/// Returns the parsed entries or the joined zod issue messages (`message`
/// field of each issue), preserving the established error body.
fn parse_update_entries(v: &Value) -> Result<Vec<UpdateEntry>, String> {
    let Some(arr) = v.as_array() else {
        return Err(format!("Expected array, received {}", typeof_value(v)));
    };

    let mut issues: Vec<String> = Vec::new();
    let mut entries: Vec<UpdateEntry> = Vec::new();

    for elem in arr {
        let Some(obj) = elem.as_object() else {
            issues.push(format!("Expected object, received {}", typeof_value(elem)));
            continue;
        };

        let mut element_ok = true;

        // modelDbId: z.number()
        match obj.get("modelDbId") {
            None => {
                issues.push("Required".to_string());
                element_ok = false;
            }
            Some(val) if !val.is_number() => {
                issues.push(format!("Expected number, received {}", typeof_value(val)));
                element_ok = false;
            }
            Some(_) => {}
        }
        // priority: z.number()
        match obj.get("priority") {
            None => {
                issues.push("Required".to_string());
                element_ok = false;
            }
            Some(val) if !val.is_number() => {
                issues.push(format!("Expected number, received {}", typeof_value(val)));
                element_ok = false;
            }
            Some(_) => {}
        }
        // enabled: z.boolean()
        match obj.get("enabled") {
            None => {
                issues.push("Required".to_string());
                element_ok = false;
            }
            Some(val) if !val.is_boolean() => {
                issues.push(format!("Expected boolean, received {}", typeof_value(val)));
                element_ok = false;
            }
            Some(_) => {}
        }

        if element_ok {
            entries.push(UpdateEntry {
                model_db_id: obj.get("modelDbId").and_then(Value::as_f64).unwrap_or(0.0),
                priority: obj.get("priority").and_then(Value::as_f64).unwrap_or(0.0),
                enabled: obj.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            });
        }
    }

    if !issues.is_empty() {
        return Err(issues.join(", "));
    }
    Ok(entries)
}

async fn update_fallback(bytes: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return crate::app::error_text(500, "Failed to parse JSON body"),
    };
    let entries = match parse_update_entries(&raw) {
        Ok(e) => e,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "message": message } })),
            )
                .into_response()
        }
    };

    let result = crate::db::connection::run_in_transaction(move |conn| {
        for entry in &entries {
            conn.execute(
                "UPDATE fallback_config SET priority = ?1, manual_priority = ?1, enabled = ?2 \
                 WHERE model_db_id = ?3",
                rusqlite::params![
                    entry.priority,
                    if entry.enabled { 1 } else { 0 },
                    entry.model_db_id
                ],
            )?;
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('fallback_strategy', 'manual')
             ON CONFLICT(key) DO UPDATE SET value = 'manual'",
            [],
        )?;
        Ok(())
    })
    .await;

    match result {
        Ok(_) => Json(json!({ "success": true })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// POST /sort/:preset
// ─────────────────────────────────────────────────────────────────────

/// `parseBudgetValue(s)` from fallback.ts — extract the high end of a
/// `~?N(-N)?([MK])?` budget string.
fn parse_budget_value(s: &str) -> f64 {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"~?([\d.]+)(?:-([\d.]+))?([MK])?").unwrap());
    let Some(caps) = re.captures(s) else {
        return 0.0;
    };
    let value = caps
        .get(2)
        .or_else(|| caps.get(1))
        .map(|m| m.as_str())
        .unwrap_or("");
    let high: f64 = value.parse().unwrap_or(0.0);
    let unit = match caps.get(3).map(|m| m.as_str()) {
        Some("M") => 1_000_000.0,
        Some("K") => 1_000.0,
        _ => 1.0,
    };
    high * unit
}

struct SortRow {
    id: i64,
    platform: String,
    model_id: String,
    manual_priority: i64,
    intelligence_rank: i64,
    speed_rank: i64,
    intelligence_score: Option<f64>,
    speed_tokens_per_sec: Option<f64>,
    monthly_token_budget: String,
    success_count: i64,
    sample_count: i64,
    availability: f64,
    penalty: i64,
}

fn descending_cmp(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Comparator used by the `sort/:preset` endpoint: negative `cmp`
/// means `a` sorts first. `a.id` is `model_db_id`.
fn sort_cmp(a: &SortRow, b: &SortRow, preset: &str) -> Ordering {
    let id_tiebreak = a.id.cmp(&b.id);
    match preset {
        "manual" => a.manual_priority.cmp(&b.manual_priority).then(id_tiebreak),
        "intelligence" | "quality" => {
            if a.intelligence_score.is_some() && b.intelligence_score.is_some() {
                descending_cmp(
                    a.intelligence_score.unwrap_or(0.0),
                    b.intelligence_score.unwrap_or(0.0),
                )
                .then(id_tiebreak)
            } else if a.intelligence_score.is_some() {
                Ordering::Less
            } else if b.intelligence_score.is_some() {
                Ordering::Greater
            } else {
                a.intelligence_rank
                    .cmp(&b.intelligence_rank)
                    .then(id_tiebreak)
            }
        }
        "speed" | "fastest" => {
            if a.speed_tokens_per_sec.is_some() && b.speed_tokens_per_sec.is_some() {
                descending_cmp(
                    a.speed_tokens_per_sec.unwrap_or(0.0),
                    b.speed_tokens_per_sec.unwrap_or(0.0),
                )
                .then(id_tiebreak)
            } else if a.speed_tokens_per_sec.is_some() {
                Ordering::Less
            } else if b.speed_tokens_per_sec.is_some() {
                Ordering::Greater
            } else {
                a.speed_rank.cmp(&b.speed_rank).then(id_tiebreak)
            }
        }
        "reliability" => {
            let a_rate = crate::services::rankings::routing::reliability_score(
                a.success_count,
                a.sample_count,
            );
            let b_rate = crate::services::rankings::routing::reliability_score(
                b.success_count,
                b.sample_count,
            );
            match (a_rate, b_rate) {
                (Some(a), Some(b)) => descending_cmp(a, b).then(id_tiebreak),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => id_tiebreak,
            }
        }
        _ => {
            // budget: `parseBudgetValue(b) - parseBudgetValue(a)`
            descending_cmp(
                parse_budget_value(&a.monthly_token_budget),
                parse_budget_value(&b.monthly_token_budget),
            )
            .then(id_tiebreak)
        }
    }
}

fn sort_row_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SortRow> {
    let legacy_speed: Option<f64> = row.get(7)?;
    let observed_speed: Option<f64> = row.get(8)?;
    let external_speed: Option<f64> = row.get(9)?;
    let ewma_speed: Option<f64> = row.get(10)?;
    Ok(SortRow {
        id: row.get(0)?,
        platform: row.get(1)?,
        model_id: row.get(2)?,
        manual_priority: row.get(3)?,
        intelligence_rank: row.get(4)?,
        speed_rank: row.get(5)?,
        intelligence_score: row.get(6)?,
        speed_tokens_per_sec: ranking_json::effective_speed(
            ewma_speed.or(observed_speed),
            external_speed,
            legacy_speed,
        ),
        monthly_token_budget: row.get::<_, String>(11).unwrap_or_default(),
        success_count: row.get::<_, Option<i64>>(12)?.unwrap_or(0),
        sample_count: row.get::<_, Option<i64>>(13)?.unwrap_or(0),
        availability: 1.0,
        penalty: 0,
    })
}

async fn sort_fallback(Path(preset): Path<String>) -> Response {
    if !matches!(
        preset.as_str(),
        "manual"
            | "intelligence"
            | "quality"
            | "speed"
            | "fastest"
            | "reliability"
            | "balanced"
            | "budget"
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": { "message": format!("Unknown preset: {preset}. Use: manual, quality, fastest, reliability, balanced, budget") }
            })),
        )
            .into_response();
    }

    let rows: Vec<SortRow> = {
        let conn = db().lock().await;
        let mut rows = conn
            .prepare(
            "SELECT fc.model_db_id, m.platform, m.model_id, \
             COALESCE(fc.manual_priority, fc.priority), m.intelligence_rank, m.speed_rank, \
             m.intelligence_score, m.speed_tokens_per_sec, m.observed_speed_tps, \
             m.external_speed_tps, mp.ewma_output_tps, m.monthly_token_budget, \
             mp.success_count, mp.sample_count \
             FROM fallback_config fc INNER JOIN models m ON m.id = fc.model_db_id \
             LEFT JOIN model_performance mp ON mp.platform = m.platform AND mp.model_id = m.model_id",
            )
            .ok()
            .and_then(|mut statement| {
                statement
                    .query_map([], sort_row_from_row)
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        let penalty_map: HashMap<i64, i64> = get_all_penalties()
            .into_iter()
            .map(|entry| (entry.model_db_id, entry.penalty))
            .collect();
        for row in &mut rows {
            row.penalty = penalty_map.get(&row.id).copied().unwrap_or(0);
            row.availability =
                if crate::providers::get_provider_with_conn(&conn, &row.platform).is_some() {
                    route_availability(&conn, &row.platform, &row.model_id)
                } else {
                    0.0
                };
        }
        rows
    };

    let mut sorted = rows;
    if preset == "balanced" {
        let candidates = sorted
            .iter()
            .map(|row| crate::services::rankings::routing::RoutingCandidate {
                model_db_id: row.id,
                manual_priority: row.manual_priority,
                quality: row.intelligence_score,
                speed: row.speed_tokens_per_sec,
                reliability: crate::services::rankings::routing::reliability_score(
                    row.success_count,
                    row.sample_count,
                ),
                availability: row.availability,
                penalty: row.penalty,
            })
            .collect::<Vec<_>>();
        let scores = crate::services::rankings::routing::score_candidates(
            &candidates,
            crate::services::rankings::routing::RoutingStrategy::Balanced,
        );
        let order: HashMap<i64, usize> = scores
            .iter()
            .enumerate()
            .map(|(index, score)| (score.model_db_id, index))
            .collect();
        sorted.sort_by_key(|row| order.get(&row.id).copied().unwrap_or(usize::MAX));
    } else {
        sorted.sort_by(|a, b| sort_cmp(a, b, &preset));
    }

    let preset_for_transaction = preset.clone();
    let result = crate::db::connection::run_in_transaction(move |conn| {
        for (i, row) in sorted.iter().enumerate() {
            conn.execute(
                "UPDATE fallback_config SET priority = ?1 WHERE model_db_id = ?2",
                rusqlite::params![(i as i64) + 1, row.id],
            )?;
        }
        let strategy = match preset_for_transaction.as_str() {
            "intelligence" | "quality" => "quality",
            "speed" | "fastest" => "fastest",
            "reliability" => "reliability",
            "balanced" => "balanced",
            "manual" => "manual",
            _ => "manual",
        };
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('fallback_strategy', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![strategy],
        )?;
        Ok(())
    })
    .await;

    match result {
        Ok(_) => Json(json!({ "success": true, "preset": preset })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

async fn get_strategy() -> Response {
    let strategy = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'fallback_strategy'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|value| crate::services::rankings::routing::RoutingStrategy::parse(&value))
        .unwrap_or(crate::services::rankings::routing::RoutingStrategy::Manual)
    };
    Json(json!({
        "strategy": strategy.as_str(),
        "weights": {
            "quality": crate::services::rankings::routing::QUALITY_WEIGHT,
            "reliability": crate::services::rankings::routing::RELIABILITY_WEIGHT,
            "speed": crate::services::rankings::routing::SPEED_WEIGHT,
            "availability": crate::services::rankings::routing::AVAILABILITY_WEIGHT,
        }
    }))
    .into_response()
}

async fn set_strategy(bytes: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return crate::app::error_text(400, "Failed to parse JSON body"),
    };
    let requested = raw.as_str().map(str::to_string).or_else(|| {
        raw.get("strategy")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let Some(strategy) = requested
        .as_deref()
        .and_then(crate::services::rankings::routing::RoutingStrategy::parse)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": { "message": "Unknown strategy. Use: manual, balanced, quality, fastest, reliability" }
            })),
        )
            .into_response();
    };
    let result = crate::db::connection::run_in_transaction(move |conn| {
        if strategy == crate::services::rankings::routing::RoutingStrategy::Manual {
            conn.execute(
                "UPDATE fallback_config SET priority = COALESCE(manual_priority, priority)",
                [],
            )?;
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('fallback_strategy', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![strategy.as_str()],
        )?;
        Ok(())
    })
    .await;
    match result {
        Ok(_) => Json(json!({ "success": true, "strategy": strategy.as_str() })).into_response(),
        Err(error) => crate::app::error_text(500, &error.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// GET /token-usage — per-model budget breakdown for the stacked bar.
// ─────────────────────────────────────────────────────────────────────

/// Emit a JS-compatible number: integral floats serialize as integers.
fn json_num(v: f64) -> Value {
    if v.is_finite() && v.fract() == 0.0 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

async fn token_usage() -> Response {
    let (platform_set, models, total_used) = {
        let conn = db().lock().await;
        let platforms: Vec<String> = conn
            .prepare("SELECT DISTINCT platform FROM api_keys WHERE enabled = 1")
            .ok()
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        let platform_set: HashSet<String> = platforms.into_iter().collect();

        let models: Vec<(String, String, String, String, i64)> = conn
            .prepare(
                "SELECT m.platform, m.model_id, m.display_name, m.monthly_token_budget, \
                 fc.priority FROM models m \
                 INNER JOIN fallback_config fc ON fc.model_db_id = m.id \
                 WHERE m.enabled = 1 ORDER BY fc.priority ASC",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();

        let total_used: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM requests \
                 WHERE created_at >= datetime('now', 'start of month')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        (platform_set, models, total_used)
    };

    let model_budgets: Vec<Value> = models
        .iter()
        .filter(|(platform, _, _, _, _)| platform_set.contains(platform))
        .map(|(platform, _, display_name, budget, _)| {
            json!({
                "displayName": display_name,
                "platform": platform,
                "budget": json_num(parse_budget_value(budget)),
            })
        })
        .collect();

    let total_budget: f64 = model_budgets
        .iter()
        .map(|m| m["budget"].as_f64().unwrap_or(0.0))
        .sum();

    Json(json!({
        "totalBudget": json_num(total_budget),
        "totalUsed": total_used,
        "models": model_budgets,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_value_parsing() {
        assert_eq!(parse_budget_value(""), 0.0);
        assert_eq!(parse_budget_value("500M"), 500_000_000.0);
        assert_eq!(parse_budget_value("12K"), 12_000.0);
        assert_eq!(parse_budget_value("~1M"), 1_000_000.0);
        assert_eq!(parse_budget_value("10-20M"), 20_000_000.0);
        assert_eq!(parse_budget_value("1500"), 1_500.0);
        assert_eq!(parse_budget_value("~?garbage"), 0.0);
    }

    #[test]
    fn update_entries_validation() {
        let ok = json!([
            { "modelDbId": 1, "priority": 2, "enabled": true },
            { "modelDbId": 2, "priority": 3, "enabled": false },
        ]);
        let entries = parse_update_entries(&ok).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(!entries[1].enabled);

        // Missing field anywhere → joined zod messages.
        let bad = json!([{ "modelDbId": 1, "priority": 2 }]);
        assert_eq!(parse_update_entries(&bad).unwrap_err(), "Required");

        // Wrong types joined in element order. JSON null reports as
        // `received: "null"` (zod's getParsedType), not JS typeof.
        let bad = json!([
            { "modelDbId": "x", "priority": 2, "enabled": true },
            { "modelDbId": 1, "priority": null, "enabled": 1 },
        ]);
        assert_eq!(
            parse_update_entries(&bad).unwrap_err(),
            "Expected number, received string, Expected number, received null, Expected boolean, received number"
        );

        // Non-array body.
        assert_eq!(
            parse_update_entries(&json!({})).unwrap_err(),
            "Expected array, received object"
        );
    }

    #[test]
    fn sort_comparator_intelligence() {
        let make = |id: i64, iq: Option<f64>, rank: i64| SortRow {
            id,
            platform: "test".to_string(),
            model_id: format!("model-{id}"),
            manual_priority: rank,
            intelligence_rank: rank,
            speed_rank: rank,
            intelligence_score: iq,
            speed_tokens_per_sec: None,
            monthly_token_budget: String::new(),
            success_count: 0,
            sample_count: 0,
            availability: 1.0,
            penalty: 0,
        };
        // Both ranked → higher score first.
        let a = make(1, Some(50.0), 3);
        let b = make(2, Some(90.0), 1);
        assert_eq!(sort_cmp(&a, &b, "intelligence"), Ordering::Greater);
        assert_eq!(sort_cmp(&b, &a, "intelligence"), Ordering::Less);
        // Same score → id tiebreak.
        let a2 = make(2, Some(50.0), 3);
        assert_eq!(sort_cmp(&a, &a2, "intelligence"), Ordering::Less);
        // Ranked outranks unranked.
        let u = make(3, None, 99);
        assert_eq!(sort_cmp(&a, &u, "intelligence"), Ordering::Less);
        assert_eq!(sort_cmp(&u, &a, "intelligence"), Ordering::Greater);
        // Neither ranked → lower ordinal first.
        assert_eq!(
            sort_cmp(&u, &make(4, None, 50), "intelligence"),
            Ordering::Greater
        );
    }

    #[test]
    fn sort_comparator_budget() {
        let cheap = SortRow {
            id: 1,
            platform: "test".to_string(),
            model_id: "cheap".to_string(),
            manual_priority: 1,
            intelligence_rank: 1,
            speed_rank: 1,
            intelligence_score: None,
            speed_tokens_per_sec: None,
            monthly_token_budget: "100K".to_string(),
            success_count: 0,
            sample_count: 0,
            availability: 1.0,
            penalty: 0,
        };
        let rich = SortRow {
            id: 2,
            platform: "test".to_string(),
            model_id: "rich".to_string(),
            manual_priority: 2,
            intelligence_rank: 2,
            speed_rank: 2,
            intelligence_score: None,
            speed_tokens_per_sec: None,
            monthly_token_budget: "1M".to_string(),
            success_count: 0,
            sample_count: 0,
            availability: 1.0,
            penalty: 0,
        };
        assert_eq!(sort_cmp(&cheap, &rich, "budget"), Ordering::Greater);
    }

    #[test]
    fn json_num_integral_emits_integer() {
        assert_eq!(json_num(12_000_000.0), json!(12_000_000));
        assert_eq!(json_num(0.5), json!(0.5));
        assert_eq!(json_num(-3.0), json!(-3));
    }
}
