//! API-key management: masked listing, add, delete, re-encryption, and
//! enable/disable operations.

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::Json;
use serde_json::{json, Value};

use crate::crypto::{decrypt, encrypt, mask_key};
use crate::db::connection::db;
use crate::db::schema::{ApiKeyRow, API_KEY_COLS};
use crate::types::PLATFORMS;

/// Uses the established validation type-name strings — integers and
/// floats both report "number".
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

/// `isCustomPlatform`/`platformExists` from keys.ts — a built-in platform or
/// any `custom:`-prefixed provider id.
fn platform_exists(platform: &str) -> bool {
    PLATFORMS.contains(&platform) || platform.starts_with("custom:")
}

/// `parseInt(param, 10)` for the id path params. Returns `None` for NaN.
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

/// Validate an API-key creation body and retain the established error text.
fn validate_add_key(body: &Value) -> Result<(String, String, Option<String>), String> {
    let mut errors: Vec<String> = Vec::new();

    // platform: z.string().min(1).refine(platformExists, { message: ... })
    match body.get("platform") {
        None => errors.push("Required".to_string()),
        Some(Value::String(s)) => {
            if s.is_empty() {
                errors.push("String must contain at least 1 character(s)".to_string());
            }
            if !platform_exists(s) {
                errors.push(
                    "Unknown platform. Use a built-in platform or custom provider id.".to_string(),
                );
            }
        }
        Some(other) => errors.push(format!("Expected string, received {}", type_name(other))),
    }

    // key: z.string().min(1)
    match body.get("key") {
        None => errors.push("Required".to_string()),
        Some(Value::String(s)) if s.is_empty() => {
            errors.push("String must contain at least 1 character(s)".to_string());
        }
        Some(Value::String(_)) => {}
        Some(other) => errors.push(format!("Expected string, received {}", type_name(other))),
    }

    // label: z.string().optional()
    if let Some(l) = body.get("label") {
        if l.as_str().is_none() {
            errors.push(format!("Expected string, received {}", type_name(l)));
        }
    }

    if !errors.is_empty() {
        return Err(errors.join(", "));
    }
    Ok((
        body.get("platform")
            .and_then(|p| p.as_str())
            .unwrap_or_default()
            .to_string(),
        body.get("key")
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string(),
        body.get("label")
            .and_then(|l| l.as_str())
            .map(|s| s.to_string()),
    ))
}

/// `GET /` — list all keys (masked).
async fn list_keys() -> Response {
    let rows: Vec<ApiKeyRow> = {
        let conn = db().lock().await;
        let sql = format!("SELECT {API_KEY_COLS} FROM api_keys ORDER BY created_at DESC, id DESC");
        match conn.prepare(&sql).and_then(|mut s| {
            s.query_map([], ApiKeyRow::from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()
        }) {
            Ok(rows) => rows,
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        }
    };

    let keys: Vec<Value> = rows
        .iter()
        .map(|row| {
            let masked = match decrypt(&row.encrypted_key, &row.iv, &row.auth_tag) {
                Ok(real) => mask_key(&real),
                Err(_) => "[decrypt failed]".to_string(),
            };
            json!({
                "id": row.id,
                "platform": row.platform,
                "label": row.label,
                "maskedKey": masked,
                "status": row.status,
                "enabled": row.enabled != 0,
                "createdAt": row.created_at,
                "lastCheckedAt": row.last_checked_at,
            })
        })
        .collect();

    Json(keys).into_response()
}

/// `POST /` — add a key.
async fn add_key(body: Bytes) -> Response {
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };
    let (platform, key, label) = match validate_add_key(&raw) {
        Ok(t) => t,
        Err(msg) => return json_error(400, &msg),
    };
    let (encrypted, iv, auth_tag) = encrypt(&key);
    let label = label.unwrap_or_default();

    let id: i64 = {
        let conn = db().lock().await;
        match conn.execute(
            "INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'unknown', 1)",
            rusqlite::params![platform, label, encrypted, iv, auth_tag],
        ) {
            Ok(_) => conn.last_insert_rowid(),
            Err(e) => return crate::app::error_text(500, &e.to_string()),
        }
    };

    (
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "platform": platform,
            "label": label,
            "maskedKey": mask_key(&key),
            "status": "unknown",
            "enabled": true,
        })),
    )
        .into_response()
}

