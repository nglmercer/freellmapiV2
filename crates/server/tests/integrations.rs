//! Benchmark-source credentials set from the dashboard, and bulk actions that
//! only enable providers with a configured key.

mod common;

use axum::http::StatusCode;
use serde_json::json;

const AA_KEY: &str = "aa_test_key_0123456789abcdef";

async fn put_key(app: &axum::Router, key: &str) -> common::TestResponse {
    common::request(
        app,
        "PUT",
        "/api/integrations/artificial-analysis",
        Some(&common::owner_credential()),
        Some(json!({ "key": key })),
    )
    .await
}

#[tokio::test]
async fn artificial_analysis_key_is_stored_encrypted_and_never_returned() {
    let app = common::setup().await;

    let status = common::get(&app.app, "/api/integrations/artificial-analysis").await;
    common::expect_status(&status, StatusCode::OK);
    assert_eq!(status.body["configured"], json!(false));
    assert_eq!(status.body["source"], json!("none"));

    let put = put_key(&app.app, AA_KEY).await;
    common::expect_status(&put, StatusCode::OK);
    assert_eq!(put.body["configured"], json!(true));
    assert_eq!(put.body["source"], json!("dashboard"));
    assert!(!put.text.contains(AA_KEY), "response must not echo the key");
    assert_eq!(put.body["maskedKey"], json!("aa_t••••••••cdef"));

    let stored: String = {
        let conn = server::db::db().lock().await;
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'integrations.artificial_analysis'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert!(!stored.contains(AA_KEY), "stored value must be encrypted");
}

#[tokio::test]
async fn saved_key_marks_the_benchmark_source_configured() {
    let app = common::setup().await;
    put_key(&app.app, AA_KEY).await;

    let sources = common::get(&app.app, "/api/metadata/sources").await;
    common::expect_status(&sources, StatusCode::OK);
    let aa = sources
        .body
        .as_array()
        .or_else(|| sources.body["data"].as_array())
        .expect("sources array")
        .iter()
        .find(|s| s["id"] == json!("artificial-analysis"))
        .expect("artificial-analysis source listed");
    assert_eq!(aa["configured"], json!(true));
}

#[tokio::test]
async fn deleting_the_key_clears_the_configuration() {
    let app = common::setup().await;
    put_key(&app.app, AA_KEY).await;

    let deleted = common::request(
        &app.app,
        "DELETE",
        "/api/integrations/artificial-analysis",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&deleted, StatusCode::OK);
    assert_eq!(deleted.body["configured"], json!(false));
}

#[tokio::test]
async fn empty_key_is_refused() {
    let app = common::setup().await;
    let put = put_key(&app.app, "   ").await;
    common::expect_status(&put, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn enable_all_only_enables_providers_with_a_key() {
    let app = common::setup().await;
    {
        let conn = server::db::db().lock().await;
        conn.execute("UPDATE models SET enabled = 0", []).unwrap();
        conn.execute("UPDATE fallback_config SET enabled = 0", [])
            .unwrap();
    }
    common::post_json(
        &app.app,
        "/api/keys",
        "",
        json!({ "platform": "google", "key": "AIzaTestKey123456789" }),
    )
    .await;

    let res = common::post_json(
        &app.app,
        "/api/models/enable-all",
        "",
        json!({ "confirm": true }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);

    let (unconfigured_enabled, configured_enabled): (i64, i64) = {
        let conn = server::db::db().lock().await;
        let unconfigured = conn
            .query_row(
                "SELECT COUNT(*) FROM models WHERE enabled = 1 AND platform != 'google'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let configured = conn
            .query_row(
                "SELECT COUNT(*) FROM models WHERE enabled = 1 AND platform = 'google'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        (unconfigured, configured)
    };
    assert_eq!(
        unconfigured_enabled, 0,
        "unconfigured providers must stay disabled"
    );
    assert!(
        configured_enabled > 0,
        "the configured provider must be enabled"
    );
}

#[tokio::test]
async fn fallback_entries_report_whether_their_provider_is_configured() {
    let app = common::setup().await;
    common::post_json(
        &app.app,
        "/api/keys",
        "",
        json!({ "platform": "google", "key": "AIzaTestKey123456789" }),
    )
    .await;

    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let entries = res.body.as_array().unwrap();
    for entry in entries {
        let configured = entry["platform"] == json!("google");
        assert_eq!(
            entry["configured"],
            json!(configured),
            "{} on {}",
            entry["displayName"],
            entry["platform"]
        );
    }
}
