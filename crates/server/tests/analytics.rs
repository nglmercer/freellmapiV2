//! Analytics route integration tests.

mod common;

use axum::http::StatusCode;
use serde_json::json;

/// Return an ISO timestamp from the requested number of hours ago.
fn hours_ago(h: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(h))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Insert a request log row for analytics assertions.
#[allow(clippy::too_many_arguments)]
async fn insert_request(
    platform: &str,
    model_id: &str,
    status: &str,
    input_tokens: i64,
    output_tokens: i64,
    latency_ms: i64,
    error: Option<&str>,
    h_ago: i64,
) {
    let created_at = hours_ago(h_ago);
    let conn = server::db::db().lock().await;
    conn.execute(
        "INSERT INTO requests \
         (platform, model_id, status, input_tokens, output_tokens, latency_ms, error, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            platform,
            model_id,
            status,
            input_tokens,
            output_tokens,
            latency_ms,
            error,
            created_at
        ],
    )
    .expect("insert request row");
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/summary
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn summary_returns_stats_fields() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/summary").await;
    common::expect_status(&res, StatusCode::OK);
    for field in [
        "totalRequests",
        "successRate",
        "totalInputTokens",
        "totalOutputTokens",
        "avgLatencyMs",
        "estimatedCostSavings",
    ] {
        assert!(res.body.get(field).is_some(), "missing {field} in summary");
    }
}

#[tokio::test]
async fn summary_defaults_to_zero_on_empty_db() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/summary").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["totalRequests"], json!(0));
    assert_eq!(res.body["successRate"], json!(0));
}

#[tokio::test]
async fn summary_accepts_range_24h() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/summary?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("totalRequests").is_some());
}

#[tokio::test]
async fn summary_accepts_range_30d() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/summary?range=30d").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("totalRequests").is_some());
}

#[tokio::test]
async fn summary_fields_are_numeric() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/summary").await;
    common::expect_status(&res, StatusCode::OK);
    for field in [
        "totalRequests",
        "successRate",
        "totalInputTokens",
        "totalOutputTokens",
        "avgLatencyMs",
        "estimatedCostSavings",
    ] {
        assert!(res.body[field].is_number(), "{field} must be numeric");
    }
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/by-model
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn by_model_returns_array() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-model").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn by_model_accepts_range_24h() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-model?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn by_model_accepts_range_30d() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-model?range=30d").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn by_model_is_empty_with_no_requests() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-model").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.as_array().expect("array").is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/by-platform
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn by_platform_returns_array() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-platform").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn by_platform_is_empty_with_no_requests() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-platform").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn by_platform_accepts_range_query_param() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/by-platform?range=7d").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/timeline
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn timeline_returns_array() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/timeline").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn timeline_accepts_interval_hour() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/timeline?range=24h&interval=hour").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn timeline_accepts_interval_day() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/timeline?range=7d&interval=day").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

/// Fresh DB has no matching requests, so the timeline is an empty array.
#[tokio::test]
async fn timeline_entries_shape() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/timeline").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/error-distribution
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn error_distribution_returns_shapes() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/error-distribution").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("byCategory").is_some());
    assert!(res.body.get("byPlatform").is_some());
    assert!(res.body.get("detailed").is_some());
    assert!(res.body["byCategory"].is_array());
    assert!(res.body["byPlatform"].is_array());
    assert!(res.body["detailed"].is_array());
}

#[tokio::test]
async fn error_distribution_accepts_range() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/error-distribution?range=30d").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("byCategory").is_some());
    assert!(res.body.get("byPlatform").is_some());
    assert!(res.body.get("detailed").is_some());
}

#[tokio::test]
async fn error_distribution_is_empty_with_no_errors() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/error-distribution").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body["byCategory"].as_array().expect("array").is_empty());
    assert!(res.body["byPlatform"].as_array().expect("array").is_empty());
    assert!(res.body["detailed"].as_array().expect("array").is_empty());
}

// ─────────────────────────────────────────────────────────────────────
// GET /api/analytics/errors
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn errors_returns_array() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/errors").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

#[tokio::test]
async fn errors_is_empty_with_no_errors() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/errors").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn errors_returns_at_most_50() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/errors").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.as_array().expect("array").len() <= 50);
}

#[tokio::test]
async fn errors_accepts_range() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/analytics/errors?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.is_array());
}

// ─────────────────────────────────────────────────────────────────────
// Analytics with request data
// ─────────────────────────────────────────────────────────────────────

async fn seed_request_fixture() {
    insert_request(
        "google",
        "gemini-2.5-flash",
        "success",
        100,
        50,
        500,
        None,
        1,
    )
    .await;
    insert_request(
        "google",
        "gemini-2.5-flash",
        "success",
        200,
        100,
        600,
        None,
        2,
    )
    .await;
    insert_request(
        "google",
        "gemini-2.5-flash",
        "error",
        50,
        0,
        300,
        Some("429 rate limit"),
        3,
    )
    .await;
    insert_request(
        "groq",
        "llama-3.3-70b-versatile",
        "success",
        150,
        80,
        200,
        None,
        5,
    )
    .await;
    insert_request(
        "groq",
        "llama-3.3-70b-versatile",
        "error",
        30,
        0,
        150,
        Some("500 internal server"),
        6,
    )
    .await;
}

#[tokio::test]
async fn summary_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/summary?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["totalRequests"], json!(5));
    assert_eq!(res.body["totalInputTokens"], json!(530));
    assert_eq!(res.body["totalOutputTokens"], json!(230));
}

#[tokio::test]
async fn by_model_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/by-model?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("by-model array");
    assert_eq!(data.len(), 2);
    let google = data
        .iter()
        .find(|d| d["platform"] == "google")
        .expect("google entry");
    assert_eq!(google["requests"], json!(3));
}

#[tokio::test]
async fn by_platform_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/by-platform?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body.as_array().expect("array").len(), 2);
}

#[tokio::test]
async fn timeline_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/timeline?range=24h&interval=hour").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("timeline array");
    assert!(!data.is_empty());
    assert!(data[0].get("timestamp").is_some());
    assert!(data[0].get("requests").is_some());
    assert!(data[0].get("successCount").is_some());
    assert!(data[0].get("failureCount").is_some());
}

#[tokio::test]
async fn error_distribution_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/error-distribution?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(
        !res.body["byCategory"].as_array().expect("array").is_empty(),
        "byCategory must be populated: {}",
        res.text
    );
    assert!(!res.body["byPlatform"].as_array().expect("array").is_empty());
    assert!(!res.body["detailed"].as_array().expect("array").is_empty());
}

#[tokio::test]
async fn errors_with_request_data() {
    let app = common::setup().await;
    seed_request_fixture().await;

    let res = common::get(&app.app, "/api/analytics/errors?range=24h").await;
    common::expect_status(&res, StatusCode::OK);
    let data = res.body.as_array().expect("errors array");
    assert_eq!(data.len(), 2);
    assert!(data[0].get("platform").is_some());
    assert!(data[0].get("error").is_some());
}
