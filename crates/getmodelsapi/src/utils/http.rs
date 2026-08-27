//! HTTP helpers for provider discovery.
//!
//! One shared `reqwest::Client` (via `OnceLock`), per-request timeouts,
//! optional shared retry configuration,
//! query params and headers.

use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// Default timeout for provider discovery requests.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Error returned by HTTP helpers.
#[derive(Debug, Clone)]
pub struct HttpError {
    pub message: String,
    pub status: Option<u16>,
}

impl HttpError {
    pub fn new(message: impl Into<String>, status: Option<u16>) -> Self {
        HttpError {
            message: message.into(),
            status,
        }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for HttpError {}

/// Per-request HTTP options.
#[derive(Debug, Clone, Default)]
pub struct HttpConfig {
    pub headers: Vec<(String, String)>,
    pub params: Vec<(String, String)>,
    /// Zero means "use the default 30s timeout".
    pub timeout: Duration,
}

/// Response metadata returned by the discovery HTTP helpers.
pub struct HttpResponse {
    pub status: u16,
    pub data: Value,
}

// Process-wide retry configuration used by discovery requests.
static RETRIES: AtomicU32 = AtomicU32::new(0);
/// When true, HTTP 429 responses are not retried (the HuggingFace config sets
/// `retryCondition: (error) => error.response?.status !== 429`).
static NO_RETRY_ON_429: AtomicBool = AtomicBool::new(false);

/// Exponential retry delay: `2^retry_count * 1000` ms.
pub fn exponential_delay(retry_count: u32) -> Duration {
    Duration::from_millis((1u64 << retry_count) * 1000)
}

/// Configure retry count and whether HTTP 429 responses are retryable.
pub fn configure_retry(retries: usize, no_retry_on_429: bool) {
    RETRIES.store(retries as u32, Ordering::SeqCst);
    NO_RETRY_ON_429.store(no_retry_on_429, Ordering::SeqCst);
}

/// Shared reqwest client.
fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            // FA roundtrips for the large Google/Mistral doc pages.
            .build()
            .expect("failed to build reqwest client")
    })
}

async fn get_raw(url: &str, config: &HttpConfig) -> Result<(u16, String), HttpError> {
    let max_attempts = RETRIES.load(Ordering::SeqCst).saturating_add(1).max(1);
    let no_retry_429 = NO_RETRY_ON_429.load(Ordering::SeqCst);
    let timeout = if config.timeout.is_zero() {
        DEFAULT_TIMEOUT
    } else {
        config.timeout
    };

    let mut last_error: Option<HttpError> = None;

    for attempt in 0..max_attempts {
        let mut req = client().get(url).timeout(timeout);
        for (k, v) in &config.headers {
            req = req.header(k, v);
        }
        if !config.params.is_empty() {
            req = req.query(&config.params);
        }

        let retryable = attempt + 1 < max_attempts;

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    let status_text = resp
                        .status()
                        .canonical_reason()
                        .unwrap_or("Unknown")
                        .to_string();
                    let err = HttpError::new(format!("HTTP {status}: {status_text}"), Some(status));
                    let should_retry = retryable && !(no_retry_429 && status == 429);
                    if should_retry {
                        last_error = Some(err);
                        tokio::time::sleep(exponential_delay(attempt)).await;
                        continue;
                    }
                    return Err(err);
                }
                let text = resp
                    .text()
                    .await
                    .map_err(|e| HttpError::new(e.to_string(), Some(status)))?;
                return Ok((status, text));
            }
            Err(e) => {
                let err = HttpError::new(e.to_string(), None);
                if retryable {
                    last_error = Some(err);
                    tokio::time::sleep(exponential_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| HttpError::new("request failed", None)))
}

/// Fetch a URL and parse the body as JSON.
/// `http.get`, which sniffs the content-type).
pub async fn get_json(url: &str, config: &HttpConfig) -> Result<Value, HttpError> {
    let (status, text) = get_raw(url, config).await?;
    serde_json::from_str(&text)
        .map_err(|e| HttpError::new(format!("Invalid JSON response: {e}"), Some(status)))
}

/// Fetch a URL and return the raw body text (the non-JSON branch of `http.get`).
pub async fn get_text(url: &str, config: &HttpConfig) -> Result<String, HttpError> {
    get_raw(url, config).await.map(|(_, text)| text)
}
