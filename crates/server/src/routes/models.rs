//! Model catalog, synchronization, filtering, and ranking endpoints.

use std::collections::{HashMap, HashSet};

use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rusqlite::types::Value as SqlValue;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::routes::ranking_json;
use crate::services::model_sync::sync_models;
use crate::services::pricing::{calculate_cost, get_all_pricing};
use crate::services::rankings::{enrich_rankings, reset_rankings};
use crate::services::router::route_availability;

pub fn router() -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", get(list_models))
        .route("/search", get(search_models))
        .route("/pricing", get(get_pricing))
        .route("/pricing/calculate", post(calculate_pricing))
        .route("/sync", post(sync_models_api))
        .route("/sync/status", get(sync_status))
        .route("/sync/history", get(sync_history))
        .route("/sync/changes/{logId}", get(sync_changes))
        .route("/enrich-rankings", post(enrich_rankings_api))
        .route("/rankings/sync", post(enrich_rankings_api))
        .route("/rankings/overrides", post(upsert_ranking_override))
        .route("/rankings/aliases", post(upsert_ranking_alias))
        .route("/enrich-rankings/status", get(rankings_status))
        .route("/ranking-sources", get(ranking_sources))
        .route("/reset-rankings", post(reset_rankings_api))
        .route("/disable-all", post(disable_all))
        .route("/enable-all", post(enable_all))
        .route("/enable-free", post(enable_free))
}

// ─────────────────────────────────────────────────────────────────────
// GET / — list all models with availability info (left join fallback).
// ─────────────────────────────────────────────────────────────────────

