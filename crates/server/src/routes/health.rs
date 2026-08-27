//! Port of `server/src/routes/health.ts` — per-platform health summary listing
//! and the manual key-check endpoints.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde_json::{json, Value};

use crate::db::connection::db;
use crate::db::schema::{ApiKeyRow, API_KEY_COLS};
use crate::providers::has_provider;
use crate::services::health::{check_all_keys, check_key_health};

type PlatformHealthRow = (String, i64, i64, i64, i64, i64, i64, i64);

/// `parseInt(param, 10)`-style id parse; `None` for NaN.
fn parse_id(s: &str) -> Option<i64> {
    s.parse::<i64>().ok()
}

fn json_error(status: u16, message: &str) -> Response {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
        Json(json!({ "error": { "message": message } })),
    )
        .into_response()
}

/// `GET /` — per-platform key-status aggregation plus the full key list.
async fn get_health_status() -> Response {
    let (platform_rows, keys): (
        Vec<PlatformHealthRow>,
        Vec<ApiKeyRow>,
    ) = {
        let conn = db().lock().await;

        let platforms_sql = {
            "SELECT platform, COUNT(*), \
                    SUM(CASE WHEN status = 'healthy' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN status = 'rate_limited' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN status = 'invalid' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN status = 'unknown' THEN 1 ELSE 0 END), \
                    SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END) \
             FROM api_keys GROUP BY platform"
        };
        let aggs: rusqlite::Result<Vec<PlatformHealthRow>> =
            conn.prepare(platforms_sql).and_then(|mut s| {
                s.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, i64>(7)?,
                    ))
                })?
                .collect()
            });

        let keys_sql = format!(
            "SELECT {API_KEY_COLS} FROM api_keys ORDER BY platform, created_at DESC"
        );
        let keys: rusqlite::Result<Vec<ApiKeyRow>> = conn
            .prepare(&keys_sql)
            .and_then(|mut s| {
                s.query_map([], ApiKeyRow::from_row)?.collect::<rusqlite::Result<Vec<_>>>()
            });

        match (aggs, keys) {
            (Ok(a), Ok(k)) => (a, k),
            (Err(e), _) | (_, Err(e)) => return crate::app::error_text(500, &e.to_string()),
        }
    };

    // `hasProvider` locks the DB internally, so run it after the guard above is dropped.
    let mut platforms_json = Vec::with_capacity(platform_rows.len());
    for (platform, total_keys, healthy, rate_limited, invalid, error, unknown, enabled_keys) in
        platform_rows
    {
        let has = has_provider(&platform).await;
        platforms_json.push(json!({
            "platform": platform,
            "hasProvider": has,
            "totalKeys": total_keys,
            "healthyKeys": healthy,
            "rateLimitedKeys": rate_limited,
            "invalidKeys": invalid,
            "errorKeys": error,
            "unknownKeys": unknown,
            "enabledKeys": enabled_keys,
        }));
    }

    let keys_json: Vec<Value> = keys
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "platform": k.platform,
                "label": k.label,
                "status": k.status,
                "enabled": k.enabled != 0,
                "createdAt": k.created_at,
                "lastCheckedAt": k.last_checked_at,
            })
        })
        .collect();

    Json(json!({ "platforms": platforms_json, "keys": keys_json })).into_response()
}

/// `POST /check/:keyId`
async fn check_key(Path(key_id): Path<String>) -> Response {
    let Some(key_id) = parse_id(&key_id) else {
        return json_error(400, "Invalid key ID");
    };
    let status = check_key_health(key_id).await;
    Json(json!({ "keyId": key_id, "status": status })).into_response()
}

/// `POST /check-all`
async fn check_all() -> Response {
    check_all_keys().await;
    Json(json!({ "success": true })).into_response()
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/", get(get_health_status))
        .route("/check/{keyId}", axum::routing::post(check_key))
        .route("/check-all", axum::routing::post(check_all))
}
