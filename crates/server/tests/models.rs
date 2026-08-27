//! Model route integration tests.

mod common;

use axum::http::StatusCode;
use serde_json::json;

/// GET /api/models returns a non-empty array.
#[tokio::test]
async fn should_return_an_array_of_models() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(
        !res.body
            .as_array()
            .expect("body must be an array")
            .is_empty(),
        "expected at least one model"
    );
}

/// Each model entry includes the required dashboard fields.
#[tokio::test]
async fn should_include_required_fields_on_each_model() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    let first = &data[0];
    for field in [
        "id",
        "platform",
        "modelId",
        "displayName",
        "intelligenceRank",
        "speedRank",
        "rpmLimit",
        "rpdLimit",
        "tpmLimit",
        "tpdLimit",
        "enabled",
        "hasProvider",
        "keyCount",
    ] {
        assert!(first.get(field).is_some(), "missing {field} on model");
    }
    assert!(first["enabled"].is_boolean());
}

/// id is a JSON number.
#[tokio::test]
async fn should_have_correct_id_type() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    assert!(data[0]["id"].is_number());
}

/// platform is a non-empty string.
#[tokio::test]
async fn should_have_correct_platform_type() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    let platform = data[0]["platform"]
        .as_str()
        .expect("platform must be string");
    assert!(!platform.is_empty());
}

/// keyCount is a number.
#[tokio::test]
async fn should_have_key_count_as_number() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    assert!(data[0]["keyCount"].is_number());
}

/// hasProvider is a boolean.
#[tokio::test]
async fn should_have_has_provider_as_boolean() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    assert!(data[0]["hasProvider"].is_boolean());
}

/// Models keep the seeded fallback ordering (and the list is non-empty).
#[tokio::test]
async fn should_return_models_ordered_by_fallback_priority() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    assert!(!data.is_empty());
}

/// Every model exposes a numeric keyCount, even with no keys inserted.
#[tokio::test]
async fn should_have_key_count_starting_at_zero_with_no_keys() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    for m in data {
        assert!(m["keyCount"].is_number(), "keyCount must be numeric: {m}");
    }
    assert!(data.iter().all(|m| m["keyCount"] == json!(0)));
}

/// fallback_config fields (priority / fallbackEnabled) are exposed.
#[tokio::test]
async fn should_include_model_fields_from_fallback_config() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("models array");
    let first = &data[0];
    assert!(first.get("priority").is_some());
    assert!(first.get("fallbackEnabled").is_some());
    assert!(first["fallbackEnabled"].is_boolean());
}

/// Legacy storage defaults (99/10) never leak through the JSON API as ranks.
#[tokio::test]
async fn unknown_ranking_values_are_nullable() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/models").await;
    common::expect_status(&res, StatusCode::OK);
    let model = res
        .body
        .as_array()
        .expect("models array")
        .iter()
        .find(|model| model["modelId"] == json!("not-yet-ranked-model"))
        .expect("unranked fixture model");
    assert!(model["intelligenceRank"].is_null());
    assert!(model["speedRank"].is_null());
    assert_eq!(model["quality"]["rank"], json!(null));
    assert_eq!(model["speed"]["rank"], json!(null));
    assert_eq!(model["ranked"], json!(false));
}

// ─────────────────────────────────────────────────────────────────────
// Bulk actions
// ─────────────────────────────────────────────────────────────────────

/// POST /api/models/disable-all without `{ confirm: true }` → 400.
#[tokio::test]
async fn disable_all_rejects_without_confirm() {
    let app = common::setup().await;
    let res = common::post_json(&app.app, "/api/models/disable-all", &app.api_key, json!({})).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

/// POST /api/models/disable-all flips every model to enabled=0.
#[tokio::test]
async fn disable_all_flips_every_model_to_disabled() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/api/models/disable-all",
        &app.api_key,
        json!({ "confirm": true }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
    assert!(res.body["modelsUpdated"].as_i64().unwrap_or(0) > 0);

    let list = common::get(&app.app, "/api/models").await;
    let models = list.body.as_array().expect("models array");
    for m in models {
        assert_eq!(m["enabled"], json!(false));
        assert_eq!(m["fallbackEnabled"], json!(false));
    }
}

/// POST /api/models/enable-all flips every model to enabled=1.
#[tokio::test]
async fn enable_all_flips_every_model_to_enabled() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/api/models/enable-all",
        &app.api_key,
        json!({ "confirm": true }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
    assert!(res.body["modelsUpdated"].as_i64().unwrap_or(0) > 0);

    let list = common::get(&app.app, "/api/models").await;
    let models = list.body.as_array().expect("models array");
    for m in models {
        assert_eq!(m["enabled"], json!(true));
        assert_eq!(m["fallbackEnabled"], json!(true));
    }
}

/// POST /api/models/enable-free without `{ confirm: true }` → 400.
#[tokio::test]
async fn enable_free_rejects_without_confirm() {
    let app = common::setup().await;
    let res = common::post_json(&app.app, "/api/models/enable-free", &app.api_key, json!({})).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
}

/// POST /api/models/enable-free only leaves free-tier rows enabled. The
/// seeded fixture has no free-tier models, so every row is disabled.
#[tokio::test]
async fn enable_free_only_enables_free_tier_rows() {
    let app = common::setup().await;
    let res = common::post_json(
        &app.app,
        "/api/models/enable-free",
        &app.api_key,
        json!({ "confirm": true }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));

    let list = common::get(&app.app, "/api/models").await;
    let models = list.body.as_array().expect("models array");
    for m in models {
        let is_free = m.get("freeTier").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_free {
            assert_eq!(m["enabled"], json!(true));
            assert_eq!(m["fallbackEnabled"], json!(true));
        } else {
            assert_eq!(m["enabled"], json!(false));
            assert_eq!(m["fallbackEnabled"], json!(false));
        }
    }
}
