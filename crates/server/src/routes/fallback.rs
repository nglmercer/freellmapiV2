//! Port of `server/src/routes/fallback.ts`.

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
use crate::services::router::get_all_penalties;

pub fn router() -> axum::Router {
    use axum::routing::{get, post, put};
    axum::Router::new()
        .route("/", get(get_fallback))
        .route("/", put(update_fallback))
        .route("/sort/{preset}", post(sort_fallback))
        .route("/token-usage", get(token_usage))
}

// ─────────────────────────────────────────────────────────────────────
// GET / — the fallback chain with dynamic penalties.
// ─────────────────────────────────────────────────────────────────────

struct FallbackItem {
    model_db_id: i64,
    priority: i64,
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
}

fn fallback_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FallbackItem> {
    Ok(FallbackItem {
        model_db_id: row.get(0)?,
        priority: row.get(1)?,
        enabled: row.get::<_, i64>(2).unwrap_or(1),
        platform: row.get(3)?,
        model_id: row.get(4)?,
        display_name: row.get(5)?,
        intelligence_rank: row.get(6)?,
        speed_rank: row.get(7)?,
        intelligence_score: row.get(8)?,
        speed_tokens_per_sec: row.get(9)?,
        ranking_source: row.get(10)?,
        last_ranked_at: row.get(11)?,
        size_label: row.get::<_, String>(12).unwrap_or_default(),
        rpm_limit: row.get(13)?,
        rpd_limit: row.get(14)?,
        monthly_token_budget: row.get::<_, String>(15).unwrap_or_default(),
        free_tier: row.get::<_, i64>(16).unwrap_or(0),
    })
}

async fn get_fallback() -> Response {
    let (rows, key_counts) = {
        let conn = db().lock().await;
        let rows: Vec<FallbackItem> = conn
            .prepare(
                "SELECT fc.model_db_id, fc.priority, fc.enabled, m.platform, m.model_id, \
                 m.display_name, m.intelligence_rank, m.speed_rank, m.intelligence_score, \
                 m.speed_tokens_per_sec, m.ranking_source, m.last_ranked_at, m.size_label, \
                 m.rpm_limit, m.rpd_limit, m.monthly_token_budget, m.free_tier \
                 FROM fallback_config fc INNER JOIN models m ON m.id = fc.model_db_id \
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
            .prepare(
                "SELECT platform, COUNT(*) FROM api_keys WHERE enabled = 1 GROUP BY platform",
            )
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
    let penalty_map: HashMap<i64, (i64, i64)> = get_all_penalties()
        .into_iter()
        .map(|p| (p.model_db_id, (p.count, p.penalty)))
        .collect();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            let (hits, penalty) = penalty_map.get(&r.model_db_id).copied().unwrap_or((0, 0));
            json!({
                "modelDbId": r.model_db_id,
                "priority": r.priority,
                "effectivePriority": r.priority + penalty,
                "penalty": penalty,
                "rateLimitHits": hits,
                "enabled": r.enabled != 0,
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
                "monthlyTokenBudget": r.monthly_token_budget,
                "freeTier": r.free_tier != 0,
                "keyCount": key_count_map.get(&r.platform).copied().unwrap_or(0),
            })
        })
        .collect();

    Json(json!(data)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// PUT / — full replace of the chain. Mirrors the zod array schema.
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
/// field of each issue), matching the TS error body exactly.
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
                "UPDATE fallback_config SET priority = ?1, enabled = ?2 \
                 WHERE model_db_id = ?3",
                rusqlite::params![
                    entry.priority,
                    if entry.enabled { 1 } else { 0 },
                    entry.model_db_id
                ],
            )?;
        }
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
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"~?([\d.]+)(?:-([\d.]+))?([MK])?").unwrap()
    });
    let Some(caps) = re.captures(s) else {
        return 0.0;
    };
    let value = caps.get(2).or_else(|| caps.get(1)).map(|m| m.as_str()).unwrap_or("");
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
    intelligence_rank: i64,
    speed_rank: i64,
    intelligence_score: Option<f64>,
    speed_tokens_per_sec: Option<f64>,
    monthly_token_budget: String,
}

fn descending_cmp(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Port of the TS comparator (fallback.ts `sort/:preset`): negative `cmp`
/// means `a` sorts first. `a.id` is `model_db_id`.
fn sort_cmp(a: &SortRow, b: &SortRow, preset: &str) -> Ordering {
    let id_tiebreak = a.id.cmp(&b.id);
    match preset {
        "intelligence" => {
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
                a.intelligence_rank.cmp(&b.intelligence_rank).then(id_tiebreak)
            }
        }
        "speed" => {
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
    Ok(SortRow {
        id: row.get(0)?,
        intelligence_rank: row.get(1)?,
        speed_rank: row.get(2)?,
        intelligence_score: row.get(3)?,
        speed_tokens_per_sec: row.get(4)?,
        monthly_token_budget: row.get::<_, String>(5).unwrap_or_default(),
    })
}

async fn sort_fallback(Path(preset): Path<String>) -> Response {
    if preset != "intelligence" && preset != "speed" && preset != "budget" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": { "message": format!("Unknown preset: {preset}. Use: intelligence, speed, budget") }
            })),
        )
            .into_response();
    }

    let rows: Vec<SortRow> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT fc.model_db_id, m.intelligence_rank, m.speed_rank, m.intelligence_score, \
             m.speed_tokens_per_sec, m.monthly_token_budget \
             FROM fallback_config fc INNER JOIN models m ON m.id = fc.model_db_id",
        )
        .ok()
        .and_then(|mut s| {
            s.query_map([], sort_row_from_row)
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default()
    };

    let mut sorted = rows;
    sorted.sort_by(|a, b| sort_cmp(a, b, &preset));

    let result = crate::db::connection::run_in_transaction(move |conn| {
        for (i, row) in sorted.iter().enumerate() {
            conn.execute(
                "UPDATE fallback_config SET priority = ?1 WHERE model_db_id = ?2",
                rusqlite::params![(i as i64) + 1, row.id],
            )?;
        }
        Ok(())
    })
    .await;

    match result {
        Ok(_) => Json(json!({ "success": true, "preset": preset })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
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
            intelligence_rank: rank,
            speed_rank: rank,
            intelligence_score: iq,
            speed_tokens_per_sec: None,
            monthly_token_budget: String::new(),
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
        assert_eq!(sort_cmp(&u, &make(4, None, 50), "intelligence"), Ordering::Greater);
    }

    #[test]
    fn sort_comparator_budget() {
        let cheap = SortRow {
            id: 1,
            intelligence_rank: 1,
            speed_rank: 1,
            intelligence_score: None,
            speed_tokens_per_sec: None,
            monthly_token_budget: "100K".to_string(),
        };
        let rich = SortRow {
            id: 2,
            intelligence_rank: 2,
            speed_rank: 2,
            intelligence_score: None,
            speed_tokens_per_sec: None,
            monthly_token_budget: "1M".to_string(),
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
