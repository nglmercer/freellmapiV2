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

/// Unknown benchmark dimensions are represented as null, never as the old
/// 99/10 storage sentinels.
#[tokio::test]
async fn unknown_ranking_values_are_nullable() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let entry = res
        .body
        .as_array()
        .expect("fallback array")
        .iter()
        .find(|entry| entry["modelId"] == json!("not-yet-ranked-model"))
        .expect("unranked fixture model");
    assert!(entry["intelligenceRank"].is_null());
    assert!(entry["speedRank"].is_null());
    assert_eq!(entry["quality"]["rank"], json!(null));
    assert_eq!(entry["speed"]["rank"], json!(null));
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
    let owner = common::owner_credential();
    let body = json!([
        { "modelDbId": 1, "priority": 2, "enabled": true },
        { "modelDbId": 2, "priority": 1, "enabled": true },
    ]);
    let res = common::request(&app.app, "PUT", "/api/fallback", Some(&owner), Some(body)).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["success"], json!(true));
}

/// Updated priorities are visible on a subsequent GET.
#[tokio::test]
async fn should_persist_updated_priorities_in_subsequent_get() {
    let app = common::setup().await;
    let owner = common::owner_credential();
    let body = json!([
        { "modelDbId": 1, "priority": 10, "enabled": true },
        { "modelDbId": 2, "priority": 5, "enabled": true },
    ]);
    let put = common::request(&app.app, "PUT", "/api/fallback", Some(&owner), Some(body)).await;
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
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "PUT",
        "/api/fallback",
        Some(&owner),
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
    let owner = common::owner_credential();
    let body = json!([{ "modelDbId": "abc", "priority": 1, "enabled": true }]);
    let res = common::request(&app.app, "PUT", "/api/fallback", Some(&owner), Some(body)).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

/// Non-boolean enabled → 400 with an error field.
#[tokio::test]
async fn should_return_400_when_enabled_not_a_boolean() {
    let app = common::setup().await;
    let owner = common::owner_credential();
    let body = json!([{ "modelDbId": 1, "priority": 1, "enabled": "yes" }]);
    let res = common::request(&app.app, "PUT", "/api/fallback", Some(&owner), Some(body)).await;
    common::expect_status(&res, StatusCode::BAD_REQUEST);
    assert!(res.body.get("error").is_some());
}

/// The fallback routes require the admin credential.
#[tokio::test]
async fn put_works_without_auth_header() {
    let app = common::setup().await;
    let body = json!([{ "modelDbId": 1, "priority": 1, "enabled": true }]);
    let res = common::request(&app.app, "PUT", "/api/fallback", None, Some(body)).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────
// POST /api/fallback/sort/:preset
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sort_by_intelligence_preset() {
    let app = common::setup().await;
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/intelligence",
        Some(&owner),
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
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/speed",
        Some(&owner),
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
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/budget",
        Some(&owner),
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
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "POST",
        "/api/fallback/sort/doesnotexist",
        Some(&owner),
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

/// The sort route requires the admin credential.
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
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
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

/// The first chain entry, with its legacy quality and speed columns cleared so
/// the registry is the only source that can fill them.
async fn unscored_entry_id(app: &axum::Router) -> (i64, serde_json::Value) {
    let res = common::get(app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    let id = res.body[0]["modelDbId"].as_i64().unwrap();
    {
        let conn = server::db::db().lock().await;
        conn.execute(
            "UPDATE models SET intelligence_score = NULL, speed_tokens_per_sec = NULL, \
             external_speed_tps = NULL, observed_speed_tps = NULL WHERE id = ?1",
            [id],
        )
        .expect("clear legacy scores");
    }
    let res = common::get(app, "/api/fallback").await;
    let entry = res
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["modelDbId"] == json!(id))
        .expect("entry still in the chain")
        .clone();
    (id, entry)
}

/// Inserts one effective registry observation for a deployment, the same rows
/// a metadata sync writes.
async fn set_deployment_metric(
    deployment_id: i64,
    metric: &str,
    value: serde_json::Value,
    source: &str,
) {
    let conn = server::db::db().lock().await;
    let (platform, model_id): (String, String) = conn
        .query_row(
            "SELECT platform, model_id FROM models WHERE id = ?1",
            [deployment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let target = format!("d:{deployment_id}");
    let value_json = json!({ "type": "number", "value": value }).to_string();
    conn.execute(
        "INSERT INTO model_metadata_observations \
         (source, source_model_id, model_id, provider, target_key, metric, scope, value_json, unit, \
          fetched_at, benchmark_version, confidence, verified, match_status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'deployment', ?7, 'test', ?8, 'test-v1', 0.9, 1, 'matched')",
        rusqlite::params![
            source,
            model_id,
            model_id,
            platform,
            target,
            metric,
            value_json,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .expect("insert observation");
    let observation: i64 = conn
        .query_row("SELECT last_insert_rowid()", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO model_metadata_effective (target_key, metric, observation_id, selection_reason) \
         VALUES (?1, ?2, ?3, 'test')",
        rusqlite::params![target, metric, observation],
    )
    .expect("insert effective value");
}

async fn entry_for(app: &axum::Router, id: i64) -> serde_json::Value {
    let res = common::get(app, "/api/fallback").await;
    common::expect_status(&res, StatusCode::OK);
    res.body
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["modelDbId"] == json!(id))
        .expect("entry in the chain")
        .clone()
}

/// A quality score in the registry fills the chain's empty quality value.
#[tokio::test]
async fn fallback_fills_quality_from_the_registry() {
    let app = common::setup().await;
    let (id, before) = unscored_entry_id(&app.app).await;
    assert!(before["intelligenceScore"].is_null());

    set_deployment_metric(
        id,
        "benchmark.aa.intelligence",
        json!(62.5),
        "artificial-analysis",
    )
    .await;

    let after = entry_for(&app.app, id).await;
    assert_eq!(after["intelligenceScore"], json!(62.5));
    assert_eq!(after["quality"]["score"], json!(62.5));
    assert_eq!(after["quality"]["source"], json!("artificial-analysis"));
    assert_eq!(after["qualityRanked"], json!(true));
    assert_eq!(after["qualityState"], json!("available"));
}

/// A reviewed manual score is preferred over the Artificial Analysis score.
#[tokio::test]
async fn manual_quality_overrides_the_aa_score() {
    let app = common::setup().await;
    let (id, _) = unscored_entry_id(&app.app).await;
    set_deployment_metric(
        id,
        "benchmark.aa.intelligence",
        json!(40.0),
        "artificial-analysis",
    )
    .await;
    set_deployment_metric(
        id,
        "benchmark.manual.intelligence",
        json!(71.0),
        "models-dev",
    )
    .await;

    let entry = entry_for(&app.app, id).await;
    assert_eq!(entry["intelligenceScore"], json!(71.0));
    assert_eq!(entry["quality"]["source"], json!("models-dev"));
}

/// Registry output speed fills the speed value when nothing was measured locally.
#[tokio::test]
async fn fallback_fills_speed_from_the_registry() {
    let app = common::setup().await;
    let (id, before) = unscored_entry_id(&app.app).await;
    assert!(before["speedTokensPerSec"].is_null());
    assert_eq!(before["speedState"], json!("awaiting_requests"));

    set_deployment_metric(
        id,
        "performance.output_tps",
        json!(118.4),
        "artificial-analysis",
    )
    .await;

    let after = entry_for(&app.app, id).await;
    assert_eq!(after["speedTokensPerSec"], json!(118.4));
    assert_eq!(after["speed"]["tokensPerSecond"], json!(118.4));
    assert_eq!(after["speedState"], json!("available"));
}

/// Context, price and capabilities from the registry appear in `metrics`.
#[tokio::test]
async fn fallback_exposes_registry_metrics_and_capabilities() {
    let app = common::setup().await;
    let (id, _) = unscored_entry_id(&app.app).await;
    set_deployment_metric(id, "context.total", json!(128000), "models-dev").await;
    set_deployment_metric(id, "price.input", json!(0.5), "models-dev").await;
    set_deployment_metric(id, "capability.tools", json!(1), "models-dev").await;
    set_deployment_metric(id, "capability.vision", json!(0), "models-dev").await;

    let entry = entry_for(&app.app, id).await;
    assert_eq!(entry["metrics"]["contextWindow"], json!(128000.0));
    assert_eq!(entry["metrics"]["priceInput"], json!(0.5));
    assert_eq!(entry["metrics"]["capabilities"], json!(["tools"]));
}

/// Without a registry value the state says why, rather than leaving a blank.
#[tokio::test]
async fn missing_quality_reports_its_state() {
    let app = common::setup().await;
    let (_, entry) = unscored_entry_id(&app.app).await;
    assert!(entry["intelligenceScore"].is_null());
    assert!(matches!(
        entry["qualityState"].as_str(),
        Some("needs_source") | Some("no_data")
    ));
}
