//! Shared integration-test harness for an in-memory database, migrations,
//! fixtures, and an application router.

#![allow(dead_code)] // helpers are consumed by different test binaries

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

/// The DB, crypto key cache, and rate-limiter state are process-wide globals;
/// tests serialize access explicitly with this lock.
fn test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    &LOCK
}

pub struct TestApp {
    pub app: Router,
    /// `Authorization: Bearer` key for the OpenAI-compatible `/v1` routes.
    pub api_key: String,
    _guard: tokio::sync::MutexGuard<'static, ()>,
}

pub const OWNER_USERNAME: &str = "owner";
pub const OWNER_PASSWORD: &str = "correct horse battery staple";

/// Credentials created by [`setup`]: the owner's session cookie (`name=value`)
/// and an API key. Process-global because the helpers below read them without
/// a `TestApp` in hand; the test lock keeps them consistent.
static CREDENTIALS: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

fn credentials() -> (String, String) {
    CREDENTIALS
        .lock()
        .unwrap()
        .clone()
        .expect("setup() has not run")
}

/// Builds a fresh in-memory DB with migrations, the auth runtime, an owner
/// account, fixtures and the router. No configuration values are needed: the
/// encryption key and auth secret are generated into the in-memory database.
///
/// Holds the global test lock for the lifetime of the returned value.
pub async fn setup() -> TestApp {
    let guard = test_lock().lock().await;

    std::env::remove_var("ENCRYPTION_KEY");
    std::env::remove_var("BETTER_AUTH_SECRET");
    std::env::remove_var("DASHBOARD_ORIGINS");
    std::env::set_var("PORT", "0");
    let conn_dir = std::env::temp_dir().join(format!("freellmapi-test-{}", std::process::id()));
    std::fs::create_dir_all(&conn_dir).ok();
    server::env::set_project_root(conn_dir.clone());

    let db_path = format!("{}/test.db", conn_dir.display());
    server::db::set_db_path_override(&db_path);
    server::db::init_db(Some(":memory:")).expect("init_db");
    {
        let conn = server::db::db().lock().await;
        server::db::run_migrations(&conn);
        seed_test_models(&conn);
    }
    server::services::integrations::load_from_db().await;
    server::auth::start_in_memory()
        .await
        .expect("start authentication");

    let app = server::app::create_app();
    let cookie = sign_up_owner(&app).await;
    let api_key = create_api_key(&app, &cookie).await;
    *CREDENTIALS.lock().unwrap() = Some((cookie, api_key.clone()));
    TestApp {
        app,
        api_key,
        _guard: guard,
    }
}

/// Creates the owner account and returns its session cookie.
async fn sign_up_owner(app: &Router) -> String {
    let resp = request(
        app,
        "POST",
        "/api/account/create",
        None,
        Some(serde_json::json!({
            "username": OWNER_USERNAME,
            "password": OWNER_PASSWORD,
        })),
    )
    .await;
    expect_status(&resp, StatusCode::OK);
    session_cookie(&resp).expect("sign-up sets a session cookie")
}

/// Creates an API key for the signed-in owner.
async fn create_api_key(app: &Router, cookie: &str) -> String {
    let resp = request(
        app,
        "POST",
        "/api/auth/api-key/create",
        Some(&format!("cookie:{cookie}")),
        Some(serde_json::json!({ "name": "test" })),
    )
    .await;
    expect_status(&resp, StatusCode::OK);
    resp.body["key"].as_str().expect("api key").to_string()
}

/// Extracts `name=value` from the first `Set-Cookie` header of a response.
fn session_cookie(resp: &TestResponse) -> Option<String> {
    resp.headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.contains("session_token="))
        .and_then(|value| value.split(';').next())
        .map(str::to_owned)
}

// ---- deterministic model fixtures ----

struct TestModel {
    platform: &'static str,
    model_id: &'static str,
    display_name: &'static str,
    intelligence_score: Option<f64>,
    speed_tps: Option<f64>,
    size_label: &'static str,
    context_window: Option<i64>,
    ranking_source: Option<&'static str>,
    last_ranked_at: Option<&'static str>,
}

const FIXTURE: &[TestModel] = &[
    TestModel {
        platform: "google",
        model_id: "gemini-2.5-pro",
        display_name: "Gemini 2.5 Pro",
        intelligence_score: Some(65.4),
        speed_tps: Some(85.2),
        size_label: "Frontier",
        context_window: Some(1048576),
        ranking_source: Some("artificial-analysis"),
        last_ranked_at: Some("2025-01-01T00:00:00Z"),
    },
    TestModel {
        platform: "cerebras",
        model_id: "qwen-3-coder-480b",
        display_name: "Qwen3-Coder 480B",
        intelligence_score: Some(62.0),
        speed_tps: Some(1800.0),
        size_label: "Frontier",
        context_window: Some(131072),
        ranking_source: Some("artificial-analysis"),
        last_ranked_at: Some("2025-01-01T00:00:00Z"),
    },
    TestModel {
        platform: "openrouter",
        model_id: "meta-llama/llama-3.3-70b-instruct:free",
        display_name: "Llama 3.3 70B (free)",
        intelligence_score: Some(38.5),
        speed_tps: Some(60.0),
        size_label: "Large",
        context_window: Some(131072),
        ranking_source: Some("artificial-analysis"),
        last_ranked_at: Some("2025-01-01T00:00:00Z"),
    },
    TestModel {
        platform: "mistral",
        model_id: "mistral-large-latest",
        display_name: "Mistral Large 3",
        intelligence_score: Some(28.0),
        speed_tps: Some(45.0),
        size_label: "Large",
        context_window: Some(131072),
        ranking_source: Some("artificial-analysis"),
        last_ranked_at: Some("2025-01-01T00:00:00Z"),
    },
    TestModel {
        platform: "unknown-platform",
        model_id: "not-yet-ranked-model",
        display_name: "Not Yet Ranked",
        intelligence_score: None,
        speed_tps: None,
        size_label: "",
        context_window: Some(8192),
        ranking_source: None,
        last_ranked_at: None,
    },
];

