//! Authentication for the dashboard and the OpenAI-compatible proxy, built on
//! Better Auth for Rust (https://github.com/nglmercer/better-auth-rs).
//!
//! - Dashboard: email/password sign-in with a cookie session, served under
//!   `/api/auth`. The first sign-up creates the single owner account; later
//!   sign-ups are refused by [`SingleOwnerAccount`].
//! - Proxy (`/v1`): API keys created in the dashboard through
//!   `/api/auth/api-key/create`, sent as `Authorization: Bearer <key>` or
//!   `x-api-key: <key>`. A dashboard session is also accepted so the built-in
//!   playground works without a key.
//!
//! No `.env` file is required. The signing secret comes from
//! `BETTER_AUTH_SECRET` when set, otherwise it is generated once and kept in
//! the SQLite `settings` table, like the provider-key encryption key.

pub mod sqlite;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use better_auth::{
    api_key::ApiKeyService,
    core::{
        adapter::{DbAdapter, Query},
        error::AuthError,
        options::{AuthOptions, PasswordHashOptions},
        plugin::Plugin,
    },
    router::axum_adapter,
    ApiKeyPlugin, AuthContext, AuthHooks, AuthPrincipal, AuthRouter, RequestMetadata,
    SessionService, User,
};
use serde_json::json;
use sqlite::SqliteDbAdapter;

/// Optional override for the signing secret. Must be at least 32 bytes.
pub const SECRET_ENV: &str = "BETTER_AUTH_SECRET";
const SECRET_SETTING: &str = "auth_secret";
const MIN_SECRET_LEN: usize = 32;

/// Dev-server origins that may send credentialed requests through the Vite
/// proxy. Production same-origin requests need no entry here.
const DEV_ORIGINS: [&str; 3] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://[::1]:5173",
];

struct AuthState {
    router: Arc<AuthRouter>,
    sessions: SessionService,
    api_keys: ApiKeyService,
    adapter: Arc<dyn DbAdapter>,
}

static STATE: Mutex<Option<Arc<AuthState>>> = Mutex::new(None);

fn state() -> Arc<AuthState> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .expect("Better Auth runtime is not initialized")
}

/// Lets only one account exist. The dashboard has a single owner, so sign-up
/// is open until that account is created and closed afterwards.
struct SingleOwnerAccount {
    adapter: Arc<dyn DbAdapter>,
}

#[async_trait]
impl AuthHooks for SingleOwnerAccount {
    async fn before_user_create(&self, _user: &mut User) -> better_auth::core::Result<()> {
        if has_account(self.adapter.as_ref()).await? {
            return Err(AuthError::Forbidden(
                "an owner account already exists; sign in instead".into(),
            ));
        }
        Ok(())
    }
}

async fn has_account(adapter: &dyn DbAdapter) -> better_auth::core::Result<bool> {
    Ok(adapter.find_one("user", Query::new()).await?.is_some())
}

/// Installs the auth runtime on `adapter`, migrating its schema first.
/// Replaces any runtime installed earlier, which lets tests start fresh.
pub async fn init(
    adapter: SqliteDbAdapter,
    secret: String,
    password_hash: PasswordHashOptions,
) -> Result<(), String> {
    let adapter = Arc::new(adapter);
    let db: Arc<dyn DbAdapter> = adapter.clone();
    let options = AuthOptions {
        secret,
        has_database: true,
        trusted_origins: trusted_origins(),
        password_hash,
        ..AuthOptions::default()
    };
    let context = AuthContext::builder(options)
        .database(db.clone())
        .plugins(vec![Arc::new(ApiKeyPlugin) as Arc<dyn Plugin>])
        .hooks(vec![Arc::new(SingleOwnerAccount {
            adapter: db.clone(),
        }) as Arc<dyn AuthHooks>])
        .build()
        .map_err(|error| error.to_string())?;
    adapter
        .apply_migrations(&context.migration_plan())
        .await
        .map_err(|error| error.to_string())?;
    let router = AuthRouter::new(context.clone()).map_err(|error| error.to_string())?;
    let sessions = SessionService::new(context.clone()).map_err(|error| error.to_string())?;
    let api_keys = ApiKeyService::new(context).map_err(|error| error.to_string())?;

    let state = AuthState {
        router: Arc::new(router),
        sessions,
        api_keys,
        adapter: db,
    };
    *STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(state));
    Ok(())
}

/// Starts the runtime on the application database file at `path`. The
/// connection is separate from the main one; SQLite WAL mode lets both use the
/// same file.
pub async fn start_with_file(path: &str) -> Result<(), String> {
    let adapter = SqliteDbAdapter::open(path).map_err(|error| error.to_string())?;
    init(
        adapter,
        resolve_secret().await,
        PasswordHashOptions::default(),
    )
    .await
}

/// Starts the runtime on a private in-memory database. Tests only: the scrypt
/// cost is lowered so each test's owner account is created in milliseconds.
pub async fn start_in_memory() -> Result<(), String> {
    let adapter = SqliteDbAdapter::in_memory().map_err(|error| error.to_string())?;
    let password_hash = PasswordHashOptions {
        scrypt_log_n: 10,
        ..PasswordHashOptions::default()
    };
    init(adapter, resolve_secret().await, password_hash).await
}

