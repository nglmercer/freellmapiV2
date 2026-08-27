//! Custom-provider CRUD and per-provider custom-model endpoints.

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch};
use axum::Json;
use serde_json::{json, Map, Value};

use crate::db::connection::db;
use crate::db::schema::{
    query_rows, CustomProviderRow, ModelRow, CUSTOM_PROVIDER_COLS, MODEL_COLS,
};
use crate::providers::provider_id_to_platform;

/// Return the established validation type-name strings.
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `parseInt(param, 10)`-style parse; `None` for NaN.
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

fn malformed_body() -> Response {
    json_error(400, "Malformed JSON body")
}

struct StringConstraints {
    min: bool,
    max: Option<usize>,
}

/// `z.string().url()` — requires an http/https/ftp scheme, `://`, and a
/// non-empty host portion before any path/query/fragment/port terminator.
fn valid_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    let rest = match lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))
        .or_else(|| lower.strip_prefix("ftp://"))
    {
        Some(r) => r,
        None => return false,
    };
    !rest.is_empty()
        && rest
            .chars()
            .next()
            .map(|c| !matches!(c, '/' | '\\' | '?' | '#' | ':'))
            .unwrap_or(false)
        && !rest.chars().any(char::is_whitespace)
}

/// `toProviderJson(row)` — extra headers column is stored JSON (or NULL);
/// `JSON.parse` failures fall back to `null`.
fn provider_json(row: &CustomProviderRow) -> Value {
    let extra_headers = match &row.extra_headers {
        Some(raw) => serde_json::from_str::<Value>(raw).unwrap_or(Value::Null),
        None => Value::Null,
    };
    json!({
        "id": row.id,
        "name": row.name,
        "baseUrl": row.base_url,
        "timeoutMs": row.timeout_ms,
        "extraHeaders": extra_headers,
        "enabled": row.enabled != 0,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
    })
}

/// `{ ...m, enabled: m.enabled === 1 }` — the full models-row spread with all
/// columns camelCased (the dashboard reads these shapes directly).
fn model_row_json(m: &ModelRow) -> Value {
    json!({
        "id": m.id,
        "platform": m.platform,
        "modelId": m.model_id,
        "displayName": m.display_name,
        "intelligenceRank": m.intelligence_rank,
        "speedRank": m.speed_rank,
        "sizeLabel": m.size_label,
        "rpmLimit": m.rpm_limit,
        "rpdLimit": m.rpd_limit,
        "tpmLimit": m.tpm_limit,
        "tpdLimit": m.tpd_limit,
        "monthlyTokenBudget": m.monthly_token_budget,
        "contextWindow": m.context_window,
        "enabled": m.enabled != 0,
        "pricingPrompt": m.pricing_prompt,
        "pricingCompletion": m.pricing_completion,
        "freeTier": m.free_tier,
        "gateway": m.gateway,
        "supportedFeatures": m.supported_features,
        "externalUrl": m.external_url,
        "description": m.description,
        "lastSyncedAt": m.last_synced_at,
        "source": m.source,
        "intelligenceScore": m.intelligence_score,
        "speedTokensPerSec": m.speed_tokens_per_sec,
        "rankingSource": m.ranking_source,
        "lastRankedAt": m.last_ranked_at,
    })
}

// ---- zod-replication helpers ----

/// String field with optional string checks; `required=false` means a missing
/// key is fine (returns `default` when given, else `None`).
fn string_field(
    body: &Value,
    key: &str,
    constraints: StringConstraints,
    url: bool,
    required: bool,
    default: Option<&str>,
    errors: &mut Vec<String>,
) -> Option<String> {
    match body.get(key) {
        None => {
            if required {
                errors.push("Required".to_string());
            }
            default.map(|s| s.to_string())
        }
        Some(Value::String(s)) => {
            if url && !valid_url(s) {
                errors.push("Invalid url".to_string());
            }
            if constraints.min && s.is_empty() {
                errors.push("String must contain at least 1 character(s)".to_string());
            }
            if let Some(max) = constraints.max {
                if s.chars().count() > max {
                    errors.push(format!("String must contain at most {max} character(s)"));
                }
            }
            Some(s.clone())
        }
        Some(other) => {
            errors.push(format!("Expected string, received {}", type_name(other)));
            default.map(|s| s.to_string())
        }
    }
}

