//! Dashboard authentication and shutdown integration tests.

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn dashboard_api_requires_a_session() {
    let app = common::setup().await;
    let res = common::request(&app.app, "GET", "/api/settings/setup-status", None, None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_api_rejects_provider_api_key() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "GET",
        "/api/settings/setup-status",
        Some(&app.api_key),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_api_accepts_session() {
    let app = common::setup().await;
    let res = common::get(&app.app, "/api/settings/setup-status").await;
    common::expect_status(&res, StatusCode::OK);
    assert!(res.body.get("unifiedKey").is_none(), "{}", res.text);
}

#[tokio::test]
async fn account_status_is_public_and_reports_the_owner() {
    let app = common::setup().await;
    let res = common::request(&app.app, "GET", "/api/account-status", None, None).await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["hasAccount"], true);
}

#[tokio::test]
async fn second_sign_up_is_refused() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/auth/sign-up/email",
        None,
        Some(serde_json::json!({
            "email": "intruder@example.com",
            "name": "Intruder",
            "password": "another long password",
        })),
    )
    .await;
    assert!(
        res.status >= 400,
        "second account must be refused, got {}: {}",
        res.status,
        res.text
    );
}

#[tokio::test]
async fn wrong_password_is_rejected() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/auth/sign-in/email",
        None,
        Some(serde_json::json!({
            "email": "owner@example.com",
            "password": "definitely wrong password",
        })),
    )
    .await;
    assert!(res.status >= 400, "got {}: {}", res.status, res.text);
}

#[tokio::test]
async fn sign_out_ends_the_session() {
    let app = common::setup().await;
    let out = common::request(
        &app.app,
        "POST",
        "/api/auth/sign-out",
        Some(&common::owner_credential()),
        Some(serde_json::json!({})),
    )
    .await;
    common::expect_status(&out, StatusCode::OK);

    let res = common::request(
        &app.app,
        "GET",
        "/api/settings/setup-status",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn shutdown_rejects_missing_credentials() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/shutdown", None, None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn shutdown_rejects_provider_api_key() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/shutdown", Some(&app.api_key), None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn shutdown_accepts_owner_session() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/shutdown",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::ACCEPTED);
    assert_eq!(res.body["status"], "shutting_down");
}

#[tokio::test]
async fn shutdown_accepts_launcher_token() {
    let app = common::setup().await;
    std::env::set_var("FREELLMAPI_SHUTDOWN_TOKEN", "launcher-token-for-test");
    let res = common::request(
        &app.app,
        "POST",
        "/api/shutdown",
        Some("launcher-token-for-test"),
        None,
    )
    .await;
    std::env::remove_var("FREELLMAPI_SHUTDOWN_TOKEN");
    common::expect_status(&res, StatusCode::ACCEPTED);
}
