//! API access on the OpenAI-compatible `/v1` routes: API keys and dashboard
//! sessions, checked end to end through the router.

mod common;

use axum::http::StatusCode;

/// No credential → 401 with the missing-header message.
#[tokio::test]
async fn v1_rejects_missing_credential() {
    let app = common::setup().await;
    let res = common::request(&app.app, "GET", "/v1/models", None, None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert_eq!(res.body["error"]["type"], "authentication_error");
    assert!(res.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Missing Authorization header"));
}

/// A key that was never issued → 401 with the invalid-key message.
#[tokio::test]
async fn v1_rejects_unknown_key() {
    let app = common::setup().await;
    let res = common::request(&app.app, "GET", "/v1/models", Some("not-a-real-key"), None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
    assert!(res.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid API key"));
}

/// An issued API key → the request reaches the handler.
#[tokio::test]
async fn v1_accepts_issued_api_key() {
    let app = common::setup().await;
    let res = common::request(&app.app, "GET", "/v1/models", Some(&app.api_key), None).await;
    common::expect_status(&res, StatusCode::OK);
}

/// The `x-api-key` header is accepted as an alternative to the bearer token.
#[tokio::test]
async fn v1_accepts_x_api_key_header() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let app = common::setup().await;
    let req = Request::builder()
        .uri("/v1/models")
        .header("x-api-key", app.api_key.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// A revoked key stops working immediately.
#[tokio::test]
async fn v1_rejects_revoked_key() {
    let app = common::setup().await;
    let list = common::get(&app.app, "/api/auth/api-key/list").await;
    common::expect_status(&list, StatusCode::OK);
    let id = list.body["api_keys"][0]["id"]
        .as_str()
        .expect("key id")
        .to_string();

    let revoke = common::post_json(
        &app.app,
        "/api/auth/api-key/revoke",
        "",
        serde_json::json!({ "id": id }),
    )
    .await;
    common::expect_status(&revoke, StatusCode::OK);
    assert_eq!(revoke.body["revoked"], true);

    let res = common::request(&app.app, "GET", "/v1/models", Some(&app.api_key), None).await;
    common::expect_status(&res, StatusCode::UNAUTHORIZED);
}

/// The dashboard session cookie also opens `/v1`, so the playground works.
#[tokio::test]
async fn v1_accepts_dashboard_session() {
    let app = common::setup().await;
    let res = common::request(
        &app.app,
        "GET",
        "/v1/models",
        Some(&common::owner_credential()),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
}
