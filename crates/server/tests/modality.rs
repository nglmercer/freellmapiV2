//! Modality-aware behaviour: request-derived capability requirements, routing
//! refusal for unverified modalities, per-modality analytics and capability
//! discovery on `/v1/models`.

mod common;

use axum::http::StatusCode;
use serde_json::{json, Value};

#[test]
fn request_parts_derive_routing_capability_requirements() {
    use server::services::model_metadata::routing::RoutingPolicy;
    let raw = json!({
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "x" },
                { "type": "image_url", "image_url": { "url": "https://x.test/a.png" } },
                { "type": "input_audio", "input_audio": { "data": "AAAA", "format": "wav" } },
                { "type": "video_url", "video_url": { "url": "https://x.test/v.mp4" } }
            ]
        }]
    });
    let policy = RoutingPolicy::from_request(&raw, 10, 10);
    for cap in ["vision", "audio_input", "video_input"] {
        assert!(
            policy.required_capabilities.iter().any(|c| c == cap),
            "missing requirement {cap}"
        );
    }
    let text_only = RoutingPolicy::from_request(
        &json!({"messages": [{"role": "user", "content": "hi"}]}),
        1,
        1,
    );
    assert!(text_only.required_capabilities.is_empty());
}

#[tokio::test]
async fn image_request_is_refused_when_no_model_has_verified_vision() {
    let app = common::setup().await;
    {
        // A key exists, so routing reaches the capability filter rather than
        // failing for lack of credentials.
        let (encrypted, iv, tag) = server::crypto::encrypt("unused-key");
        let conn = server::db::connection::db().lock().await;
        conn.execute(
            "INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled) \
             VALUES ('groq', 'test', ?1, ?2, ?3, 'unknown', 1)",
            rusqlite::params![encrypted, iv, tag],
        )
        .unwrap();
    }
    let body = json!({
        "model": "auto",
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": "what is this?" },
                { "type": "image_url", "image_url": { "url": "https://x.test/cat.png" } }
            ]
        }]
    });
    let res = common::post_json(&app.app, "/v1/chat/completions", &app.api_key, body).await;
    // Explicit client error: no upstream call is made for an unsupported input.
    assert_eq!(res.status, StatusCode::BAD_REQUEST.as_u16(), "{}", res.text);
    assert!(
        res.text.contains("verified support for: vision"),
        "unexpected refusal: {}",
        res.text
    );
}

#[tokio::test]
async fn analytics_groups_usage_by_modality() {
    let app = common::setup().await;
    {
        let conn = server::db::connection::db().lock().await;
        for (modality, status) in [
            ("speech", "success"),
            ("speech", "error"),
            ("embeddings", "success"),
        ] {
            conn.execute(
                "INSERT INTO requests (platform, model_id, status, latency_ms, modality) \
                 VALUES ('groq', 'm', ?1, 100, ?2)",
                rusqlite::params![status, modality],
            )
            .unwrap();
        }
    }
    let owner = common::owner_credential();
    let res = common::request(
        &app.app,
        "GET",
        "/api/analytics/modalities?range=7d",
        Some(&owner),
        None,
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    let rows = res.body.as_array().expect("array");
    let speech = rows
        .iter()
        .find(|r| r["modality"] == "speech")
        .expect("speech row");
    assert_eq!(speech["requests"], 2);
    assert_eq!(speech["failures"], 1);
    assert_eq!(speech["costTracked"], false);
    assert!(speech["estimatedCostUsd"].is_null());
}

#[tokio::test]
async fn models_list_declares_capabilities_and_filters_by_capability() {
    let app = common::setup().await;
    let listed = common::get(&app.app, "/v1/models").await;
    common::expect_status(&listed, StatusCode::OK);
    let chat_entry: Value = listed.body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] != "auto")
        .cloned()
        .expect("a chat model");
    assert_eq!(chat_entry["capabilities"][0], "chat");

    let platform = chat_entry["owned_by"].as_str().unwrap().to_string();
    let model_id = chat_entry["id"].as_str().unwrap().to_string();
    {
        let conn = server::db::connection::db().lock().await;
        conn.execute(
            "INSERT INTO media_models (platform, model_id, capability, priority, enabled) \
             VALUES (?1, ?2, 'embeddings', 0, 1)",
            rusqlite::params![platform, model_id],
        )
        .unwrap();
    }
    let relisted = common::get(&app.app, "/v1/models").await;
    let entry = relisted.body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == model_id)
        .cloned()
        .unwrap();
    assert!(entry["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "embeddings"));

    let filtered = common::get(&app.app, "/v1/models?capability=embeddings").await;
    common::expect_status(&filtered, StatusCode::OK);
    assert_eq!(filtered.body["data"][0]["id"], model_id);

    let unknown = common::get(&app.app, "/v1/models?capability=teleport").await;
    common::expect_status(&unknown, StatusCode::BAD_REQUEST);
}
