//! Analytics endpoints.

use std::collections::HashMap;

use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::db::connection::db;

type SummaryRow = (i64, Option<i64>, Option<i64>, Option<i64>, Option<f64>);

pub fn router() -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/summary", get(summary))
        .route("/by-model", get(by_model))
        .route("/by-platform", get(by_platform))
        .route("/timeline", get(timeline))
        .route("/error-distribution", get(error_distribution))
        .route("/errors", get(errors))
}

/// `getSinceTimestamp(range)` — map range to a JS-computed ISO timestamp.
fn get_since_timestamp(range: &str) -> String {
    let now = chrono::Utc::now();
    let since = match range {
        "24h" => now - chrono::Duration::hours(24),
        "30d" => now - chrono::Duration::days(30),
        // '7d' (and anything else) is the default
        _ => now - chrono::Duration::days(7),
    };
    since.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// JS `Math.round(x * 10) / 10` — integral results serialize as integers.
fn json_round1(v: f64) -> Value {
    let r = (v * 10.0).round() / 10.0;
    if r.is_finite() && r.fract() == 0.0 {
        json!(r as i64)
    } else {
        json!(r)
    }
}

/// JS `Math.round(x * 100) / 100`.
fn json_round2(v: f64) -> Value {
    let r = (v * 100.0).round() / 100.0;
    if r.is_finite() && r.fract() == 0.0 {
        json!(r as i64)
    } else {
        json!(r)
    }
}

/// JS `Math.round(x)` → integer.
fn json_round(v: f64) -> Value {
    json!(v.round() as i64)
}

// ─────────────────────────────────────────────────────────────────────
// GET /summary
// ─────────────────────────────────────────────────────────────────────

async fn summary(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let since = get_since_timestamp(&range);

    // Aggregate without GROUP BY always returns one row; SUM/AVG are NULL
    // for an empty set.
    let row: Option<SummaryRow> = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT COUNT(*), \
                    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), \
                    SUM(input_tokens), SUM(output_tokens), AVG(latency_ms) \
             FROM requests WHERE created_at >= ?1",
            rusqlite::params![since],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .ok()
    };

    let (total_requests, success_count, total_input_tokens, total_output_tokens, avg_latency_ms) =
        row.unwrap_or_default();

    // Keep the rate as floating-point division for JSON compatibility.
    let success_rate = if total_requests > 0 {
        (success_count.unwrap_or(0) as f64 / total_requests as f64) * 100.0
    } else {
        0.0
    };

    // ~$3/M input + $15/M output (GPT-4o pricing).
    let input_cost = (total_input_tokens.unwrap_or(0) as f64 / 1_000_000.0) * 3.0;
    let output_cost = (total_output_tokens.unwrap_or(0) as f64 / 1_000_000.0) * 15.0;

    Json(json!({
        "totalRequests": total_requests,
        "successRate": json_round1(success_rate),
        "totalInputTokens": total_input_tokens.unwrap_or(0),
        "totalOutputTokens": total_output_tokens.unwrap_or(0),
        "avgLatencyMs": json_round(avg_latency_ms.unwrap_or(0.0)),
        "estimatedCostSavings": json_round2(input_cost + output_cost),
    }))
    .into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /by-model, GET /by-platform
// ─────────────────────────────────────────────────────────────────────

struct GroupedStat {
    platform: String,
    model_id: Option<String>,
    display_name: Option<String>,
    requests: i64,
    success_rate: Option<f64>,
    avg_latency_ms: Option<f64>,
    total_input_tokens: Option<i64>,
    total_output_tokens: Option<i64>,
}

fn stat_row_from_row(row: &rusqlite::Row<'_>, with_model: bool) -> rusqlite::Result<GroupedStat> {
    let model_id = if with_model { row.get(1)? } else { None };
    Ok(GroupedStat {
        platform: row.get(0)?,
        model_id,
        display_name: if with_model { row.get(2)? } else { None },
        requests: row.get(if with_model { 3 } else { 1 })?,
        success_rate: row.get(if with_model { 4 } else { 2 })?,
        avg_latency_ms: row.get(if with_model { 5 } else { 3 })?,
        total_input_tokens: row.get(if with_model { 6 } else { 4 })?,
        total_output_tokens: row.get(if with_model { 7 } else { 5 })?,
    })
}