/// Optional string (update schemas) — `None` when the key is absent.
fn optional_string(
    body: &Value,
    key: &str,
    min: bool,
    max: Option<usize>,
    url: bool,
    errors: &mut Vec<String>,
) -> Option<String> {
    string_field(
        body,
        key,
        StringConstraints { min, max },
        url,
        false,
        None,
        errors,
    )
}

/// Number field with zod's `int()`/`min`/`max` check messages plus a default
/// for absent keys (create schemas).
fn int_field(
    body: &Value,
    key: &str,
    nullable: bool,
    min: Option<i64>,
    max: Option<i64>,
    default: Option<i64>,
    errors: &mut Vec<String>,
) -> Option<i64> {
    match body.get(key) {
        None => default,
        Some(Value::Null) => {
            if !nullable {
                errors.push("Expected number, received null".to_string());
            }
            default
        }
        Some(v) => match v.as_f64() {
            None => {
                errors.push(format!("Expected number, received {}", type_name(v)));
                default
            }
            Some(f) => {
                if f.fract() != 0.0 {
                    errors.push("Expected integer, received float".to_string());
                }
                if let Some(min) = min {
                    if f < min as f64 {
                        errors.push(format!("Number must be greater than or equal to {min}"));
                    }
                }
                if let Some(max) = max {
                    if f > max as f64 {
                        errors.push(format!("Number must be less than or equal to {max}"));
                    }
                }
                Some(f as i64)
            }
        },
    }
}

/// Optional number (update schemas) — outer `None` = key absent; inner `None`
/// = explicit JSON `null`.
fn optional_int(
    body: &Value,
    key: &str,
    nullable: bool,
    min: Option<i64>,
    max: Option<i64>,
    errors: &mut Vec<String>,
) -> Option<Option<i64>> {
    match body.get(key) {
        None => None,
        Some(Value::Null) => {
            if nullable {
                Some(None)
            } else {
                errors.push("Expected number, received null".to_string());
                None
            }
        }
        Some(v) => match v.as_f64() {
            None => {
                errors.push(format!("Expected number, received {}", type_name(v)));
                Some(None)
            }
            Some(f) => {
                if f.fract() != 0.0 {
                    errors.push("Expected integer, received float".to_string());
                }
                if let Some(min) = min {
                    if f < min as f64 {
                        errors.push(format!("Number must be greater than or equal to {min}"));
                    }
                }
                if let Some(max) = max {
                    if f > max as f64 {
                        errors.push(format!("Number must be less than or equal to {max}"));
                    }
                }
                Some(Some(f as i64))
            }
        },
    }
}

fn bool_field(
    body: &Value,
    key: &str,
    optional: bool,
    default: bool,
    errors: &mut Vec<String>,
) -> Option<bool> {
    match body.get(key) {
        None => {
            if !optional {
                errors.push("Required".to_string());
            }
            Some(default)
        }
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Null) => {
            errors.push("Expected boolean, received null".to_string());
            Some(default)
        }
        Some(other) => {
            errors.push(format!("Expected boolean, received {}", type_name(other)));
            Some(default)
        }
    }
}

/// Optional boolean (update schemas) — `None` when the key is absent.
fn optional_bool(body: &Value, key: &str, errors: &mut Vec<String>) -> Option<bool> {
    match body.get(key) {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Null) => {
            errors.push("Expected boolean, received null".to_string());
            None
        }
        Some(other) => {
            errors.push(format!("Expected boolean, received {}", type_name(other)));
            None
        }
    }
}

