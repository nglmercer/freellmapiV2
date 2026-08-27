//! Health route integration tests.

mod common;

use axum::http::StatusCode;
use serde_json::json;

async fn insert_key(platform: &str, label: &str, status: &str, enabled: i64) {
    let (encrypted, iv, auth_tag) = server::crypto::encrypt("fake-key");
    let conn = server::db::db().lock().await;
    conn.execute(
        "INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![platform, label, encrypted, iv, auth_tag, status, enabled],
    )
    .expect("insert api_key row");
}

/// /api/ping returns ok status + timestamp.
#[tokio::test]
async fn ping_returns_ok_status() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/ping").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["status"], "ok");
    assert!(
        res.body.get("timestamp").is_some(),
        "timestamp must be present"
    );
}

/// GET /api/health returns platform summary + keys array.
#[tokio::test]
async fn health_returns_platform_summary_and_keys_array() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/health").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("platforms").is_some());
    assert!(res.body.get("keys").is_some());
    assert!(res.body["platforms"].is_array());
    assert!(res.body["keys"].is_array());
}

/// Platforms summary entries have the full status-count shape.
#[tokio::test]
async fn platform_summary_shape() {
    let app = common::setup().await;
    insert_key("google", "shape", "healthy", 1).await;

    let res = common::get(&app.app, "/api/health").await;
    common::expect_status(&res, StatusCode::OK);
    let platforms = res.body["platforms"].as_array().expect("platforms array");
    assert!(!platforms.is_empty());
    let p = &platforms[0];
    for field in [
        "platform",
        "hasProvider",
        "totalKeys",
        "healthyKeys",
        "rateLimitedKeys",
        "invalidKeys",
        "errorKeys",
        "unknownKeys",
        "enabledKeys",
    ] {
        assert!(
            p.get(field).is_some(),
            "missing {field} on platform summary"
        );
    }
    assert!(p["hasProvider"].is_boolean());
    assert!(p["totalKeys"].is_number());
    assert!(p["healthyKeys"].is_number());
}

/// Keys are counted by status per platform.
#[tokio::test]
async fn counts_keys_by_status_correctly() {
    let app = common::setup().await;
    insert_key("google", "k1", "healthy", 1).await;
    insert_key("google", "k2", "healthy", 1).await;
    insert_key("google", "k3", "invalid", 1).await;
    insert_key("groq", "k4", "rate_limited", 1).await;
    insert_key("groq", "k5", "error", 0).await;

    let res = common::get(&app.app, "/api/health").await;
    common::expect_status(&res, StatusCode::OK);
    let platforms = res.body["platforms"].as_array().expect("platforms array");

    let google = platforms
        .iter()
        .find(|p| p["platform"] == "google")
        .expect("google platform entry");
    assert_eq!(google["totalKeys"], json!(3));
    assert_eq!(google["healthyKeys"], json!(2));
    assert_eq!(google["invalidKeys"], json!(1));
    assert_eq!(google["rateLimitedKeys"], json!(0));
    assert_eq!(google["errorKeys"], json!(0));
    assert_eq!(google["enabledKeys"], json!(3));

    let groq = platforms
        .iter()
        .find(|p| p["platform"] == "groq")
        .expect("groq platform entry");
    assert_eq!(groq["rateLimitedKeys"], json!(1));
    assert_eq!(groq["errorKeys"], json!(1));
    assert_eq!(groq["enabledKeys"], json!(1)); // 5th key has enabled = 0
}

/// The keys array exposes label/status/enabled for each stored key.
#[tokio::test]
async fn lists_all_keys_in_keys_array() {
    let app = common::setup().await;
    insert_key("google", "Health Test Key", "healthy", 1).await;

    let res = common::get(&app.app, "/api/health").await;
    common::expect_status(&res, StatusCode::OK);
    let google_key = res.body["keys"]
        .as_array()
        .expect("keys array")
        .iter()
        .find(|k| k["platform"] == "google")
        .expect("google key entry");
    assert_eq!(google_key["label"], "Health Test Key");
    assert_eq!(google_key["status"], "healthy");
    assert_eq!(google_key["enabled"], json!(true));
}

/// POST /api/health/check/:keyId — non-numeric id → 400.
#[tokio::test]
async fn check_invalid_key_id_returns_400() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/health/check/notanumber", None, None).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert_eq!(res.body["error"]["message"], "Invalid key ID");
}

/// parseInt('-1') = -1, so the route proceeds and reports a missing key as error.
#[tokio::test]
async fn check_negative_key_id_is_accepted_numeric_id() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/health/check/-1", None, None).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["keyId"], json!(-1));
    assert_eq!(res.body["status"], "error");
}

/// A non-existent key id → 200 with status 'error'.
#[tokio::test]
async fn check_nonexistent_key_returns_error_status() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/health/check/99999", None, None).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["keyId"], json!(99999));
    assert_eq!(res.body["status"], "error");
}

/// POST /api/health/check-all — no keys → 200 { success: true }.
#[tokio::test]
async fn check_all_with_no_keys_returns_success() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/health/check-all", None, None).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
}

/// POST /api/health/check-all — with a stored key → still 200 success.
#[tokio::test]
async fn check_all_with_keys_returns_success() {
    let app = common::setup().await;
    insert_key("google", "check-all", "unknown", 1).await;

    let res = common::request(&app.app, "POST", "/api/health/check-all", None, None).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
}