fn group_to_json(r: &GroupedStat, with_model: bool) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("platform".to_string(), json!(r.platform));
    if with_model {
        obj.insert(
            "modelId".to_string(),
            json!(r.model_id.as_deref().unwrap_or_default()),
        );
        obj.insert(
            "displayName".to_string(),
            json!(match (&r.display_name, &r.model_id) {
                (Some(d), _) => d.clone(),
                (None, Some(m)) => m.clone(),
                (None, None) => String::new(),
            }),
        );
    }
    obj.insert("requests".to_string(), json!(r.requests));
    obj.insert(
        "successRate".to_string(),
        json_round1(r.success_rate.unwrap_or(0.0)),
    );
    obj.insert(
        "avgLatencyMs".to_string(),
        json_round(r.avg_latency_ms.unwrap_or(0.0)),
    );
    obj.insert(
        "totalInputTokens".to_string(),
        json!(r.total_input_tokens.unwrap_or(0)),
    );
    obj.insert(
        "totalOutputTokens".to_string(),
        json!(r.total_output_tokens.unwrap_or(0)),
    );
    Value::Object(obj)
}

async fn by_model(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let since = get_since_timestamp(&range);

    let rows: Vec<GroupedStat> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT r.platform, r.model_id, m.display_name, COUNT(*), \
                    SUM(CASE WHEN r.status = 'success' THEN 1 ELSE 0 END) * 100.0 / COUNT(*), \
                    AVG(r.latency_ms), SUM(r.input_tokens), SUM(r.output_tokens) \
             FROM requests r \
             LEFT JOIN models m ON m.platform = r.platform AND m.model_id = r.model_id \
             WHERE r.created_at >= ?1 \
             GROUP BY r.platform, r.model_id \
             ORDER BY COUNT(*) DESC",
        )
        .ok()
        .and_then(|mut s| {
            s.query_map(rusqlite::params![since], |r| stat_row_from_row(r, true))
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default()
    };

    let data: Vec<Value> = rows.iter().map(|r| group_to_json(r, true)).collect();
    Json(json!(data)).into_response()
}

async fn by_platform(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let since = get_since_timestamp(&range);

    let rows: Vec<GroupedStat> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT platform, COUNT(*), \
                    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) * 100.0 / COUNT(*), \
                    AVG(latency_ms), SUM(input_tokens), SUM(output_tokens) \
             FROM requests WHERE created_at >= ?1 \
             GROUP BY platform \
             ORDER BY COUNT(*) DESC",
        )
        .ok()
        .and_then(|mut s| {
            s.query_map(rusqlite::params![since], |r| stat_row_from_row(r, false))
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default()
    };

    let data: Vec<Value> = rows.iter().map(|r| group_to_json(r, false)).collect();
    Json(json!(data)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /timeline
// ─────────────────────────────────────────────────────────────────────

async fn timeline(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let interval = params.get("interval").cloned().unwrap_or_else(|| {
        if range == "24h" {
            "hour".to_string()
        } else {
            "day".to_string()
        }
    });
    let since = get_since_timestamp(&range);

    // Hardcoded whitelist — never user-controlled.
    let date_format = if interval == "hour" {
        "%Y-%m-%dT%H:00:00"
    } else {
        "%Y-%m-%d"
    };

    let rows: Vec<Value> = {
        let conn = db().lock().await;
        let sql = format!(
            "SELECT strftime('{date_format}', created_at), COUNT(*), \
                    SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END) \
             FROM requests WHERE created_at >= ?1 \
             GROUP BY strftime('{date_format}', created_at) \
             ORDER BY strftime('{date_format}', created_at) ASC"
        );
        conn.prepare(&sql)
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![since], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                    ))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(timestamp, requests, success_count, failure_count)| {
                json!({
                    "timestamp": timestamp,
                    "requests": requests,
                    "successCount": success_count.unwrap_or(0),
                    "failureCount": failure_count.unwrap_or(0),
                })
            })
            .collect()
    };
    Json(json!(rows)).into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /error-distribution
// ─────────────────────────────────────────────────────────────────────

/// Categorize error text using SQLite `LIKE` patterns. SQLite treats `.*`
/// literally; only `%` and `_` are wildcards.
const ERROR_CATEGORY_SQL: &str = "CASE \
        WHEN error LIKE '%429%' OR error LIKE '%rate limit%' OR error LIKE '%too many%' OR error LIKE '%quota%' THEN 'Rate Limited (429)' \
        WHEN error LIKE '%401%' OR error LIKE '%unauthorized%' OR error LIKE '%invalid.*key%' THEN 'Auth Error (401)' \
        WHEN error LIKE '%403%' OR error LIKE '%forbidden%' THEN 'Forbidden (403)' \
        WHEN error LIKE '%404%' OR error LIKE '%not found%' THEN 'Not Found (404)' \
        WHEN error LIKE '%timeout%' OR error LIKE '%ETIMEDOUT%' OR error LIKE '%ECONNREFUSED%' THEN 'Timeout/Connection' \
        WHEN error LIKE '%500%' OR error LIKE '%internal server%' THEN 'Server Error (500)' \
        WHEN error LIKE '%503%' OR error LIKE '%unavailable%' THEN 'Unavailable (503)' \
        ELSE 'Other' END";