/// `z.record(z.string(), z.string())` — outer `None` = key absent; inner
/// `None` = explicit `null` (allowed only when `nullable`).
fn record_field(
    body: &Value,
    key: &str,
    nullable: bool,
    errors: &mut Vec<String>,
) -> Option<Option<Map<String, Value>>> {
    match body.get(key) {
        None => None,
        Some(Value::Null) => {
            if nullable {
                Some(None)
            } else {
                errors.push("Expected object, received null".to_string());
                None
            }
        }
        Some(Value::Object(m)) => {
            for v in m.values() {
                if !v.is_string() {
                    errors.push(format!("Expected string, received {}", type_name(v)));
                }
            }
            Some(Some(m.clone()))
        }
        Some(other) => {
            errors.push(format!("Expected object, received {}", type_name(other)));
            None
        }
    }
}

// ---- createSchema / updateSchema parsing ----

#[derive(Debug)]
struct CreateProviderInput {
    name: String,
    base_url: String,
    timeout_ms: i64,
    extra_headers: Option<Map<String, Value>>,
    enabled: bool,
}

fn validate_create_provider(body: &Value) -> Result<CreateProviderInput, String> {
    let mut errors: Vec<String> = Vec::new();
    let name = string_field(
        body,
        "name",
        StringConstraints {
            min: true,
            max: Some(100),
        },
        false,
        true,
        None,
        &mut errors,
    )
    .unwrap_or_default();
    let base_url = string_field(
        body,
        "baseUrl",
        StringConstraints {
            min: true,
            max: None,
        },
        true,
        true,
        None,
        &mut errors,
    )
    .unwrap_or_default();
    let timeout_ms = int_field(
        body,
        "timeoutMs",
        false,
        Some(1000),
        Some(300000),
        Some(15000),
        &mut errors,
    );
    let extra_headers = record_field(body, "extraHeaders", false, &mut errors).flatten();
    let enabled = bool_field(body, "enabled", true, true, &mut errors).unwrap_or(true);

    if !errors.is_empty() {
        return Err(errors.join(", "));
    }
    Ok(CreateProviderInput {
        name,
        base_url,
        timeout_ms: timeout_ms.unwrap_or(15000),
        extra_headers,
        enabled,
    })
}

#[derive(Debug)]
struct UpdateProviderInput {
    name: Option<String>,
    base_url: Option<String>,
    timeout_ms: Option<i64>,
    extra_headers: Option<Option<Map<String, Value>>>,
    enabled: Option<bool>,
}

fn validate_update_provider(body: &Value) -> Result<UpdateProviderInput, String> {
    let mut errors: Vec<String> = Vec::new();
    let name = optional_string(body, "name", true, Some(100), false, &mut errors);
    let base_url = optional_string(body, "baseUrl", true, None, true, &mut errors);
    let timeout_ms = optional_int(
        body,
        "timeoutMs",
        false,
        Some(1000),
        Some(300000),
        &mut errors,
    )
    .flatten();
    let extra_headers = record_field(body, "extraHeaders", true, &mut errors);
    let enabled = optional_bool(body, "enabled", &mut errors);

    if !errors.is_empty() {
        return Err(errors.join(", "));
    }
    Ok(UpdateProviderInput {
        name,
        base_url,
        timeout_ms,
        extra_headers,
        enabled,
    })
}

#[derive(Debug)]
struct CreateModelInput {
    model_id: String,
    display_name: String,
    intelligence_rank: i64,
    speed_rank: i64,
    size_label: String,
    rpm_limit: Option<i64>,
    rpd_limit: Option<i64>,
    tpm_limit: Option<i64>,
    tpd_limit: Option<i64>,
    monthly_token_budget: String,
    context_window: Option<i64>,
    enabled: bool,
}

