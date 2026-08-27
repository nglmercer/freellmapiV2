//! Settings route integration tests.

mod common;

use axum::http::StatusCode;

/// GET /api/settings/api-key returns the unified API key.
#[tokio::test]
async fn should_return_the_unified_api_key() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/settings/api-key").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(
        res.body.get("apiKey").is_some(),
        "missing apiKey field: {}",
        res.text
    );
    let api_key = res.body["apiKey"]
        .as_str()
        .expect("apiKey must be a string");
    assert_eq!(api_key.len(), 48);
}

/// GET /api/settings/api-key matches getUnifiedApiKey().
#[tokio::test]
async fn should_match_get_unified_api_key_value() {
    let app = common::setup().await;
    let expected = server::db::get_unified_api_key().await;
    let res = common::get(&app.app, "/api/settings/api-key").await;
    assert_eq!(res.body["apiKey"].as_str().unwrap(), expected);
}

/// POST /api/settings/api-key/regenerate returns a new 48-char key.
#[tokio::test]
async fn should_regenerate_and_return_a_new_api_key() {
    let app = common::setup().await;
    let old_key = server::db::get_unified_api_key().await;

    let res = common::post(&app.app, "/api/settings/api-key/regenerate", "").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("apiKey").is_some());
    let new_key = res.body["apiKey"]
        .as_str()
        .expect("apiKey must be a string");
    assert_eq!(new_key.len(), 48);
    assert_ne!(
        new_key, old_key,
        "regenerated key must differ from the old one"
    );
}

/// The regenerated key persists in the DB and is returned by GET afterwards.
#[tokio::test]
async fn should_persist_the_regenerated_key() {
    let app = common::setup().await;
    let res = common::post(&app.app, "/api/settings/api-key/regenerate", "").await;
    let new_key = res.body["apiKey"].as_str().unwrap().to_string();

    let get_res = common::get(&app.app, "/api/settings/api-key").await;
    common::expect_status(&get_res, StatusCode::OK);
    assert_eq!(get_res.body["apiKey"].as_str().unwrap(), new_key);
}