/// `DELETE /:id`
async fn delete_key(Path(id): Path<String>) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid key ID");
    };

    let changed = {
        let conn = db().lock().await;
        conn.execute("DELETE FROM api_keys WHERE id = ?1", rusqlite::params![id])
    };

    match changed {
        Ok(0) => json_error(404, "Key not found"),
        Ok(_) => Json(json!({ "success": true })).into_response(),
        Err(e) => crate::app::error_text(500, &e.to_string()),
    }
}

/// `PATCH /:id` — re-encrypt the key value (decryption-failed keys) or toggle
/// `enabled`. Missing-key cases return 400 for API compatibility.
async fn patch_key(Path(id): Path<String>, body: Bytes) -> Response {
    let Some(id) = parse_id(&id) else {
        return json_error(400, "Invalid key ID");
    };
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return malformed_body(),
    };

    // Re-encrypt only when a non-empty replacement key was supplied.
    if let Some(key) = raw
        .get("key")
        .and_then(|k| k.as_str())
        .filter(|k| !k.is_empty())
    {
        let (encrypted, iv, auth_tag) = encrypt(key);
        let changed = {
            let conn = db().lock().await;
            conn.execute(
                "UPDATE api_keys SET encrypted_key = ?1, iv = ?2, auth_tag = ?3, status = 'unknown' \
                 WHERE id = ?4",
                rusqlite::params![encrypted, iv, auth_tag, id],
            )
        };
        return match changed {
            Ok(0) => json_error(400, "Key not found"),
            Ok(_) => Json(json!({
                "success": true,
                "reEncrypted": true,
                "maskedKey": mask_key(key),
            }))
            .into_response(),
            Err(e) => crate::app::error_text(500, &e.to_string()),
        };
    }

    // Toggle enabled branch — typeof body.enabled === 'boolean'
    if let Some(enabled) = raw.get("enabled").and_then(|e| e.as_bool()) {
        let changed = {
            let conn = db().lock().await;
            conn.execute(
                "UPDATE api_keys SET enabled = ?1 WHERE id = ?2",
                rusqlite::params![if enabled { 1 } else { 0 }, id],
            )
        };
        return match changed {
            Ok(0) => json_error(400, "Key not found"),
            Ok(_) => Json(json!({ "success": true, "enabled": enabled })).into_response(),
            Err(e) => crate::app::error_text(500, &e.to_string()),
        };
    }

    json_error(
        400,
        "Provide either \"key\" (string) or \"enabled\" (boolean)",
    )
}

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/", get(list_keys).post(add_key))
        .route("/{id}", delete(delete_key).patch(patch_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_exists_matches_ts() {
        assert!(platform_exists("google"));
        assert!(platform_exists("custom:5"));
        assert!(platform_exists("custom:notanumber"));
        assert!(!platform_exists("nonexistent"));
    }

    #[test]
    fn parse_id_matches_parse_int() {
        assert_eq!(parse_id("12"), Some(12));
        assert_eq!(parse_id("-1"), Some(-1));
        assert_eq!(parse_id("abc"), None);
        assert_eq!(parse_id(""), None);
    }

    #[test]
    fn validate_add_key_matches_zod_messages() {
        let body = json!({ "platform": "", "key": "" });
        assert_eq!(
            validate_add_key(&body).unwrap_err(),
            "String must contain at least 1 character(s), Unknown platform. Use a built-in platform or custom provider id., String must contain at least 1 character(s)"
        );

        let body = json!({ "platform": "nope", "key": "k" });
        assert_eq!(
            validate_add_key(&body).unwrap_err(),
            "Unknown platform. Use a built-in platform or custom provider id."
        );

        let body = json!({ "platform": "google", "key": "sk-123", "label": null });
        assert_eq!(
            validate_add_key(&body).unwrap_err(),
            "Expected string, received null"
        );

        let body = json!({ "key": "k" });
        assert_eq!(validate_add_key(&body).unwrap_err(), "Required");

        let body = json!({ "platform": "google", "key": "sk-123", "label": "lbl" });
        let (platform, key, label) = validate_add_key(&body).unwrap();
        assert_eq!(platform, "google");
        assert_eq!(key, "sk-123");
        assert_eq!(label, Some("lbl".to_string()));
    }
}
