//! Fallback route integration tests.

mod common;

use axum::http::StatusCode;
use serde_json::json;

/// GET /api/fallback returns a non-empty fallback chain array.
#[tokio::test]
async fn get_fallback_returns_chain() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("fallback array");
    assert!(!data.is_empty(), "fallback chain must not be empty");
}

/// Each fallback entry includes the required fields.
#[tokio::test]
async fn fallback_entries_include_required_fields() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("fallback array");
    let entry = &data[0];
    for field in [
        "modelDbId",
        "priority",
        "enabled",
        "effectivePriority",
        "platform",
        "modelId",
        "displayName",
        "keyCount",
        "intelligenceRank",
        "speedRank",
        "penalty",
        "rateLimitHits",
    ] {
        assert!(
            entry.get(field).is_some(),
            "missing {field} on fallback entry"
        );
    }
    assert!(entry["modelDbId"].is_number());
    assert!(entry["priority"].is_number());
    assert!(entry["enabled"].is_boolean());
}

/// A fresh DB starts with zero penalties / rate-limit hits.
#[tokio::test]
async fn fallback_starts_with_zero_penalties() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("fallback array");
    for entry in data {
        assert_eq!(entry["penalty"], json!(0));
        assert_eq!(entry["rateLimitHits"], json!(0));
    }
}

/// effectivePriority equals priority while no dynamic penalties are active.
#[tokio::test]
async fn effective_priority_equals_priority_initially() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("fallback array");
    for entry in data {
        assert_eq!(entry["effectivePriority"], entry["priority"]);
    }
}

// ─────────────────────────────────────────────────────────────────────
// PUT /api/fallback
// ─────────────────────────────────────────────────────────────────────

/// A well-formed chain body replaces priorities successfully.
#[tokio::test]
async fn should_update_fallback_chain_successfully() {
    let app = common::setup().await;
    let body = json!([
        { "modelDbId": 1, "priority": 2, "enabled": true },
        { "modelDbId": 2, "priority": 1, "enabled": true },
    ]);
    let res = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&app.api_key),
        Some(body),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
}

/// Updated priorities are visible on a subsequent GET.
#[tokio::test]
async fn should_persist_updated_priorities_in_subsequent_get() {
    let app = common::setup().await;
    let body = json!([
        { "modelDbId": 1, "priority": 10, "enabled": true },
        { "modelDbId": 2, "priority": 5, "enabled": true },
    ]);
    let put = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&app.api_key),
        Some(body),
    )
    .await;
    common::expect_status(&put, StatusCode::OK);

    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("fallback array");
    let model1 = data
        .iter()
        .find(|e| e["modelDbId"] == json!(1))
        .expect("modelDbId 1 entry");
    assert_eq!(model1["priority"], json!(10));
}

/// Non-array body → 400 with an error field.
#[tokio::test]
async fn should_return_400_for_invalid_body() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&app.api_key),
        Some(json!("not an array")),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(
        res.body.get("error").is_some(),
        "400 body must include error: {}",
        res.text
    );
}

/// Non-numeric modelDbId → 400 with an error field.
#[tokio::test]
async fn should_return_400_when_model_db_id_not_a_number() {
    let app = common::setup().await;
    let body = json!([{ "modelDbId": "abc", "priority": 1, "enabled": true }]);
    let res = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&app.api_key),
        Some(body),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

/// Non-boolean enabled → 400 with an error field.
#[tokio::test]
async fn should_return_400_when_enabled_not_a_boolean() {
    let app = common::setup().await;
    let body = json!([{ "modelDbId": 1, "priority": 1, "enabled": "yes" }]);
    let res = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&app.api_key),
        Some(body),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

/// The fallback routes are NOT protected — PUT works without an auth header.
#[tokio::test]
async fn put_works_without_auth_header() {
    let app = common::setup().await;
    let body = json!([{ "modelDbId": 1, "priority": 1, "enabled": true }]);
    let res = common::request(&app.app, "PUT", "/api/fallback", None, Some(body)).await;
    common::expect_status(&res, StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────
// POST /api/fallback/sort/:preset
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sort_by_intelligence_preset() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/intelligence",
        Some(&app.api_key),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
    assert_eq!(res.body["preset"], "intelligence");
}

#[tokio::test]
async fn sort_by_speed_preset() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/speed",
        Some(&app.api_key),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
    assert_eq!(res.body["preset"], "speed");
}

#[tokio::test]
async fn sort_by_budget_preset() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/budget",
        Some(&app.api_key),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
    assert_eq!(res.body["preset"], "budget");
}

/// Unknown presets are rejected with 400 and a message mentioning the preset.
#[tokio::test]
async fn sort_unknown_preset_returns_400() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/doesnotexist",
        Some(&app.api_key),
        Some(json!({})),
    )
    .await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
    assert!(
        res.body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown preset"),
        "message must mention Unknown preset: {}",
        res.text
    );
}

/// The sort route is not protected either.
#[tokio::test]
async fn sort_works_without_auth_header() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/intelligence",
        None,
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/fallback/token-usage
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn token_usage_returns_budget_data() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback/token-usage").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("totalBudget").is_some());
    assert!(res.body["totalBudget"].is_number());
    assert!(res.body.get("totalUsed").is_some());
    assert!(res.body["totalUsed"].is_number());
    assert!(res.body.get("models").is_some());
    assert!(res.body["models"].is_array());
}

#[tokio::test]
async fn token_usage_budget_values_are_finite_numbers() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback/token-usage").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body["totalBudget"]
        .as_f64()
        .is_some_and(|f| f.is_finite()));
    assert!(res.body["totalUsed"]
        .as_f64()
        .is_some_and(|f| f.is_finite()));
}

#[tokio::test]
async fn token_usage_model_breakdown_has_required_fields() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback/token-usage").await;
    common::expect_status(&res, StatusCode::OK);
    let models = res.body["models"].as_array().expect("models array");
    if !models.is_empty() {
        let model = &models[0];
        assert!(model.get("displayName").is_some());
        assert!(model.get("platform").is_some());
        assert!(model.get("budget").is_some());
    }
}
