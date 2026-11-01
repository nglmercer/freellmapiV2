//! Owner account creation with a username, and fast join with a join code.

mod common;

use axum::http::StatusCode;
use serde_json::json;

async fn new_join_code(app: &common::TestApp) -> String {
    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join-code",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    res.body["code"].as_str().expect("join code").to_string()
}

fn has_session_cookie(res: &common::TestResponse) -> bool {
    res.headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.starts_with("better-auth.session_token="))
}

#[tokio::test]
async fn create_is_refused_once_the_owner_exists() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/account/create",
        None,
        Some(json!({ "username": "x", "password": "long enough password" })),
    )
    .await;
    common::expect_status(&res, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sign_in_works_with_username_and_password() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/auth/sign-in/username",
        None,
        Some(json!({
            "username": common::OWNER_USERNAME,
            "password": common::OWNER_PASSWORD,
        })),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert!(
        has_session_cookie(&res),
        "sign-in must set a session cookie"
    );
}

#[tokio::test]
async fn join_code_is_required_to_generate_and_revoke() {
    let app = common::setup().await;
    let res = common::request(&app.app, "POST", "/api/account/join-code", None, None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    let res = common::request(&app.app, "DELETE", "/api/account/join-code", None, None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn join_code_signs_a_new_browser_in_without_password() {
    let app = common::setup().await;
    let code = new_join_code(&app).await;

    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": code })),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert!(has_session_cookie(&res));
    assert_eq!(res.body["user"]["name"], common::OWNER_USERNAME);
}

#[tokio::test]
async fn join_code_ignores_case_spaces_and_dashes() {
    let app = common::setup().await;
    let code = new_join_code(&app).await;
    let typed = format!(" {} ", code.to_lowercase().replace('-', " "));

    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": typed })),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
}

#[tokio::test]
async fn wrong_join_code_is_refused() {
    let app = common::setup().await;
    let _ = new_join_code(&app).await;
    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": "AAAA-BBBB-CCCC-DDDD" })),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(!has_session_cookie(&res));
}

#[tokio::test]
async fn regenerating_the_code_invalidates_the_previous_one() {
    let app = common::setup().await;
    let old = new_join_code(&app).await;
    let new = new_join_code(&app).await;
    assert_ne!(old, new);

    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": old })),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);

    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": new })),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
}

#[tokio::test]
async fn revoking_the_code_disables_joining() {
    let app = common::setup().await;
    let code = new_join_code(&app).await;

    let status = common::get(&app.app, "/api/account/join-code").await;
    common::expect_status(&status, StatusCode::OK);
    assert_eq!(status.body["enabled"], true);

    let revoke = common::request(
        &app.app,
        "DELETE",
        "/api/account/join-code",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&revoke, StatusCode::OK);

    let res = common::request(
        &app.app,
        "POST",
        "/api/account/join",
        None,
        Some(json!({ "code": code })),
    )
    .await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn join_code_is_not_stored_in_plain_text() {
    let app = common::setup().await;
    let code = new_join_code(&app).await;
    let stored: String = {
        let conn = server::db::db().lock().await;
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'join_code_hash'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_ne!(stored, code);
    assert!(!stored.contains(&code.replace('-', "")));
    assert_eq!(stored.len(), 64, "SHA-256 hex digest");
}