/// Resolves the signing secret: `BETTER_AUTH_SECRET` if set, otherwise the
/// value stored in the database (generated on first use).
pub async fn resolve_secret() -> String {
    if let Some(value) = crate::env::env_string(SECRET_ENV) {
        assert!(
            value.len() >= MIN_SECRET_LEN,
            "{SECRET_ENV} must be at least {MIN_SECRET_LEN} characters"
        );
        return value;
    }

    let conn = crate::db::connection::db().lock().await;
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![SECRET_SETTING],
            |row| row.get(0),
        )
        .ok();
    if let Some(value) = stored.filter(|value| value.len() >= MIN_SECRET_LEN) {
        return value;
    }

    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let secret = hex::encode(bytes);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![SECRET_SETTING, secret],
    )
    .expect("failed to persist the auth secret");
    secret
}

fn trusted_origins() -> Vec<String> {
    let mut origins: Vec<String> = DEV_ORIGINS
        .iter()
        .map(|origin| origin.to_string())
        .collect();
    if let Some(extra) = crate::env::env_string("DASHBOARD_ORIGINS") {
        origins.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty() && *origin != "*")
                .map(str::to_owned),
        );
    }
    origins
}

/// Serves every `/api/auth/*` request through the Better Auth router.
pub async fn handler(mut request: Request<Body>) -> Response {
    ensure_host(&mut request);
    let router = state().router.clone();
    axum_adapter::handler(State(router), request)
        .await
        .into_response()
}

/// The router builds absolute request URLs from `Host`. HTTP/1.1 clients always
/// send it, and HTTP/2 puts the authority in the URI, so this only fills the gap
/// for in-process callers that send neither.
fn ensure_host(request: &mut Request<Body>) {
    if !request.headers().contains_key(header::HOST) && request.uri().authority().is_none() {
        request
            .headers_mut()
            .insert(header::HOST, HeaderValue::from_static("localhost"));
    }
}

/// Whether the owner account already exists.
pub async fn account_exists() -> bool {
    has_account(state().adapter.as_ref()).await.unwrap_or(false)
}

/// Resolves the dashboard session cookie, if it is valid.
pub async fn session_principal(
    method: Method,
    uri: Uri,
    mut headers: HeaderMap,
) -> Option<AuthPrincipal> {
    // Without a cookie there is no session to resolve, and the library needs
    // a host to build the request URL. HTTP/2 clients may omit `Host`, so a
    // placeholder keeps resolution working; cookies are only ever matched by name.
    if !headers.contains_key(header::COOKIE) {
        return None;
    }
    if !headers.contains_key(header::HOST) && uri.authority().is_none() {
        headers.insert(header::HOST, HeaderValue::from_static("localhost"));
    }
    let request = RequestMetadata::new(method, uri, headers);
    state()
        .sessions
        .resolve_request(&request)
        .await
        .ok()
        .flatten()
}

/// Whether the request carries a valid dashboard session cookie.
pub async fn is_signed_in(headers: &HeaderMap) -> bool {
    session_principal(Method::GET, Uri::from_static("/"), headers.clone())
        .await
        .is_some()
}

/// Requires a signed-in dashboard session for the admin API.
pub async fn require_session(request: Request, next: Next) -> Response {
    let principal = session_principal(
        request.method().clone(),
        request.uri().clone(),
        request.headers().clone(),
    )
    .await;
    if principal.is_none() {
        return unauthorized(
            "Sign in to the dashboard to continue.",
            "authentication_error",
        );
    }
    next.run(request).await
}

/// Requires an API key or a dashboard session for the OpenAI-compatible API.
pub async fn require_api_access(request: Request, next: Next) -> Response {
    let (method, uri, headers) = (
        request.method().clone(),
        request.uri().clone(),
        request.headers().clone(),
    );
    if let Err(rejection) = authorize_api_request(method, uri, headers).await {
        return rejection.into_response();
    }
    next.run(request).await
}

/// Why an `/v1` request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    MissingCredential,
    InvalidCredential,
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        let message = match self {
            Self::MissingCredential => {
                "Missing Authorization header. Include \"Authorization: Bearer <your-api-key>\"."
            }
            Self::InvalidCredential => {
                "Invalid API key. Create one in the dashboard Settings page."
            }
        };
        tracing::info!("[Auth] Rejected API request: {message}");
        unauthorized(message, "authentication_error")
    }
}

/// Returns `Ok(())` when the request carries a valid API key or session.
/// Takes owned parts: holding a `&Request` across an await is not `Send`.
pub async fn authorize_api_request(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> Result<(), Rejection> {
    if let Some(key) = bearer_token(&headers).or_else(|| header_value(&headers, "x-api-key")) {
        if state().api_keys.authenticate(&key, None).await.is_ok() {
            return Ok(());
        }
    }
    if session_principal(method, uri, headers.clone())
        .await
        .is_some()
    {
        return Ok(());
    }
    let presented =
        bearer_token(&headers).is_some() || header_value(&headers, "x-api-key").is_some();
    Err(if presented {
        Rejection::InvalidCredential
    } else {
        Rejection::MissingCredential
    })
}

/// Shared secret for the desktop launcher's shutdown call, taken from
/// `FREELLMAPI_SHUTDOWN_TOKEN`. The launcher generates it per launch and passes
/// it to the child process, so nothing is written to disk.
pub fn shutdown_token_matches(headers: &HeaderMap) -> bool {
    let Some(expected) = crate::env::env_string("FREELLMAPI_SHUTDOWN_TOKEN") else {
        return false;
    };
    bearer_token(headers).is_some_and(|token| constant_time_eq(&token, &expected))
}

/// Pulls the credential out of `Authorization: Bearer <token>`.
pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = header_value(headers, "authorization")?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Compares without an early exit on the first differing byte.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn unauthorized(message: &str, kind: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": { "message": message, "type": kind } })),
    )
        .into_response()
}