fn validate_create_model(body: &Value) -> Result<CreateModelInput, String> {
    let mut errors: Vec<String> = Vec::new();
    let model_id = string_field(
        body,
        "modelId",
        StringConstraints {
            min: true,
            max: None,
        },
        false,
        true,
        None,
        &mut errors,
    )
    .unwrap_or_default();
    let display_name = string_field(
        body,
        "displayName",
        StringConstraints {
            min: true,
            max: None,
        },
        false,
        true,
        None,
        &mut errors,
    )
    .unwrap_or_default();
    let intelligence_rank = int_field(
        body,
        "intelligenceRank",
        false,
        Some(1),
        Some(999),
        Some(99),
        &mut errors,
    );
    let speed_rank = int_field(
        body,
        "speedRank",
        false,
        Some(1),
        Some(999),
        Some(10),
        &mut errors,
    );
    let size_label = string_field(
        body,
        "sizeLabel",
        StringConstraints {
            min: false,
            max: None,
        },
        false,
        false,
        Some(""),
        &mut errors,
    )
    .unwrap_or_default();
    let rpm_limit = int_field(body, "rpmLimit", true, None, None, None, &mut errors);
    let rpd_limit = int_field(body, "rpdLimit", true, None, None, None, &mut errors);
    let tpm_limit = int_field(body, "tpmLimit", true, None, None, None, &mut errors);
    let tpd_limit = int_field(body, "tpdLimit", true, None, None, None, &mut errors);
    let monthly_token_budget = string_field(
        body,
        "monthlyTokenBudget",
        StringConstraints {
            min: false,
            max: None,
        },
        false,
        false,
        Some(""),
        &mut errors,
    )
    .unwrap_or_default();
    let context_window = int_field(body, "contextWindow", true, None, None, None, &mut errors);
    let enabled = bool_field(body, "enabled", true, true, &mut errors).unwrap_or(true);

    if !errors.is_empty() {
        return Err(errors.join(", "));
    }
    Ok(CreateModelInput {
        model_id,
        display_name,
        intelligence_rank: intelligence_rank.unwrap_or(99),
        speed_rank: speed_rank.unwrap_or(10),
        size_label,
        rpm_limit,
        rpd_limit,
        tpm_limit,
        tpd_limit,
        monthly_token_budget,
        context_window,
        enabled,
    })
}

#[derive(Debug)]
struct UpdateModelInput {
    display_name: Option<String>,
    intelligence_rank: Option<i64>,
    speed_rank: Option<i64>,
    size_label: Option<String>,
    rpm_limit: Option<Option<i64>>,
    rpd_limit: Option<Option<i64>>,
    tpm_limit: Option<Option<i64>>,
    tpd_limit: Option<Option<i64>>,
    monthly_token_budget: Option<String>,
    context_window: Option<Option<i64>>,
    enabled: Option<bool>,
}

fn validate_update_model(body: &Value) -> Result<UpdateModelInput, String> {
    let mut errors: Vec<String> = Vec::new();
    let display_name = optional_string(body, "displayName", true, None, false, &mut errors);
    let intelligence_rank = optional_int(
        body,
        "intelligenceRank",
        false,
        Some(1),
        Some(999),
        &mut errors,
    )
    .flatten();
    let speed_rank =
        optional_int(body, "speedRank", false, Some(1), Some(999), &mut errors).flatten();
    let size_label = optional_string(body, "sizeLabel", false, None, false, &mut errors);
    let rpm_limit = optional_int(body, "rpmLimit", true, None, None, &mut errors);
    let rpd_limit = optional_int(body, "rpdLimit", true, None, None, &mut errors);
    let tpm_limit = optional_int(body, "tpmLimit", true, None, None, &mut errors);
    let tpd_limit = optional_int(body, "tpdLimit", true, None, None, &mut errors);
    let monthly_token_budget =
        optional_string(body, "monthlyTokenBudget", false, None, false, &mut errors);
    let context_window = optional_int(body, "contextWindow", true, None, None, &mut errors);
    let enabled = optional_bool(body, "enabled", &mut errors);

    if !errors.is_empty() {
        return Err(errors.join(", "));
    }
    Ok(UpdateModelInput {
        display_name,
        intelligence_rank,
        speed_rank,
        size_label,
        rpm_limit,
        rpd_limit,
        tpm_limit,
        tpd_limit,
        monthly_token_budget,
        context_window,
        enabled,
    })
}

