//! Optional end-to-end tests that drive `POST /v1/chat/completions` against
//! live providers.
//!
//! The suite is skipped when no usable provider key is configured.
//! and the connection tests with `describe.skipIf(!hasWorkingProvider())`.
//! Those guards are replicated here: each test early-returns when the required
//! DB state / env key is absent, so the suite is a no-op in a fresh in-memory
//! test DB and only exercises the real path when API keys are configured.

mod common;

use axum::http::StatusCode;
use serde_json::json;

const COMMIT_PROMPT: &str = "Generate a concise git commit message for the following diff:

diff --git a/src/auth.ts b/src/auth.ts
index 1234567..abcdefg 100644
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -10,6 +10,15 @@ import { createHash } from 'crypto';
+function validateToken(token: string): boolean {
+  const parts = token.split('.');
+  if (parts.length !== 3) return false;
+  try {
+    const payload = JSON.parse(atob(parts[1]!));
+    return payload.exp > Date.now() / 1000;
+  } catch { return false; }
+}

The commit message should follow conventional commits format.";

const STREAM_COMMIT_PROMPT: &str = "Write a short commit message for this change:

diff --git a/src/routes/proxy.ts b/src/routes/proxy.ts
--- a/src/routes/proxy.ts
+++ b/src/routes/proxy.ts
@@ -50,3 +50,8 @@
+// Added retry logic for transient failures
+const MAX_RETRIES = 3;
+for (let i = 0; i < MAX_RETRIES; i++) {
+  try { return await fetch(url); } catch (e) { if (i === MAX_RETRIES - 1) throw e; }
+}";

/// `hasApiKeysInDb()` — at least one enabled key row.
async fn has_api_keys_in_db() -> bool {
    let conn = server::db::db().lock().await;
    conn.query_row("SELECT COUNT(*) FROM api_keys WHERE enabled = 1", [], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        > 0
}

/// `getWorkingPlatforms()` — platforms with an enabled, non-invalid key.
async fn get_working_platforms() -> std::collections::HashSet<String> {
    let conn = server::db::db().lock().await;
    let mut stmt = conn
        .prepare("SELECT platform FROM api_keys WHERE enabled = 1 AND status != 'invalid'")
        .expect("prepare working platform query");
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query working platforms");
    rows.filter_map(|r| r.ok()).collect()
}

/// `hasWorkingProvider()` — a free-tier fallback model whose platform has a
/// working key.
async fn has_working_provider() -> bool {
    let keys = get_working_platforms().await;
    let conn = server::db::db().lock().await;
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT m.platform FROM fallback_config fc \
             INNER JOIN models m ON m.id = fc.model_db_id \
             WHERE fc.enabled = 1 AND m.free_tier = 1 AND m.enabled = 1",
        )
        .expect("prepare free-tier platform query");
    let platforms: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query free-tier platforms")
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);
    platforms.iter().any(|p| keys.contains(p))
}

/// `getFreeTierModel()` — first free-tier fallback model whose platform has a
/// working key, ordered by fallback priority.
async fn get_free_tier_model() -> Option<String> {
    let keys = get_working_platforms().await;
    let conn = server::db::db().lock().await;
    let mut stmt = conn
        .prepare(
            "SELECT m.platform, m.model_id FROM fallback_config fc \
             INNER JOIN models m ON m.id = fc.model_db_id \
             WHERE fc.enabled = 1 AND m.free_tier = 1 AND m.enabled = 1 \
             ORDER BY fc.priority ASC",
        )
        .expect("prepare free-tier model query");
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .expect("query free-tier model")
        .filter_map(|r| r.ok())
        .collect();
    for (platform, model_id) in rows {
        if keys.contains(&platform) {
            return Some(model_id);
        }
    }
    None
}

/// Diagnostic: dump which platforms have keys and fallback entries. Skips when
/// no keys are configured.
#[tokio::test]
async fn diagnostic_reports_keys_and_fallback_platforms() {
    let app = common::setup().await;
    if !has_api_keys_in_db().await {
        // Nothing to assert on a fresh test database.
        return;
    }
    let conn = server::db::db().lock().await;
    let mut stmt = conn
        .prepare("SELECT platform, status, enabled FROM api_keys")
        .expect("prepare api keys query");
    let rows: Vec<(String, String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query api keys")
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);

    eprintln!("[E2E DIAG] {} api key(s) in database", rows.len());
    assert!(!rows.is_empty(), "keys.length must be > 0");

    let _ = app.api_key;
}

/// OpenAI-format model list used by AI-editor model discovery.
#[tokio::test]
async fn lists_models_in_openai_format() {
    let app = common::setup().await;
    if !has_api_keys_in_db().await {
        return;
    }
    let res = common::get(&app.app, "/v1/models").await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(res.body["object"], "list");
    let data = res.body["data"].as_array().expect("data array");
    assert!(!data.is_empty());

    let auto = data
        .iter()
        .find(|m| m["id"] == "auto")
        .expect("auto model present");
    assert_eq!(auto["object"], "model");

    for model in data {
        assert!(model.get("id").is_some());
        assert_eq!(model["object"], "model");
        assert!(model.get("owned_by").is_some());
        assert!(model["id"].is_string());
        // encodeURIComponent(model.id) must not throw — any string is
        // URI-encodable in Rust.
        assert!(model["id"].as_str().is_some());
    }
}

// ─────────────────────────────────────────────────────────────────────
// Connection tests require a configured working provider.
// ─────────────────────────────────────────────────────────────────────

