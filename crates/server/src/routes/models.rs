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
use crate::services::model_sync::sync_models;
use crate::services::pricing::{calculate_cost, get_all_pricing};
use crate::services::rankings::{enrich_rankings, reset_rankings};

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
        .route("/enrich-rankings/status", get(rankings_status))
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
                 fc.priority, fc.enabled \
                 FROM models m LEFT JOIN fallback_config fc ON fc.model_db_id = m.id \
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

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "platform": r.platform,
                "modelId": r.model_id,
                "displayName": r.display_name,
                "intelligenceRank": r.intelligence_rank,
                "speedRank": r.speed_rank,
                "intelligenceScore": r.intelligence_score,
                "speedTokensPerSec": r.speed_tokens_per_sec,
                "rankingSource": r.ranking_source,
                "lastRankedAt": r.last_ranked_at,
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
    })
}

const SEARCH_COLS: &str = "id, platform, model_id, display_name, context_window, free_tier, \
     gateway, supported_features, pricing_prompt, pricing_completion, external_url, description, \
     intelligence_rank, speed_rank, intelligence_score, speed_tokens_per_sec, ranking_source, \
     last_ranked_at, size_label, last_synced_at, source";

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
                "intelligenceRank": m.intelligence_rank,
                "speedRank": m.speed_rank,
                "intelligenceScore": m.intelligence_score,
                "speedTokensPerSec": m.speed_tokens_per_sec,
                "rankingSource": m.ranking_source,
                "lastRankedAt": m.last_ranked_at,
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
        obj.insert("success".to_string(), json!(true));
    }
    Json(out).into_response()
}

async fn reset_rankings_api() -> Response {
    let reset = reset_rankings().await;
    Json(json!({ "success": true, "reset": reset })).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /enrich-rankings/status
// ─────────────────────────────────────────────────────────────────────

async fn rankings_status() -> Response {
    let (total, ranked_row, by_source) = {
        let conn = db().lock().await;
        let total: i64 = conn
            .query_row("SELECT count(*) FROM models", [], |r| r.get(0))
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
        (total, ranked_row, by_source)
    };

    let ranked = ranked_row.as_ref().map(|r| r.0).unwrap_or(0);
    let oldest_ranking = ranked_row.as_ref().and_then(|r| r.1.clone());
    let newest_ranking = ranked_row.as_ref().and_then(|r| r.2.clone());
    Json(json!({
        "total": total,
        "ranked": ranked,
        "unranked": total - ranked,
        "oldestRanking": oldest_ranking,
        "newestRanking": newest_ranking,
        "bySource": by_source.iter().map(|(s, c)| json!({ "source": s, "count": c })).collect::<Vec<_>>(),
    }))
    .into_response()
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