struct ModelListItem {
    id: i64,
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
    tpm_limit: Option<i64>,
    tpd_limit: Option<i64>,
    monthly_token_budget: String,
    context_window: Option<i64>,
    enabled: i64,
    priority: Option<i64>,
    fallback_enabled: Option<i64>,
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

fn model_list_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelListItem> {
    Ok(ModelListItem {
        id: row.get(0)?,
        platform: row.get(1)?,
        model_id: row.get(2)?,
        display_name: row.get(3)?,
        intelligence_rank: row.get(4)?,
        speed_rank: row.get(5)?,
        intelligence_score: row.get(6)?,
        speed_tokens_per_sec: row.get(7)?,
        ranking_source: row.get(8)?,
        last_ranked_at: row.get(9)?,
        size_label: row.get::<_, String>(10).unwrap_or_default(),
        rpm_limit: row.get(11)?,
        rpd_limit: row.get(12)?,
        tpm_limit: row.get(13)?,
        tpd_limit: row.get(14)?,
        monthly_token_budget: row.get::<_, String>(15).unwrap_or_default(),
        context_window: row.get(16)?,
        enabled: row.get::<_, i64>(17).unwrap_or(1),
        priority: row.get(18)?,
        fallback_enabled: row.get(19)?,
        external_speed_tps: row.get(20)?,
        observed_speed_tps: row.get(21)?,
        quality_source: row.get(22)?,
        speed_source: row.get(23)?,
        ranking_confidence: row.get(24)?,
        quality_confidence: row.get(25)?,
        speed_confidence: row.get(26)?,
        quality_updated_at: row.get(27)?,
        external_speed_updated_at: row.get(28)?,
        observed_speed_updated_at: row.get(29)?,
        canonical_model_id: row.get(30)?,
        sample_count: row.get::<_, Option<i64>>(31)?.unwrap_or(0),
        success_count: row.get::<_, Option<i64>>(32)?.unwrap_or(0),
        rate_limit_count: row.get::<_, Option<i64>>(33)?.unwrap_or(0),
        ewma_output_tps: row.get(34)?,
        ewma_output_tps_confidence: row.get(35)?,
        performance_updated_at: row.get(36)?,
    })
}

async fn list_models() -> Response {
    let (rows, key_counts) = {
        let conn = db().lock().await;
        let rows: Vec<ModelListItem> = conn
            .prepare(
                "SELECT m.id, m.platform, m.model_id, m.display_name, m.intelligence_rank, \
                 m.speed_rank, m.intelligence_score, m.speed_tokens_per_sec, m.ranking_source, \
                 m.last_ranked_at, m.size_label, m.rpm_limit, m.rpd_limit, m.tpm_limit, \
                 m.tpd_limit, m.monthly_token_budget, m.context_window, m.enabled, \
                 fc.priority, fc.enabled, m.external_speed_tps, m.observed_speed_tps, \
                 m.quality_source, m.speed_source, m.ranking_confidence, m.quality_confidence, \
                 m.speed_confidence, m.quality_updated_at, m.external_speed_updated_at, \
                 m.observed_speed_updated_at, m.canonical_model_id, mp.sample_count, \
                 mp.success_count, mp.rate_limit_count, mp.ewma_output_tps, \
                 mp.ewma_output_tps_confidence, mp.updated_at \
                 FROM models m LEFT JOIN fallback_config fc ON fc.model_db_id = m.id \
                 LEFT JOIN model_performance mp ON mp.platform = m.platform AND mp.model_id = m.model_id \
                 ORDER BY COALESCE(fc.priority, m.intelligence_rank) ASC",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map([], model_list_from_row)
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
        (rows, key_counts)
    };

    let key_count_map: HashMap<String, i64> = key_counts.into_iter().collect();
    let platforms: HashSet<String> = rows.iter().map(|r| r.platform.clone()).collect();
    let mut has_provider_map: HashMap<String, bool> = HashMap::new();
    for p in &platforms {
        has_provider_map.insert(p.clone(), crate::providers::has_provider(p).await);
    }
    let availability_by_model: HashMap<i64, f64> = {
        let conn = db().lock().await;
        rows.iter()
            .map(|row| {
                let availability = if has_provider_map
                    .get(&row.platform)
                    .copied()
                    .unwrap_or(false)
                {
                    route_availability(&conn, &row.platform, &row.model_id)
                } else {
                    0.0
                };
                (row.id, availability)
            })
            .collect()
    };
    let penalty_map: HashMap<i64, i64> = crate::services::router::get_all_penalties()
        .into_iter()
        .map(|entry| (entry.model_db_id, entry.penalty))
        .collect();
    let balanced_scores: HashMap<i64, f64> = crate::services::rankings::routing::score_candidates(
        &rows
            .iter()
            .map(|row| crate::services::rankings::routing::RoutingCandidate {
                model_db_id: row.id,
                manual_priority: row.priority.unwrap_or(i64::MAX),
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
                availability: availability_by_model.get(&row.id).copied().unwrap_or(0.0),
                penalty: penalty_map.get(&row.id).copied().unwrap_or(0),
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
                "id": r.id,
                "platform": r.platform,
                "modelId": r.model_id,
                "displayName": r.display_name,
                "intelligenceRank": quality["rank"],
                "speedRank": speed["rank"],
                "intelligenceScore": r.intelligence_score,
                "speedTokensPerSec": ranking_json::effective_speed(
                    r.ewma_output_tps.or(r.observed_speed_tps),
                    r.external_speed_tps,
                    r.speed_tokens_per_sec,
                ),
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
                "canonicalModelId": r.canonical_model_id,
                "ranked": effective_speed.is_some() || r.intelligence_score.is_some(),
                "qualityRanked": r.intelligence_score.is_some(),
                "speedRanked": effective_speed.is_some(),
                "locallyMeasured": r.sample_count > 0,
                "routing": {
                    "balancedScore": balanced_scores.get(&r.id).copied(),
                },
                "sizeLabel": r.size_label,
                "rpmLimit": r.rpm_limit,
                "rpdLimit": r.rpd_limit,
                "tpmLimit": r.tpm_limit,
                "tpdLimit": r.tpd_limit,
                "monthlyTokenBudget": r.monthly_token_budget,
                "contextWindow": r.context_window,
                "enabled": r.enabled != 0,
                "priority": r.priority,
                "fallbackEnabled": r.fallback_enabled == Some(1),
                "hasProvider": has_provider_map.get(&r.platform).copied().unwrap_or(false),
                "keyCount": key_count_map.get(&r.platform).copied().unwrap_or(0),
            })
        })
        .collect();

    Json(json!(data)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /search — query the catalog with provider, gateway, free-tier, and text
// filters.
// ─────────────────────────────────────────────────────────────────────

fn build_search_conditions(query: &HashMap<String, String>) -> (String, Vec<SqlValue>) {
    let mut conditions = vec!["enabled = 1".to_string()];
    let mut values: Vec<SqlValue> = Vec::new();
    if let Some(provider) = query.get("provider").filter(|s| !s.is_empty()) {
        conditions.push("platform = ?".to_string());
        values.push(provider.clone().into());
    }
    if let Some(gateway) = query.get("gateway").filter(|s| !s.is_empty()) {
        conditions.push("gateway = ?".to_string());
        values.push(gateway.clone().into());
    }
    if query.get("free").map(|s| s == "true").unwrap_or(false) {
        conditions.push("free_tier = 1".to_string());
    }
    if let Some(search) = query.get("search").filter(|s| !s.is_empty()) {
        conditions
            .push("(display_name LIKE ? OR model_id LIKE ? OR description LIKE ?)".to_string());
        let q = SqlValue::Text(format!("%{search}%"));
        values.push(q.clone());
        values.push(q.clone());
        values.push(q);
    }
    (conditions.join(" AND "), values)
}

fn search_limit(query: &HashMap<String, String>) -> i64 {
    // `Math.min(parseInt(limit ?? '50'), 200)` — negative passes through
    // (SQLite treats LIMIT < 0 as unlimited, matching drizzle).
    query
        .get("limit")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(50)
        .min(200)
}

fn search_offset(query: &HashMap<String, String>) -> i64 {
    query
        .get("offset")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

struct SearchRow {
    id: i64,
    platform: String,
    model_id: String,
    display_name: String,
    context_window: Option<i64>,
    free_tier: i64,
    gateway: Option<String>,
    supported_features: Option<String>,
    pricing_prompt: Option<f64>,
    pricing_completion: Option<f64>,
    external_url: Option<String>,
    description: Option<String>,
    intelligence_rank: i64,
    speed_rank: i64,
    intelligence_score: Option<f64>,
    speed_tokens_per_sec: Option<f64>,
    ranking_source: Option<String>,
    last_ranked_at: Option<String>,
    size_label: String,
    last_synced_at: Option<String>,
    source: String,
    external_speed_tps: Option<f64>,
    observed_speed_tps: Option<f64>,
    quality_source: Option<String>,
    speed_source: Option<String>,
    quality_confidence: Option<f64>,
    speed_confidence: Option<f64>,
    quality_updated_at: Option<String>,
    external_speed_updated_at: Option<String>,
    observed_speed_updated_at: Option<String>,
}

fn search_row_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchRow> {
    Ok(SearchRow {
        id: row.get(0)?,
        platform: row.get(1)?,
        model_id: row.get(2)?,
        display_name: row.get(3)?,
        context_window: row.get(4)?,
        free_tier: row.get::<_, i64>(5).unwrap_or(0),
        gateway: row.get(6)?,
        supported_features: row.get(7)?,
        pricing_prompt: row.get(8)?,
        pricing_completion: row.get(9)?,
        external_url: row.get(10)?,
        description: row.get(11)?,
        intelligence_rank: row.get(12)?,
        speed_rank: row.get(13)?,
        intelligence_score: row.get(14)?,
        speed_tokens_per_sec: row.get(15)?,
        ranking_source: row.get(16)?,
        last_ranked_at: row.get(17)?,
        size_label: row.get::<_, String>(18).unwrap_or_default(),
        last_synced_at: row.get(19)?,
        source: row.get::<_, String>(20).unwrap_or_else(|_| "manual".into()),
        external_speed_tps: row.get(21)?,
        observed_speed_tps: row.get(22)?,
        quality_source: row.get(23)?,
        speed_source: row.get(24)?,
        quality_confidence: row.get(25)?,
        speed_confidence: row.get(26)?,
        quality_updated_at: row.get(27)?,
        external_speed_updated_at: row.get(28)?,
        observed_speed_updated_at: row.get(29)?,
    })
}

const SEARCH_COLS: &str = "id, platform, model_id, display_name, context_window, free_tier, \
     gateway, supported_features, pricing_prompt, pricing_completion, external_url, description, \
     intelligence_rank, speed_rank, intelligence_score, speed_tokens_per_sec, ranking_source, \
     last_ranked_at, size_label, last_synced_at, source, external_speed_tps, observed_speed_tps, \
     quality_source, speed_source, quality_confidence, speed_confidence, quality_updated_at, \
     external_speed_updated_at, observed_speed_updated_at";

async fn search_models(Query(params): Query<HashMap<String, String>>) -> Response {
    let limit = search_limit(&params);
    let offset = search_offset(&params);
    let (where_sql, where_values) = build_search_conditions(&params);

    let data_sql = format!(
        "SELECT {SEARCH_COLS} FROM models WHERE {where_sql} \
         ORDER BY intelligence_rank ASC LIMIT ? OFFSET ?"
    );
    let count_sql = format!("SELECT count(*) FROM models WHERE {where_sql}");

    let (models, total) = {
        let conn = db().lock().await;
        let mut data_values = where_values.clone();
        data_values.push(SqlValue::Integer(limit));
        data_values.push(SqlValue::Integer(offset));
        let models: Vec<SearchRow> = conn
            .prepare(&data_sql)
            .ok()
            .and_then(|mut s| {
                s.query_map(
                    rusqlite::params_from_iter(data_values.iter()),
                    search_row_from_row,
                )
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        let total = conn
            .query_row(
                &count_sql,
                rusqlite::params_from_iter(where_values.iter()),
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);
        (models, total)
    };

    let data: Vec<Value> = models
        .iter()
        .map(|m| {
            let supported_features: Value = m
                .supported_features
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(json!([]));
            let effective_speed = ranking_json::effective_speed(
                m.observed_speed_tps,
                m.external_speed_tps,
                m.speed_tokens_per_sec,
            );
            let quality = ranking_json::quality(
                m.intelligence_score,
                m.intelligence_rank,
                m.quality_source.as_deref().or(m.ranking_source.as_deref()),
                m.quality_confidence,
                m.quality_updated_at
                    .as_deref()
                    .or(m.last_ranked_at.as_deref()),
            );
            let speed = ranking_json::speed(
                effective_speed,
                m.speed_rank,
                m.speed_source.as_deref().or(m.ranking_source.as_deref()),
                m.speed_confidence,
                m.observed_speed_updated_at
                    .as_deref()
                    .or(m.external_speed_updated_at.as_deref())
                    .or(m.last_ranked_at.as_deref()),
                0,
            );
            json!({
                "id": m.id,
                "platform": m.platform,
                "modelId": m.model_id,
                "displayName": m.display_name,
                "contextWindow": m.context_window,
                "freeTier": m.free_tier != 0,
                "gateway": m.gateway,
                "supportedFeatures": supported_features,
                "pricingPrompt": m.pricing_prompt,
                "pricingCompletion": m.pricing_completion,
                "externalUrl": m.external_url,
                "description": m.description,
                "intelligenceRank": quality["rank"],
                "speedRank": speed["rank"],
                "intelligenceScore": m.intelligence_score,
                "speedTokensPerSec": effective_speed,
                "rankingSource": m.ranking_source,
                "lastRankedAt": m.last_ranked_at,
                "quality": quality,
                "speed": speed,
                "ranked": effective_speed.is_some() || m.intelligence_score.is_some(),
                "qualityRanked": m.intelligence_score.is_some(),
                "speedRanked": effective_speed.is_some(),
                "sizeLabel": m.size_label,
                "lastSyncedAt": m.last_synced_at,
                "source": m.source,
            })
        })
        .collect();

    Json(json!({ "data": data, "meta": { "total": total, "limit": limit, "offset": offset } }))
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /pricing, POST /pricing/calculate
// ─────────────────────────────────────────────────────────────────────

async fn get_pricing() -> Response {
    let pricing = {
        let conn = db().lock().await;
        get_all_pricing(&conn)
    };
    Json(serde_json::to_value(&pricing).unwrap_or(json!([]))).into_response()
}

/// Minimal replica of the Zod error for the calcSchema. Produces the array
/// shape of `parsed.error.errors` (each item a ZodIssue). Only the branches
/// reachable from a JSON payload are implemented.
fn calc_validation_issues(v: &Value) -> Vec<Value> {
    fn zod_type(path: &[Value], expected: &str, received: &str) -> Value {
        json!({
            "code": "invalid_type",
            "expected": expected,
            "received": received,
            "path": path,
            "message": if received == "undefined" {
                "Required".to_string()
            } else {
                format!("Expected {expected}, received {received}")
            },
        })
    }

    // Zod's `getParsedType` (the `received` value in invalid_type issues):
    // JSON null reports as "null", not JS `typeof null` ("object").
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

    fn check_string(obj: &serde_json::Map<String, Value>, key: &str, issues: &mut Vec<Value>) {
        match obj.get(key) {
            None => issues.push(zod_type(&[json!(key)], "string", "undefined")),
            Some(v) if !v.is_string() => {
                issues.push(zod_type(&[json!(key)], "string", typeof_value(v)))
            }
            Some(_) => {}
        }
    }

    fn check_int_min(obj: &serde_json::Map<String, Value>, key: &str, issues: &mut Vec<Value>) {
        match obj.get(key) {
            None => issues.push(zod_type(&[json!(key)], "number", "undefined")),
            Some(v) => {
                let Some(n) = v.as_f64() else {
                    issues.push(zod_type(&[json!(key)], "number", typeof_value(v)));
                    return;
                };
                if n.fract() != 0.0 {
                    issues.push(zod_type(&[json!(key)], "integer", "number"));
                }
                if n < 0.0 {
                    issues.push(json!({
                        "code": "too_small",
                        "minimum": 0,
                        "type": "number",
                        "inclusive": true,
                        "exact": false,
                        "message": "Number must be greater than or equal to 0",
                    }));
                }
            }
        }
    }

    let mut issues: Vec<Value> = Vec::new();
    let Some(obj) = v.as_object() else {
        return vec![zod_type(&[], "object", typeof_value(v))];
    };
    check_string(obj, "platform", &mut issues);
    check_string(obj, "modelId", &mut issues);
    check_int_min(obj, "promptTokens", &mut issues);
    check_int_min(obj, "completionTokens", &mut issues);
    issues
}

async fn calculate_pricing(bytes: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return crate::app::error_text(500, "Failed to parse JSON body"),
    };
    let issues = calc_validation_issues(&raw);
    if !issues.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": issues }))).into_response();
    }
    let platform = raw
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let model_id = raw
        .get("modelId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let prompt_tokens = raw.get("promptTokens").and_then(Value::as_i64).unwrap_or(0);
    let completion_tokens = raw
        .get("completionTokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let estimate = {
        let conn = db().lock().await;
        calculate_cost(&conn, platform, model_id, prompt_tokens, completion_tokens)
    };
    Json(serde_json::to_value(&estimate).unwrap_or(Value::Null)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// POST /sync, /enrich-rankings, /reset-rankings
// ─────────────────────────────────────────────────────────────────────

async fn sync_models_api() -> Response {
    match sync_models().await {
        Ok(result) => {
            let mut out = serde_json::to_value(&result).unwrap_or(json!({}));
            if let Some(obj) = out.as_object_mut() {
                obj.insert("success".to_string(), json!(true));
            }
            Json(out).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": err })),
        )
            .into_response(),
    }
}

async fn enrich_rankings_api() -> Response {
    let result = enrich_rankings().await;
    let mut out = serde_json::to_value(&result).unwrap_or(json!({}));
    if let Some(obj) = out.as_object_mut() {
        let success = obj
            .get("errors")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty);
        obj.insert("success".to_string(), json!(success));
    }
    Json(out).into_response()
}

async fn reset_rankings_api() -> Response {
    let reset = reset_rankings().await;
    Json(json!({ "success": true, "reset": reset })).into_response()
}

fn ranking_metric_input(body: &Value, key: &str) -> Result<Option<Option<f64>>, String> {
    match body.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(value) => {
            let Some(value) = value.as_f64() else {
                return Err(format!("{key} must be a non-negative number or null"));
            };
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{key} must be a non-negative number or null"));
            }
            Ok(Some(Some(value)))
        }
    }
}

fn ranking_confidence_input(body: &Value) -> Result<f64, String> {
    let value = body
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        Err("confidence must be a number between 0 and 1".to_string())
    } else {
        Ok(value)
    }
}

fn model_db_id_input(body: &Value) -> Result<i64, String> {
    let value = body
        .get("modelDbId")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0 && value.fract() == 0.0)
        .map(|value| value as i64)
        .ok_or_else(|| "modelDbId must be a positive integer".to_string())?;
    Ok(value)
}

/// Add or update a clearly-labelled manual benchmark override. The value is
/// persisted in the same provenance table as external sources; it is never
/// presented as an Artificial Analysis value.
async fn upsert_ranking_override(bytes: Bytes) -> Response {
    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return crate::app::error_text(400, "Failed to parse JSON body"),
    };
    if !body.is_object() {
        return crate::app::error_text(400, "Expected an object");
    }
    let model_db_id = match model_db_id_input(&body) {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let intelligence = match ranking_metric_input(&body, "intelligenceScore") {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let speed = match ranking_metric_input(&body, "speedTokensPerSec") {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let has_value = intelligence.is_some_and(|value| value.is_some())
        || speed.is_some_and(|value| value.is_some());
    if !has_value {
        return crate::app::error_text(
            400,
            "At least one of intelligenceScore or speedTokensPerSec is required",
        );
    }
    let confidence = match ranking_confidence_input(&body) {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let result = crate::db::connection::run_in_transaction(move |conn| {
        let (platform, model_id, display_name): (String, String, String) = conn.query_row(
            "SELECT platform, model_id, display_name FROM models WHERE id = ?1",
            rusqlite::params![model_db_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let canonical = crate::services::rankings::match_ids::canonical_id_for_local(
            &platform,
            &model_id,
        );
        let publisher = crate::services::rankings::match_ids::publisher_from_model_id(&model_id);
        conn.execute(
            "INSERT INTO canonical_models (canonical_id, display_name, publisher)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(canonical_id) DO UPDATE SET
               display_name = COALESCE(canonical_models.display_name, excluded.display_name),
               publisher = COALESCE(canonical_models.publisher, excluded.publisher),
               updated_at = datetime('now')",
            rusqlite::params![canonical, display_name, publisher],
        )?;
        let canonical_model_id: i64 = conn.query_row(
            "SELECT id FROM canonical_models WHERE canonical_id = ?1",
            rusqlite::params![canonical],
            |row| row.get(0),
        )?;
        conn.execute(
            "UPDATE models SET canonical_model_id = ?1 WHERE id = ?2",
            rusqlite::params![canonical_model_id, model_db_id],
        )?;
        conn.execute(
            "INSERT INTO model_aliases
             (canonical_model_id, source, source_model_id, confidence, verified)
             VALUES (?1, 'manual', ?2, ?3, 1)
             ON CONFLICT(source, source_model_id) DO UPDATE SET
               canonical_model_id = excluded.canonical_model_id,
               confidence = excluded.confidence, verified = 1",
            rusqlite::params![
                canonical_model_id,
                format!("{platform}:{model_id}"),
                confidence
            ],
        )?;
        conn.execute(
            "INSERT INTO model_benchmarks
             (canonical_model_id, source, source_model_id, source_model_slug,
              intelligence_score, speed_tokens_per_sec, confidence, fetched_at)
             VALUES (?1, 'manual', ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(canonical_model_id, source) DO UPDATE SET
               source_model_id = excluded.source_model_id,
               source_model_slug = excluded.source_model_slug,
               intelligence_score = COALESCE(excluded.intelligence_score, model_benchmarks.intelligence_score),
               speed_tokens_per_sec = COALESCE(excluded.speed_tokens_per_sec, model_benchmarks.speed_tokens_per_sec),
               confidence = excluded.confidence, fetched_at = excluded.fetched_at",
            rusqlite::params![
                canonical_model_id,
                format!("{platform}:{model_id}"),
                model_id,
                intelligence.flatten(),
                speed.flatten(),
                confidence,
                now,
            ],
        )?;
        if let Some(Some(value)) = intelligence {
            conn.execute(
                "UPDATE models SET intelligence_score = ?1, intelligence_rank = 0,
                        quality_source = 'manual', quality_confidence = ?2,
                        ranking_confidence = ?2,
                        quality_updated_at = ?3, last_ranked_at = ?3
                 WHERE canonical_model_id = ?4",
                rusqlite::params![value, confidence, now, canonical_model_id],
            )?;
        }
        if let Some(Some(value)) = speed {
            conn.execute(
                "UPDATE models SET external_speed_tps = ?1,
                        external_speed_updated_at = ?2,
                        speed_confidence = ?3, ranking_confidence = ?3,
                        speed_source = CASE WHEN observed_speed_tps IS NULL THEN 'manual' ELSE speed_source END,
                        speed_rank = CASE WHEN observed_speed_tps IS NULL THEN 0 ELSE speed_rank END,
                        last_ranked_at = ?2
                 WHERE canonical_model_id = ?4",
                rusqlite::params![value, now, confidence, canonical_model_id],
            )?;
        }
        Ok(canonical_model_id)
    })
    .await;
    match result {
        Ok(canonical_model_id) => Json(json!({
            "success": true,
            "source": "manual",
            "modelDbId": model_db_id,
            "canonicalModelId": canonical_model_id,
        }))
        .into_response(),
        Err(error) => crate::app::error_text(404, &error.to_string()),
    }
}

/// Add a reviewed source alias. Manual aliases have explicit precedence in
/// the identity resolver and do not perform fuzzy matching.
async fn upsert_ranking_alias(bytes: Bytes) -> Response {
    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return crate::app::error_text(400, "Failed to parse JSON body"),
    };
    if !body.is_object() {
        return crate::app::error_text(400, "Expected an object");
    }
    let model_db_id = match model_db_id_input(&body) {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let source = body
        .get("source")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let source_model_id = body
        .get("sourceModelId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (Some(source), Some(source_model_id)) = (source, source_model_id) else {
        return crate::app::error_text(400, "source and sourceModelId are required");
    };
    let confidence = match ranking_confidence_input(&body) {
        Ok(value) => value,
        Err(error) => return crate::app::error_text(400, &error),
    };
    let response_source = source.clone();
    let response_source_model_id = source_model_id.clone();
    let result = crate::db::connection::run_in_transaction(move |conn| {
        let (platform, model_id, display_name, existing_canonical): (
            String,
            String,
            String,
            Option<i64>,
        ) = conn.query_row(
            "SELECT platform, model_id, display_name, canonical_model_id
             FROM models WHERE id = ?1",
            rusqlite::params![model_db_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let canonical_model_id = if let Some(existing) = existing_canonical {
            existing
        } else {
            let canonical =
                crate::services::rankings::match_ids::canonical_id_for_local(&platform, &model_id);
            let publisher =
                crate::services::rankings::match_ids::publisher_from_model_id(&model_id);
            conn.execute(
                "INSERT INTO canonical_models (canonical_id, display_name, publisher)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(canonical_id) DO NOTHING",
                rusqlite::params![canonical, display_name, publisher],
            )?;
            let id: i64 = conn.query_row(
                "SELECT id FROM canonical_models WHERE canonical_id = ?1",
                rusqlite::params![canonical],
                |row| row.get(0),
            )?;
            conn.execute(
                "UPDATE models SET canonical_model_id = ?1 WHERE id = ?2",
                rusqlite::params![id, model_db_id],
            )?;
            id
        };
        conn.execute(
            "INSERT INTO model_aliases
             (canonical_model_id, source, source_model_id, confidence, verified)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(source, source_model_id) DO UPDATE SET
               canonical_model_id = excluded.canonical_model_id,
               confidence = excluded.confidence, verified = 1",
            rusqlite::params![canonical_model_id, source, source_model_id, confidence],
        )?;
        Ok(canonical_model_id)
    })
    .await;
    match result {
        Ok(canonical_model_id) => Json(json!({
            "success": true,
            "source": response_source,
            "sourceModelId": response_source_model_id,
            "canonicalModelId": canonical_model_id,
        }))
        .into_response(),
        Err(error) => crate::app::error_text(404, &error.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// GET /enrich-rankings/status
// ─────────────────────────────────────────────────────────────────────

async fn rankings_status() -> Response {
    let (
        total,
        quality_ranked,
        speed_ranked,
        locally_measured,
        ranked,
        unranked,
        ranked_row,
        by_source,
    ) = {
        let conn = db().lock().await;
        let total: i64 = conn
            .query_row("SELECT count(*) FROM models", [], |r| r.get(0))
            .unwrap_or(0);
        let quality_ranked: i64 = conn
            .query_row(
                "SELECT count(*) FROM models WHERE intelligence_score IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let speed_ranked: i64 = conn
            .query_row(
                "SELECT count(*) FROM models
                 WHERE COALESCE(observed_speed_tps, external_speed_tps, speed_tokens_per_sec) IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let locally_measured: i64 = conn
            .query_row(
                "SELECT count(*) FROM models m
                 INNER JOIN model_performance mp
                   ON mp.platform = m.platform AND mp.model_id = m.model_id
                 WHERE mp.sample_count > 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let ranked: i64 = conn
            .query_row(
                "SELECT count(*) FROM models
                 WHERE intelligence_score IS NOT NULL
                    OR COALESCE(observed_speed_tps, external_speed_tps, speed_tokens_per_sec) IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let unranked: i64 = conn
            .query_row(
                "SELECT count(*) FROM models
                 WHERE intelligence_score IS NULL
                   AND COALESCE(observed_speed_tps, external_speed_tps, speed_tokens_per_sec) IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let ranked_row: Option<(i64, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT count(*), min(last_ranked_at), max(last_ranked_at) \
                 FROM models WHERE last_ranked_at IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        let by_source: Vec<(Option<String>, i64)> = conn
            .prepare(
                "SELECT ranking_source, count(*) FROM models \
                 WHERE ranking_source IS NOT NULL GROUP BY ranking_source",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();
        (
            total,
            quality_ranked,
            speed_ranked,
            locally_measured,
            ranked,
            unranked,
            ranked_row,
            by_source,
        )
    };

    let oldest_ranking = ranked_row.as_ref().and_then(|r| r.1.clone());
    let newest_ranking = ranked_row.as_ref().and_then(|r| r.2.clone());
    Json(json!({
        "total": total,
        "ranked": ranked,
        "qualityRanked": quality_ranked,
        "speedRanked": speed_ranked,
        "locallyMeasured": locally_measured,
        "unranked": unranked,
        "oldestRanking": oldest_ranking,
        "newestRanking": newest_ranking,
        "bySource": by_source.iter().map(|(s, c)| json!({ "source": s, "count": c })).collect::<Vec<_>>(),
    }))
    .into_response()
}

async fn ranking_sources() -> Response {
    let configured_aa = crate::env::env_string("ARTIFICIAL_ANALYSIS_API_KEY")
        .is_some_and(|key| !key.trim().is_empty());
    let rows: Vec<Value> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT source, enabled, last_success, last_failure, model_count, last_error
             FROM ranking_source_status ORDER BY source",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok(json!({
                    "name": row.get::<_, String>(0)?,
                    "enabled": row.get::<_, i64>(1)? != 0,
                    "lastSuccess": row.get::<_, Option<String>>(2)?,
                    "lastFailure": row.get::<_, Option<String>>(3)?,
                    "modelCount": row.get::<_, Option<i64>>(4)?,
                    "lastError": row.get::<_, Option<String>>(5)?,
                }))
            })
            .ok()
            .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default()
    };
    let mut result = rows;
    if !result
        .iter()
        .any(|row| row["name"] == "artificial-analysis")
    {
        result.push(json!({
            "name": "artificial-analysis",
            "enabled": configured_aa,
            "lastSuccess": null,
            "lastFailure": null,
            "modelCount": null,
            "lastError": null,
        }));
    }
    if !result.iter().any(|row| row["name"] == "livebench") {
        result.push(json!({ "name": "livebench", "enabled": false }));
    }
    Json(result).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /sync/status, /sync/history, /sync/changes/:logId
// ─────────────────────────────────────────────────────────────────────

fn sync_log_row_to_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "startedAt": row.get::<_, String>(1)?,
        "completedAt": row.get::<_, Option<String>>(2)?,
        "status": row.get::<_, String>(3)?,
        "totalDiscovered": row.get::<_, Option<i64>>(4)?,
        "added": row.get::<_, Option<i64>>(5)?,
        "updated": row.get::<_, Option<i64>>(6)?,
        "disabled": row.get::<_, Option<i64>>(7)?,
        "freeToPaid": row.get::<_, Option<i64>>(8)?,
        "paidToFree": row.get::<_, Option<i64>>(9)?,
        "storedDisabled": row.get::<_, Option<i64>>(10)?,
        "error": row.get::<_, Option<String>>(11)?,
    }))
}

fn sync_change_row_to_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "syncLogId": row.get::<_, i64>(1)?,
        "changeType": row.get::<_, String>(2)?,
        "platform": row.get::<_, String>(3)?,
        "modelId": row.get::<_, String>(4)?,
        "displayName": row.get::<_, Option<String>>(5)?,
        "details": row.get::<_, Option<String>>(6)?,
        "createdAt": row.get::<_, String>(7)?,
    }))
}

const SYNC_LOG_COLS: &str = "id, started_at, completed_at, status, total_discovered, added, \
     updated, disabled, free_to_paid, paid_to_free, stored_disabled, error \
     FROM sync_log";

const SYNC_CHANGE_COLS: &str = "id, sync_log_id, change_type, platform, model_id, display_name, \
     details, created_at FROM sync_changes";

async fn sync_status() -> Response {
    let (last, changes) = {
        let conn = db().lock().await;
        let last: Option<Value> = conn
            .prepare(&format!("SELECT {SYNC_LOG_COLS} ORDER BY id DESC LIMIT 1"))
            .ok()
            .and_then(|mut s| s.query_row([], sync_log_row_to_json).ok());
        let changes: Vec<Value> = last
            .as_ref()
            .and_then(|l| l.get("id").and_then(Value::as_i64))
            .map(|log_id| {
                conn.prepare(&format!("SELECT {SYNC_CHANGE_COLS} WHERE sync_log_id = ?1"))
                    .ok()
                    .and_then(|mut s| {
                        s.query_map(rusqlite::params![log_id], sync_change_row_to_json)
                            .ok()
                            .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        (last, changes)
    };
    Json(json!({ "lastSync": last, "recentChanges": changes })).into_response()
}

async fn sync_history(Query(params): Query<HashMap<String, String>>) -> Response {
    let limit = params
        .get("limit")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(10)
        .min(50);
    let rows: Vec<Value> = {
        let conn = db().lock().await;
        conn.prepare(&format!("SELECT {SYNC_LOG_COLS} ORDER BY id DESC LIMIT ?1"))
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![limit], sync_log_row_to_json)
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
    };
    Json(json!(rows)).into_response()
}

async fn sync_changes(Path(log_id): Path<String>) -> Response {
    let Ok(log_id) = log_id.parse::<i64>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid logId" })),
        )
            .into_response();
    };
    let rows: Vec<Value> = {
        let conn = db().lock().await;
        conn.prepare(&format!("SELECT {SYNC_CHANGE_COLS} WHERE sync_log_id = ?1"))
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![log_id], sync_change_row_to_json)
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
    };
    Json(json!(rows)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// Bulk enable / disable — require `{ "confirm": true }` in the body.
// ─────────────────────────────────────────────────────────────────────

/// Parse the confirmation body and return an error response on failure.
fn parse_bulk_confirm(bytes: &Bytes) -> Result<(), Box<Response>> {
    // `c.req.json()` throws → body stays `{}` → "Missing confirm" message.
    let raw: Value = serde_json::from_slice(bytes).unwrap_or_else(|_| json!({}));
    // Arrays are object-like for this validation and fall through to the
    // missing-confirmation branch; primitives and null are invalid bodies.
    let js_object_like = raw.is_object() || raw.is_array();
    if !js_object_like {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": { "message": "Invalid body. Expected { \"confirm\": true }." }
                })),
            )
                .into_response(),
        ));
    }
    let confirm = raw.is_object() && raw.get("confirm").and_then(Value::as_bool) == Some(true);
    if !confirm {
        return Err(Box::new((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": { "message": "Missing confirm: true. Bulk actions are destructive and require explicit confirmation." }
            })),
        )
            .into_response()));
    }
    Ok(())
}

async fn disable_all(bytes: Bytes) -> Response {
    if let Err(resp) = parse_bulk_confirm(&bytes) {
        return *resp;
    }
    let (models_updated, fb_updated) = {
        let conn = db().lock().await;
        let models_updated = match conn.execute("UPDATE models SET enabled = 0", []) {
            Ok(n) => n as i64,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        };
        let fb_updated = match conn.execute("UPDATE fallback_config SET enabled = 0", []) {
            Ok(n) => n as i64,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        };
        (models_updated, fb_updated)
    };
    Json(json!({
        "success": true,
        "modelsUpdated": models_updated,
        "fallbackEntriesUpdated": fb_updated,
    }))
    .into_response()
}

async fn enable_all(bytes: Bytes) -> Response {
    if let Err(resp) = parse_bulk_confirm(&bytes) {
        return *resp;
    }
    let (models_updated, fb_updated) = {
        let conn = db().lock().await;
        let models_updated = match conn.execute("UPDATE models SET enabled = 1", []) {
            Ok(n) => n as i64,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        };
        let fb_updated = match conn.execute("UPDATE fallback_config SET enabled = 1", []) {
            Ok(n) => n as i64,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        };
        (models_updated, fb_updated)
    };
    Json(json!({
        "success": true,
        "modelsUpdated": models_updated,
        "fallbackEntriesUpdated": fb_updated,
    }))
    .into_response()
}

async fn enable_free(bytes: Bytes) -> Response {
    if let Err(resp) = parse_bulk_confirm(&bytes) {
        return *resp;
    }
    let (free_enabled, paid_disabled, fb_free_enabled, fb_paid_disabled) = {
        let conn = db().lock().await;
        let free_enabled =
            match conn.execute("UPDATE models SET enabled = 1 WHERE free_tier = 1", []) {
                Ok(n) => n as i64,
                Err(e) => return crate::app::error_text(500, &e.to_string()),
            };
        let paid_disabled =
            match conn.execute("UPDATE models SET enabled = 0 WHERE free_tier != 1", []) {
                Ok(n) => n as i64,
                Err(e) => return crate::app::error_text(500, &e.to_string()),
            };

        let free_ids: Vec<i64> = conn
            .prepare("SELECT id FROM models WHERE free_tier = 1")
            .ok()
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, i64>(0))
                    .ok()
                    .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default();

        if free_ids.is_empty() {
            let fb_paid_disabled = match conn.execute("UPDATE fallback_config SET enabled = 0", [])
            {
                Ok(n) => n as i64,
                Err(e) => return crate::app::error_text(500, &e.to_string()),
            };
            (free_enabled, paid_disabled, 0, fb_paid_disabled)
        } else {
            let placeholders = free_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let fb_free_enabled = match conn.execute(
                &format!(
                    "UPDATE fallback_config SET enabled = 1 WHERE model_db_id IN ({placeholders})"
                ),
                rusqlite::params_from_iter(free_ids.iter()),
            ) {
                Ok(n) => n as i64,
                Err(e) => return crate::app::error_text(500, &e.to_string()),
            };
            let fb_paid_disabled = match conn.execute(
                &format!("UPDATE fallback_config SET enabled = 0 WHERE model_db_id NOT IN ({placeholders})"),
                rusqlite::params_from_iter(free_ids.iter()),
            ) {
                Ok(n) => n as i64,
                Err(e) => return crate::app::error_text(500, &e.to_string()),
            };
            (
                free_enabled,
                paid_disabled,
                fb_free_enabled,
                fb_paid_disabled,
            )
        }
    };
    Json(json!({
        "success": true,
        "freeEnabled": free_enabled,
        "paidDisabled": paid_disabled,
        "fallbackFreeEnabled": fb_free_enabled,
        "fallbackPaidDisabled": fb_paid_disabled,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_conditions_with_all_filters() {
        let mut q = HashMap::new();
        q.insert("provider".to_string(), "google".to_string());
        q.insert("gateway".to_string(), "api".to_string());
        q.insert("free".to_string(), "true".to_string());
        q.insert("search".to_string(), "gemini".to_string());
        let (sql, values) = build_search_conditions(&q);
        assert!(sql.contains("platform = ?"));
        assert!(sql.contains("gateway = ?"));
        assert!(sql.contains("free_tier = 1"));
        assert!(sql.contains("display_name LIKE ?"));
        assert_eq!(values.len(), 5);
        assert!(values
            .iter()
            .any(|v| *v == SqlValue::Text("%gemini%".into())));
    }

    #[test]
    fn search_conditions_empty_provider_ignored() {
        let mut q = HashMap::new();
        q.insert("provider".to_string(), String::new());
        let (sql, values) = build_search_conditions(&q);
        assert!(sql == "enabled = 1");
        assert!(values.is_empty());
    }

    #[test]
    fn search_limit_clamped_and_offset_defaulted() {
        let mut q = HashMap::new();
        q.insert("limit".to_string(), "500".to_string());
        assert_eq!(search_limit(&q), 200);
        q.insert("limit".to_string(), "10".to_string());
        assert_eq!(search_limit(&q), 10);
        assert_eq!(search_offset(&HashMap::new()), 0);
    }

    #[test]
    fn calc_validation_issues_shapes() {
        // Missing everything
        let issues = calc_validation_issues(&json!({}));
        assert_eq!(issues.len(), 4);
        assert_eq!(issues[0]["message"], "Required");
        assert!(issues[0]["path"].as_array().unwrap()[0] == json!("platform"));

        // Wrong type
        let issues = calc_validation_issues(&json!({
            "platform": 5,
            "modelId": "x",
            "promptTokens": 1,
            "completionTokens": 2,
        }));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["message"], "Expected string, received number");

        // Non-integer and negative
        let issues = calc_validation_issues(&json!({
            "platform": "p",
            "modelId": "m",
            "promptTokens": 1.5,
            "completionTokens": -2,
        }));
        let messages: Vec<&str> = issues
            .iter()
            .map(|i| i["message"].as_str().unwrap())
            .collect();
        assert!(messages.contains(&"Expected integer, received number"));
        assert!(messages.contains(&"Number must be greater than or equal to 0"));

        // Non-object body
        let issues = calc_validation_issues(&json!([1, 2]));
        assert_eq!(issues[0]["message"], "Expected object, received array");
    }

    #[test]
    fn bulk_confirm_accepts_and_rejects() {
        assert!(parse_bulk_confirm(&Bytes::from_static(b"{\"confirm\":true}")).is_ok());
        assert!(parse_bulk_confirm(&Bytes::from_static(b"[1]")).is_err());
        assert!(parse_bulk_confirm(&Bytes::from_static(b"\"x\"")).is_err());
        assert!(parse_bulk_confirm(&Bytes::from_static(b"null")).is_err());
        // Missing confirm on a valid object
        assert!(parse_bulk_confirm(&Bytes::from_static(b"{}")).is_err());
        assert!(parse_bulk_confirm(&Bytes::from_static(b"{\"confirm\":false}")).is_err());
    }
}