// ---- handlers ----

/// `GET /` — list all custom providers (newest first).
async fn list_providers() -> Response {
    let rows: Vec<CustomProviderRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers ORDER BY id DESC");
        match query_rows(&conn, &sql, CustomProviderRow::from_row) {
            Ok(rows) => rows,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        }
    };
    let out: Vec<Value> = rows.iter().map(provider_json).collect();
    Json(out).into_response()
}

/// `GET /:id`
async fn get_provider_by_id(Path(id): Path<String>) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid ID");
    };
    let row: Option<CustomProviderRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers WHERE id = ?1");
        conn.query_row(&sql, rusqlite::params![id], CustomProviderRow::from_row)
            .ok()
    };
    match row {
        Some(r) => Json(provider_json(&r)).into_response(),
        None => json_error(404, "Provider not found"),
    }
}

/// `POST /` — create a custom provider.
async fn create_provider(body: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };
    let input = match validate_create_provider(&raw) {
        Ok(i) => i,
        Err(msg) => return json_error(400, &msg),
    };

    // name conflict check
    let name_conflict: bool = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT id FROM custom_providers WHERE name = ?1",
            rusqlite::params![input.name],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    };
    if name_conflict {
        return json_error(409, "A provider with this name already exists");
    }

    let extra_headers_json = input
        .extra_headers
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string()));

    let id: i64 = {
        let conn = db().lock().await;
        match conn.execute(
            "INSERT INTO custom_providers (name, base_url, timeout_ms, extra_headers, enabled) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                input.name,
                input.base_url,
                input.timeout_ms,
                extra_headers_json,
                if input.enabled { 1 } else { 0 }
            ],
        ) {
            Ok(_) => conn.last_insert_rowid(),
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        }
    };

    let inserted: Option<CustomProviderRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers WHERE id = ?1");
        conn.query_row(&sql, rusqlite::params![id], CustomProviderRow::from_row)
            .ok()
    };

    (
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "provider": inserted.as_ref().map(provider_json).unwrap_or(Value::Null),
        })),
    )
        .into_response()
}

/// `PATCH /:id` updates the timestamp even when no optional field changes.
async fn update_provider(Path(id): Path<String>, body: Bytes) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid ID");
    };
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };
    let input = match validate_update_provider(&raw) {
        Ok(i) => i,
        Err(msg) => return json_error(400, &msg),
    };

    let exists: bool = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT id FROM custom_providers WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    };
    if !exists {
        return json_error(404, "Provider not found");
    }

    let mut cols: Vec<&str> = Vec::new();
    let mut vals: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(name) = &input.name {
        cols.push("name");
        vals.push(rusqlite::types::Value::from(name.clone()));
    }
    if let Some(url) = &input.base_url {
        cols.push("base_url");
        vals.push(rusqlite::types::Value::from(url.clone()));
    }
    if let Some(ms) = input.timeout_ms {
        cols.push("timeout_ms");
        vals.push(rusqlite::types::Value::from(ms));
    }
    if let Some(eh) = &input.extra_headers {
        cols.push("extra_headers");
        vals.push(match eh {
            Some(m) => rusqlite::types::Value::from(
                serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string()),
            ),
            None => rusqlite::types::Value::Null,
        });
    }
    if let Some(en) = input.enabled {
        cols.push("enabled");
        vals.push(rusqlite::types::Value::from(if en { 1 } else { 0 }));
    }
    // Always update the timestamp, including for an empty patch body.
    cols.push("updated_at");
    vals.push(rusqlite::types::Value::from(
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    ));

    let updated: Option<CustomProviderRow> = {
        let conn = db().lock().await;
        let set_clause = cols
            .iter()
            .map(|c| format!("{c} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE custom_providers SET {set_clause} WHERE id = ?");
        let mut params = vals;
        params.push(rusqlite::types::Value::from(id));
        if let Err(e) = conn.execute(&sql, rusqlite::params_from_iter(params.iter())) {
            return crate::app::error_text(500, &e.to_string());
        }
        let sql = format!("SELECT {CUSTOM_PROVIDER_COLS} FROM custom_providers WHERE id = ?1");
        conn.query_row(&sql, rusqlite::params![id], CustomProviderRow::from_row)
            .ok()
    };

    Json(json!({
        "success": true,
        "provider": updated.as_ref().map(provider_json).unwrap_or(Value::Null),
    }))
    .into_response()
}