pub fn seed_test_models(conn: &rusqlite::Connection) {
    let scores: Vec<f64> = FIXTURE
        .iter()
        .filter_map(|f| f.intelligence_score)
        .filter(|s| *s > 0.0)
        .collect();
    let tps: Vec<f64> = FIXTURE
        .iter()
        .filter_map(|f| f.speed_tps)
        .filter(|s| *s > 0.0)
        .collect();

    for m in FIXTURE {
        let (int_rank, spd_rank) = match (m.intelligence_score, m.speed_tps) {
            (Some(score), Some(tps_v)) => {
                let mut sorted = scores.clone();
                sorted.sort_by(|a, b| b.total_cmp(a));
                let mut sorted_t = tps.clone();
                sorted_t.sort_by(|a, b| b.total_cmp(a));
                let ir = sorted
                    .iter()
                    .position(|s| *s == score)
                    .map(|p| p as i64 + 1);
                let sr = sorted_t
                    .iter()
                    .position(|s| *s == tps_v)
                    .map(|p| p as i64 + 1);
                (ir.unwrap_or(99), sr.unwrap_or(10))
            }
            _ => (99, 10),
        };

        conn.execute(
            "INSERT INTO models (platform, model_id, display_name, intelligence_rank, \
             speed_rank, size_label, context_window, free_tier, enabled, source, \
             intelligence_score, speed_tokens_per_sec, ranking_source, last_ranked_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,0,1,'manual',?8,?9,?10,?11)",
            rusqlite::params![
                m.platform,
                m.model_id,
                m.display_name,
                int_rank,
                spd_rank,
                m.size_label,
                m.context_window,
                m.intelligence_score,
                m.speed_tps,
                m.ranking_source,
                m.last_ranked_at,
            ],
        )
        .expect("insert fixture model");
    }
    server::db::ensure_fallback_entries(conn);
}

// ---- HTTP request helpers (app.request(...) equivalent) ----

pub struct TestResponse {
    pub status: u16,
    pub body: Value,
    pub headers: axum::http::HeaderMap,
    pub text: String,
}

pub async fn request(
    app: &Router,
    method: &str,
    path: &str,
    auth: Option<&str>,
    body: Option<Value>,
) -> TestResponse {
    let auth_header = auth.map(|value| match value.strip_prefix("cookie:") {
        Some(cookie) => ("cookie", cookie.to_string()),
        None => ("authorization", format!("Bearer {value}")),
    });
    request_with_header(app, method, path, auth_header, body).await
}

async fn request_with_header(
    app: &Router,
    method: &str,
    path: &str,
    auth_header: Option<(&str, String)>,
    body: Option<Value>,
) -> TestResponse {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some((name, value)) = auth_header {
        builder = builder.header(name, value);
    }
    let req = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            builder.body(Body::from(v.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    TestResponse {
        status,
        body,
        headers,
        text,
    }
}

pub async fn get(app: &Router, path: &str) -> TestResponse {
    let auth = if path.starts_with("/api/") && path != "/api/ping" {
        Some(owner_credential())
    } else if path.starts_with("/v1/") {
        Some(api_key_credential())
    } else {
        None
    };
    request(app, "GET", path, auth.as_deref(), None).await
}

pub async fn post_json(app: &Router, path: &str, key: &str, body: Value) -> TestResponse {
    let admin = path.starts_with("/api/") && path != "/api/ping";
    let auth = admin
        .then(owner_credential)
        .unwrap_or_else(|| key.to_string());
    request(app, "POST", path, Some(&auth), Some(body)).await
}

pub async fn post(app: &Router, path: &str, key: &str) -> TestResponse {
    let admin = path.starts_with("/api/") && path != "/api/ping";
    let auth = admin
        .then(owner_credential)
        .unwrap_or_else(|| key.to_string());
    request(app, "POST", path, Some(&auth), None).await
}

/// Credential for dashboard routes: the owner's session cookie. Passed to
/// [`request`] with a `cookie:` prefix so it is sent as a `Cookie` header.
pub fn owner_credential() -> String {
    format!("cookie:{}", credentials().0)
}

/// Credential for `/v1` routes: the owner's API key, sent as a bearer token.
pub fn api_key_credential() -> String {
    credentials().1
}

pub fn expect_status(resp: &TestResponse, code: StatusCode) {
    assert_eq!(
        resp.status,
        code.as_u16(),
        "unexpected status; body: {}",
        resp.text
    );
}