/// Non-streaming chat completion (like Cursor/Continue).
#[tokio::test]
async fn non_streaming_commit_message_request() {
    let app = common::setup().await;
    if !has_working_provider().await {
        // No configured provider means there is nothing to probe.
        return;
    }
    let Some(test_model) = get_free_tier_model().await else {
        return;
    };

    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": test_model,
            "messages": [
                { "role": "system", "content": "You are a helpful assistant that generates git commit messages." },
                { "role": "user", "content": COMMIT_PROMPT },
            ],
            "max_tokens": 200,
            "temperature": 0.7,
        }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);

    assert!(res.body.get("id").is_some());
    let choices = res.body["choices"]
        .as_array()
        .expect("choices must be an array");
    assert!(!choices.is_empty());
    assert!(choices[0].get("message").is_some());
    let content = choices[0]["message"]["content"]
        .as_str()
        .expect("content must be a string");
    assert!(!content.is_empty());
}

/// Streaming chat completion (like Cursor/Continue): parse the SSE stream.
#[tokio::test]
async fn streaming_commit_message_request() {
    let app = common::setup().await;
    if !has_working_provider().await {
        return;
    }
    let Some(test_model) = get_free_tier_model().await else {
        return;
    };

    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": test_model,
            "stream": true,
            "messages": [
                { "role": "system", "content": "You generate concise git commit messages." },
                { "role": "user", "content": STREAM_COMMIT_PROMPT },
            ],
            "max_tokens": 150,
            "temperature": 0.5,
            "stream_options": { "include_usage": true },
        }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);
    assert_eq!(
        res.headers
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream"),
        "content-type must be text/event-stream"
    );

    let mut chunk_count = 0usize;
    let mut full_content = String::new();
    let mut saw_done = false;
    for line in res.text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("data: ") {
            continue;
        }
        let payload = &trimmed[6..];
        if payload == "[DONE]" {
            saw_done = true;
            break;
        }
        if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(payload) {
            chunk_count += 1;
            if let Some(delta) = chunk["choices"][0]["delta"]["content"].as_str() {
                full_content.push_str(delta);
            }
        }
    }

    assert!(saw_done, "stream must terminate with data: [DONE]");
    assert!(chunk_count > 0, "at least one SSE chunk expected");
    assert!(
        !full_content.is_empty(),
        "streamed content must be non-empty"
    );
}

/// Diffs containing special / URL / unicode characters (URI safety).
#[tokio::test]
async fn special_characters_in_diff_are_uri_safe() {
    let app = common::setup().await;
    if !has_working_provider().await {
        return;
    }
    let Some(test_model) = get_free_tier_model().await else {
        return;
    };

    let tricky_diff = "diff --git a/src/utils/parser.ts b/src/utils/parser.ts
--- a/src/utils/parser.ts
+++ b/src/utils/parser.ts
@@ -1,5 +1,10 @@
+// Fix: handle special chars like <>&\"'\\`$!#%~
+// URL-like strings in code: https://example.com/path?q=test&foo=bar#hash
+// Unicode: 日本語 中文 한국어 العربية
+const REGEX = /^[a-zA-Z0-9_.-]+$/;
+function sanitize(input: string): string {
+  return input.replace(/[<>]/g, (c) => c === '<' ? '&lt;' : '&gt;');
+}";

    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": test_model,
            "messages": [
                { "role": "system", "content": "Generate a commit message for the given diff." },
                { "role": "user", "content": tricky_diff },
            ],
            "max_tokens": 100,
        }),
    )
    .await;

    if res.status != 200 {
        // Preserve useful diagnostics before the assertion.
        eprintln!("[E2E DEBUG] URI Safety Status: {}", res.status);
        eprintln!("[E2E DEBUG] URI Safety Error: {}", res.text);
    }
    common::expect_status(&res, StatusCode::OK);

    let content = res.body["choices"][0]["message"]["content"]
        .as_str()
        .expect("content must be a string");
    assert!(
        !content.is_empty(),
        "content must be defined after a successful completion"
    );
}

/// OpenAI-compatible response shape. This test is ignored because it requires
/// a live provider (run with `--ignored` to exercise it).
#[tokio::test]
#[ignore]
async fn returns_valid_openai_compatible_response_shape() {
    let app = common::setup().await;
    if !has_working_provider().await {
        return;
    }
    let Some(test_model) = get_free_tier_model().await else {
        return;
    };

    let res = common::post_json(
        &app.app,
        "/v1/chat/completions",
        &app.api_key,
        json!({
            "model": test_model,
            "messages": [{ "role": "user", "content": "Say \"test\"" }],
            "max_tokens": 10,
        }),
    )
    .await;
    common::expect_status(&res, StatusCode::OK);

    assert!(res.body["id"]
        .as_str()
        .unwrap_or("")
        .starts_with("chatcmpl-"));
    assert_eq!(res.body["object"], "chat.completion");
    assert!(res.body["created"].is_number());
    assert!(res.body["model"].is_string());
    let choices = res.body["choices"].as_array().expect("choices array");
    assert_eq!(choices[0]["index"], json!(0));
    assert_eq!(choices[0]["message"]["role"], "assistant");
    assert!(choices[0].get("finish_reason").is_some());
    let usage = res.body["usage"].as_object().expect("usage object");
    assert!(usage.get("prompt_tokens").unwrap_or(&json!(0)).is_number());
    assert!(usage
        .get("completion_tokens")
        .unwrap_or(&json!(0))
        .is_number());
    assert!(usage.get("total_tokens").unwrap_or(&json!(0)).is_number());
}
