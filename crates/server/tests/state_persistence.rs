//! Runtime-state snapshot and restore integration tests.

mod common;

use serde_json::json;
use server::services::health::{restore_health_state, snapshot_health_state, HealthSnapshot};
use server::services::ratelimit::{
    can_make_request, get_sticky_model, is_on_cooldown, record_request, restore_rate_limit_state,
    set_cooldown, set_sticky_model, Limits, RateLimitSnapshot,
};
use server::services::router::{
    get_all_penalties, record_rate_limit_hit, restore_router_state, snapshot_router_state,
    RouterSnapshot,
};
use server::services::state_persistence::{
    clear_runtime_state, restore_runtime_state, save_runtime_state,
};
use server::types::ChatMessage;

fn empty_rl() -> RateLimitSnapshot {
    RateLimitSnapshot {
        windows: vec![],
        cooldowns: vec![],
        sticky_sessions: vec![],
    }
}

fn empty_router() -> RouterSnapshot {
    RouterSnapshot {
        round_robin_index: vec![],
        rate_limit_penalties: vec![],
    }
}

// ── Rate Limit snapshot/restore ──

#[tokio::test]
async fn preserves_cooldowns_across_snapshot_restore() {
    let _app = common::setup().await;

    set_cooldown("google", "gemini-pro", 1, 60_000);
    assert!(is_on_cooldown("google", "gemini-pro", 1));

    let snap = server::services::ratelimit::snapshot_rate_limit_state();

    restore_rate_limit_state(empty_rl());
    assert!(!is_on_cooldown("google", "gemini-pro", 1));

    restore_rate_limit_state(snap);
    assert!(is_on_cooldown("google", "gemini-pro", 1));
}

#[tokio::test]
async fn preserves_rate_limit_windows() {
    let _app = common::setup().await;

    record_request("groq", "llama-3", 5);
    let limits = Limits {
        rpm: Some(1),
        rpd: None,
        tpm: None,
        tpd: None,
    };
    assert!(!can_make_request("groq", "llama-3", 5, &limits));

    let snap = server::services::ratelimit::snapshot_rate_limit_state();

    restore_rate_limit_state(empty_rl());
    assert!(can_make_request("groq", "llama-3", 5, &limits));

    restore_rate_limit_state(snap);
    assert!(!can_make_request("groq", "llama-3", 5, &limits));
}

#[tokio::test]
async fn preserves_sticky_sessions() {
    let _app = common::setup().await;

    let messages = vec![
        ChatMessage::text("user", "hello world test session"),
        ChatMessage::text("assistant", "hi there"),
    ];
    set_sticky_model(&messages, 42);
    assert_eq!(get_sticky_model(&messages), Some(42));

    let snap = server::services::ratelimit::snapshot_rate_limit_state();

    restore_rate_limit_state(empty_rl());
    assert_eq!(get_sticky_model(&messages), None);

    restore_rate_limit_state(snap);
    assert_eq!(get_sticky_model(&messages), Some(42));
}

// ── Router snapshot/restore ──

#[tokio::test]
async fn preserves_rate_limit_penalties() {
    let _app = common::setup().await;

    record_rate_limit_hit(1);
    record_rate_limit_hit(1);
    let penalties_before = get_all_penalties();
    assert!(!penalties_before.is_empty());
    let model1 = penalties_before
        .iter()
        .find(|p| p.model_db_id == 1)
        .unwrap();
    assert!(model1.penalty > 0);
    let penalty_val = model1.penalty;

    let snap = snapshot_router_state();

    restore_router_state(empty_router());
    assert!(get_all_penalties().iter().all(|p| p.model_db_id != 1));

    restore_router_state(snap);
    let restored = get_all_penalties();
    let model1_restored = restored.iter().find(|p| p.model_db_id == 1).unwrap();
    assert_eq!(model1_restored.penalty, penalty_val);
}

#[tokio::test]
async fn round_robin_snapshot_is_array() {
    let _app = common::setup().await;
    let snap = snapshot_router_state();
    // Serialized form is a JSON array of key/value pairs.
    let v = serde_json::to_value(&snap).unwrap();
    assert!(v["roundRobinIndex"].is_array());
}

// ── Health snapshot/restore ──

#[tokio::test]
async fn health_snapshot_shape() {
    let _app = common::setup().await;
    let snap = snapshot_health_state();
    let v = serde_json::to_value(&snap).unwrap();
    assert!(v["failureCount"].is_array());
    restore_health_state(HealthSnapshot {
        failure_count: vec![],
    });
}

// ── Full save/restore cycle to disk ──

async fn reset_all_state() {
    restore_rate_limit_state(empty_rl());
    restore_router_state(empty_router());
    restore_health_state(HealthSnapshot {
        failure_count: vec![],
    });
    clear_runtime_state();
}

#[tokio::test]
async fn save_state_to_disk_and_restore() {
    let _app = common::setup().await;
    reset_all_state().await;

    set_cooldown("openai", "gpt-4", 10, 60_000);
    record_rate_limit_hit(3);

    save_runtime_state().unwrap();

    restore_rate_limit_state(empty_rl());
    restore_router_state(empty_router());
    assert!(!is_on_cooldown("openai", "gpt-4", 10));

    assert!(restore_runtime_state());
    assert!(is_on_cooldown("openai", "gpt-4", 10));
    assert!(get_all_penalties().iter().any(|p| p.model_db_id == 3));

    clear_runtime_state();
}

#[tokio::test]
async fn restore_returns_false_without_state_file() {
    let _app = common::setup().await;
    reset_all_state().await;
    assert!(!restore_runtime_state());
}

#[tokio::test]
async fn discards_stale_state_older_than_60_minutes() {
    let _app = common::setup().await;
    reset_all_state().await;

    set_cooldown("test", "model", 1, 60_000);
    save_runtime_state().unwrap();

    let state_file = {
        let path = server::db::db_path();
        std::path::Path::new(&path)
            .parent()
            .unwrap()
            .join("runtime-state.json")
    };
    let raw = std::fs::read_to_string(&state_file).unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    parsed["savedAt"] = json!(chrono::Utc::now().timestamp_millis() - 61 * 60 * 1000);
    std::fs::write(&state_file, parsed.to_string()).unwrap();

    assert!(!restore_runtime_state());
    clear_runtime_state();
}