/// `DELETE /:id` — cascade-deletes the provider's models, their fallback
/// config, its api keys, then the provider row itself.
async fn delete_provider(Path(id): Path<String>) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid ID");
    };

    let exists: bool = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT id FROM custom_providers WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    };
    if !exists {
        return json_error(404, "Provider not found");
    }

    let platform = provider_id_to_platform(id);
    let result: Result<(), rusqlite::Error> = async {
        let conn = db().lock().await;
        let model_ids: Vec<i64> = conn
            .prepare("SELECT id FROM models WHERE platform = ?1")
            .and_then(|mut s| {
                s.query_map(rusqlite::params![platform], |r| r.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })?;
        for m in model_ids {
            conn.execute(
                "DELETE FROM fallback_config WHERE model_db_id = ?1",
                rusqlite::params![m],
            )?;
        }
        conn.execute(
            "DELETE FROM models WHERE platform = ?1",
            rusqlite::params![platform],
        )?;
        conn.execute(
            "DELETE FROM api_keys WHERE platform = ?1",
            rusqlite::params![platform],
        )?;
        conn.execute(
            "DELETE FROM custom_providers WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => Json(json!({ "success": true })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

/// `GET /:id/models` lists the custom provider's models without a separate
/// provider-existence check.
async fn list_provider_models(Path(id): Path<String>) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid ID");
    };
    let platform = provider_id_to_platform(id);
    let rows: Vec<ModelRow> = {
        let conn = db().lock().await;
        let sql = format!(
            "SELECT {MODEL_COLS} FROM models WHERE platform = ?1 ORDER BY intelligence_rank"
        );
        match conn.prepare(&sql).and_then(|mut s| {
            s.query_map(rusqlite::params![platform], ModelRow::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()
        }) {
            Ok(rows) => rows,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        }
    };
    let out: Vec<Value> = rows.iter().map(model_row_json).collect();
    Json(out).into_response()
}

/// `POST /:id/models` — create a custom model. When enabled, also adds it to
/// the fallback config with the next priority.
async fn create_provider_model(Path(id): Path<String>, body: Bytes) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid ID");
    };
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };
    let input = match validate_create_model(&raw) {
        Ok(i) => i,
        Err(msg) => return json_error(400, &msg),
    };

    let platform = provider_id_to_platform(id);

    // Provider must exist (404), then model must not already exist (409).
    let provider_exists: bool = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT id FROM custom_providers WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    };
    if !provider_exists {
        return json_error(404, "Provider not found");
    }

    let model_exists: bool = {
        let conn = db().lock().await;
        conn.query_row(
            "SELECT id FROM models WHERE platform = ?1 AND model_id = ?2",
            rusqlite::params![platform, input.model_id],
            |r| r.get::<_, i64>(0),
        )
        .is_ok()
    };
    if model_exists {
        return json_error(409, "Model already exists");
    }

    let result: Result<(), rusqlite::Error> = async {
        let conn = db().lock().await;
        conn.execute(
            "INSERT INTO models (platform, model_id, display_name, intelligence_rank, speed_rank, \
             size_label, rpm_limit, rpd_limit, tpm_limit, tpd_limit, monthly_token_budget, \
             context_window, enabled, source, free_tier) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'custom', 0)",
            rusqlite::params![
                platform,
                input.model_id,
                input.display_name,
                input.intelligence_rank,
                input.speed_rank,
                input.size_label,
                input.rpm_limit,
                input.rpd_limit,
                input.tpm_limit,
                input.tpd_limit,
                input.monthly_token_budget,
                input.context_window,
                if input.enabled { 1 } else { 0 }
            ],
        )?;
        if input.enabled {
            let last_id = conn.last_insert_rowid();
            let mx: Option<i64> = conn
                .query_row("SELECT MAX(priority) FROM fallback_config", [], |r| {
                    r.get(0)
                })
                .ok()
                .flatten();
            conn.execute(
                "INSERT INTO fallback_config (model_db_id, priority, enabled) VALUES (?1, ?2, 1)",
                rusqlite::params![last_id, mx.unwrap_or(0) + 1],
            )?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => (StatusCode::CREATED, Json(json!({ "success": true }))).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

/// `PATCH /:id/models/:modelId`
async fn update_provider_model(
    Path((id, model_id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid provider ID");
    };
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };
    let input = match validate_update_model(&raw) {
        Ok(i) => i,
        Err(msg) => return json_error(400, &msg),
    };
    let platform = provider_id_to_platform(id);

    let model: Option<ModelRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {MODEL_COLS} FROM models WHERE platform = ?1 AND model_id = ?2");
        conn.query_row(
            &sql,
            rusqlite::params![platform, model_id],
            ModelRow::from_row,
        )
        .ok()
    };
    let Some(model) = model else {
        return json_error(404, "Model not found");
    };

    let mut cols: Vec<&str> = Vec::new();
    let mut vals: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(v) = &input.display_name {
        cols.push("display_name");
        vals.push(rusqlite::types::Value::from(v.clone()));
    }
    if let Some(v) = input.intelligence_rank {
        cols.push("intelligence_rank");
        vals.push(rusqlite::types::Value::from(v));
    }
    if let Some(v) = input.speed_rank {
        cols.push("speed_rank");
        vals.push(rusqlite::types::Value::from(v));
    }
    if let Some(v) = &input.size_label {
        cols.push("size_label");
        vals.push(rusqlite::types::Value::from(v.clone()));
    }
    for (_, col, val) in [
        ("rpmLimit", "rpm_limit", &input.rpm_limit),
        ("rpdLimit", "rpd_limit", &input.rpd_limit),
        ("tpmLimit", "tpm_limit", &input.tpm_limit),
        ("tpdLimit", "tpd_limit", &input.tpd_limit),
    ] {
        if let Some(inner) = val {
            cols.push(col);
            vals.push(match inner {
                Some(v) => rusqlite::types::Value::from(*v),
                None => rusqlite::types::Value::Null,
            });
        }
    }
    if let Some(v) = &input.monthly_token_budget {
        cols.push("monthly_token_budget");
        vals.push(rusqlite::types::Value::from(v.clone()));
    }
    if let Some(inner) = &input.context_window {
        cols.push("context_window");
        vals.push(match inner {
            Some(v) => rusqlite::types::Value::from(*v),
            None => rusqlite::types::Value::Null,
        });
    }
    if let Some(v) = input.enabled {
        cols.push("enabled");
        vals.push(rusqlite::types::Value::from(if v { 1 } else { 0 }));
    }

    if cols.is_empty() {
        return json_error(400, "No fields to update");
    }

    let result: Result<(), rusqlite::Error> = async {
        let conn = db().lock().await;
        let set_clause = cols
            .iter()
            .map(|c| format!("{c} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE models SET {set_clause} WHERE platform = ? AND model_id = ?");
        let mut params = vals;
        params.push(rusqlite::types::Value::from(platform.clone()));
        params.push(rusqlite::types::Value::from(model.model_id.clone()));
        conn.execute(&sql, rusqlite::params_from_iter(params.iter()))?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => Json(json!({ "success": true })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

/// `DELETE /:id/models/:modelId`
async fn delete_provider_model(Path((id, model_id)): Path<(String, String)>) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid provider ID");
    };
    let platform = provider_id_to_platform(id);

    let model: Option<ModelRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {MODEL_COLS} FROM models WHERE platform = ?1 AND model_id = ?2");
        conn.query_row(
            &sql,
            rusqlite::params![platform, model_id],
            ModelRow::from_row,
        )
        .ok()
    };
    let Some(model) = model else {
        return json_error(404, "Model not found");
    };

    let result: Result<(), rusqlite::Error> = async {
        let conn = db().lock().await;
        conn.execute(
            "DELETE FROM fallback_config WHERE model_db_id = ?1",
            rusqlite::params![model.id],
        )?;
        conn.execute(
            "DELETE FROM models WHERE id = ?1",
            rusqlite::params![model.id],
        )?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => Json(json!({ "success": true })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/", get(list_providers).post(create_provider))
        .route(
            "/{id}",
            get(get_provider_by_id)
                .patch(update_provider)
                .delete(delete_provider),
        )
        .route(
            "/{id}/models",
            get(list_provider_models).post(create_provider_model),
        )
        .route(
            "/{id}/models/{modelId}",
            patch(update_provider_model).delete(delete_provider_model),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_url_matches_zod_loose_regex() {
        assert!(valid_url("http://localhost:8000/v1"));
        assert!(valid_url("https://api.example.com"));
        assert!(valid_url("ftp://x.com"));
        assert!(valid_url("HTTP://EXAMPLE.COM"));
        assert!(!valid_url("foobar"));
        assert!(!valid_url("www.example.com"));
        assert!(!valid_url(""));
        assert!(!valid_url("http://"));
    }

    #[test]
    fn provider_json_shape() {
        let row = CustomProviderRow {
            id: 3,
            name: "Test".to_string(),
            base_url: "https://x.com".to_string(),
            timeout_ms: Some(20000),
            extra_headers: Some("{\"a\":\"b\"}".to_string()),
            enabled: 1,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
        };
        let v = provider_json(&row);
        assert_eq!(v["id"], 3);
        assert_eq!(v["baseUrl"], "https://x.com");
        assert_eq!(v["extraHeaders"]["a"], "b");
        assert_eq!(v["enabled"], true);

        let bad = CustomProviderRow {
            extra_headers: Some("not-json".to_string()),
            ..row
        };
        assert_eq!(provider_json(&bad)["extraHeaders"], Value::Null);
    }

    #[test]
    fn create_provider_zod_messages() {
        let body = json!({ "name": "", "baseUrl": "http://x.com" });
        assert_eq!(
            validate_create_provider(&body).unwrap_err(),
            "String must contain at least 1 character(s)"
        );
        let body = json!({ "name": "n", "baseUrl": "notaurl" });
        assert_eq!(validate_create_provider(&body).unwrap_err(), "Invalid url");
        let body = json!({ "name": "n", "baseUrl": "", "timeoutMs": 1.5 });
        assert_eq!(
            validate_create_provider(&body).unwrap_err(),
            "Invalid url, String must contain at least 1 character(s), Expected integer, received float, Number must be greater than or equal to 1000"
        );
        let body = json!({ "name": "n", "baseUrl": "http://x.com", "timeoutMs": null });
        assert_eq!(
            validate_create_provider(&body).unwrap_err(),
            "Expected number, received null"
        );
        let body = json!({ "name": "n", "baseUrl": "http://x.com", "extraHeaders": { "a": 5 } });
        assert_eq!(
            validate_create_provider(&body).unwrap_err(),
            "Expected string, received number"
        );
    }

    #[test]
    fn create_model_defaults_match_zod() {
        let body = json!({ "modelId": "m", "displayName": "d" });
        let input = validate_create_model(&body).unwrap();
        assert_eq!(input.intelligence_rank, 99);
        assert_eq!(input.speed_rank, 10);
        assert_eq!(input.size_label, "");
        assert_eq!(input.monthly_token_budget, "");
        assert!(input.enabled);
        assert_eq!(input.rpm_limit, None);
        assert_eq!(input.context_window, None);

        let body = json!({ "modelId": "m", "displayName": "d", "intelligenceRank": 5.5 });
        assert_eq!(
            validate_create_model(&body).unwrap_err(),
            "Expected integer, received float"
        );
    }
}