async fn error_distribution(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let since = get_since_timestamp(&range);
    let category = ERROR_CATEGORY_SQL;

    let detailed_sql = format!(
        "SELECT platform, model_id, {category}, COUNT(*) \
         FROM requests WHERE status = 'error' AND created_at >= ?1 \
         GROUP BY platform, {category} \
         ORDER BY COUNT(*) DESC"
    );
    let by_category_sql = format!(
        "SELECT {category}, COUNT(*) \
         FROM requests WHERE status = 'error' AND created_at >= ?1 \
         GROUP BY {category} \
         ORDER BY COUNT(*) DESC"
    );
    let by_platform_sql = "SELECT platform, COUNT(*) \
         FROM requests WHERE status = 'error' AND created_at >= ?1 \
         GROUP BY platform \
         ORDER BY COUNT(*) DESC";

    let (by_category, by_platform, detailed) = {
        let conn = db().lock().await;
        let by_category: Vec<Value> = conn
            .prepare(&by_category_sql)
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![since], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(category, count)| json!({ "category": category, "count": count }))
            .collect();
        let by_platform: Vec<Value> = conn
            .prepare(by_platform_sql)
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![since], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(platform, count)| json!({ "platform": platform, "count": count }))
            .collect();
        // Keep the snake_case `error_category` field used by the response
        // contract.
        let detailed: Vec<Value> = conn
            .prepare(&detailed_sql)
            .ok()
            .and_then(|mut s| {
                s.query_map(rusqlite::params![since], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .ok()
                .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(platform, model_id, error_category, count)| {
                json!({
                    "platform": platform,
                    "modelId": model_id,
                    "error_category": error_category,
                    "count": count,
                })
            })
            .collect();
        (by_category, by_platform, detailed)
    };

    Json(json!({ "byCategory": by_category, "byPlatform": by_platform, "detailed": detailed }))
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────
// GET /errors — recent errors (limit 50).
// ─────────────────────────────────────────────────────────────────────

async fn errors(Query(params): Query<HashMap<String, String>>) -> Response {
    let range = params
        .get("range")
        .cloned()
        .unwrap_or_else(|| "7d".to_string());
    let since = get_since_timestamp(&range);

    let rows: Vec<Value> = {
        let conn = db().lock().await;
        conn.prepare(
            "SELECT id, platform, model_id, error, latency_ms, created_at \
             FROM requests WHERE status = 'error' AND created_at >= ?1 \
             ORDER BY created_at DESC LIMIT 50",
        )
        .ok()
        .and_then(|mut s| {
            s.query_map(rusqlite::params![since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .ok()
            .and_then(|it| it.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default()
        .into_iter()
        .map(|(id, platform, model_id, error, latency_ms, created_at)| {
            json!({
                "id": id,
                "platform": platform,
                "modelId": model_id,
                "error": error,
                "latencyMs": latency_ms,
                "createdAt": created_at,
            })
        })
        .collect()
    };
    Json(json!(rows)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_timestamp_formats() {
        for range in ["7d", "24h", "30d"] {
            let ts = get_since_timestamp(range);
            assert!(ts.ends_with('Z'), "must be a JS-style ISO timestamp: {ts}");
            assert!(ts.contains('T'), "must be a JS-style ISO timestamp: {ts}");
            chrono::DateTime::parse_from_rfc3339(&ts).expect("parseable ISO 8601");
        }
        // Unknown ranges fall back to 7d.
        let fallback = get_since_timestamp("1y");
        let seven = get_since_timestamp("7d");
        let now = chrono::Utc::now().timestamp_millis();
        let fb_age = now
            - chrono::DateTime::parse_from_rfc3339(&fallback)
                .unwrap()
                .timestamp_millis();
        let sv_age = now
            - chrono::DateTime::parse_from_rfc3339(&seven)
                .unwrap()
                .timestamp_millis();
        assert!((fb_age - sv_age).abs() < 2000);
    }

    #[test]
    fn rounding_helpers() {
        assert_eq!(json_round1(50.0), json!(50));
        assert_eq!(json_round1(50.06), json!(50.1));
        assert_eq!(json_round2(1234.567), json!(1234.57));
        assert_eq!(json_round2(12.0), json!(12));
        assert_eq!(json_round(42.6), json!(43));
        assert_eq!(json_round(-0.4), json!(0));
    }
}
