//! Port of `server/src/providers/base.ts` plus the shared HTTP helpers the
//! TS `BaseProvider` provided.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::{ChatCompletionChunk, ChatCompletionResponse, ChatMessage, CompletionOptions};

/// Streaming chunks arrive over a channel (replaces the TS AsyncGenerator).
pub type ChunkReceiver = mpsc::Receiver<Result<ChatCompletionChunk, ProviderError>>;
pub type ChunkSender = mpsc::Sender<Result<ChatCompletionChunk, ProviderError>>;

/// Upstream/provider failure. `status` carries the HTTP status when the
/// provider returned one (used for fallback decisions: 429/5xx/timeout).
#[derive(Debug, Clone)]
pub struct ProviderError {
    pub status: Option<u16>,
    pub message: String,
}

impl ProviderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { status: None, message: message.into() }
    }
    pub fn http(status: u16, message: impl Into<String>) -> Self {
        Self { status: Some(status), message: message.into() }
    }
    /// 429 or 5xx — worth falling over to another provider.
    pub fn is_retryable(&self) -> bool {
        matches!(self.status, Some(429) | Some(500..=599))
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ProviderError {}

/// `BaseProvider` from base.ts. Object-safe; providers live behind
/// `Arc<dyn Provider>`.
#[async_trait]
pub trait Provider: Send + Sync {
    fn platform(&self) -> String;
    fn name(&self) -> String;

    async fn chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChatCompletionResponse, ProviderError>;

    /// Spawns the streaming work; the returned receiver yields chunks until
    /// the upstream `[DONE]` sentinel (channel closes).
    async fn stream_chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChunkReceiver, ProviderError>;

    /// `Ok(true)` — key accepted; `Ok(false)` — confirmed 401/403; `Err` —
    /// transport error (DNS/timeout/TLS), which health checks must NOT treat
    /// as a failure count (matches TS comment in openai-compat.ts).
    async fn validate_key(&self, api_key: &str) -> Result<bool, ProviderError>;
}

/// Shared reqwest client (matches TS global fetch, with per-request timeouts).
pub fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("freellmapi/0.1")
            .build()
            .expect("reqwest client")
    })
}

/// `fetchWithTimeout` from base.ts. No timeout when `timeout_ms` is None.
pub async fn send_request(
    req: reqwest::RequestBuilder,
    timeout_ms: Option<u64>,
) -> Result<reqwest::Response, ProviderError> {
    let req = match timeout_ms {
        Some(ms) => req.timeout(std::time::Duration::from_millis(ms)),
        None => req,
    };
    req.send().await.map_err(|e| {
        if e.is_timeout() {
            // "timeout" keyword matters: proxy/completions isRetryableError
            // matches on it, mirroring the TS AbortError messages.
            ProviderError::new(format!("Request timeout: {e}"))
        } else {
            ProviderError::new(format!("Request failed: {e}"))
        }
    })
}

/// `makeId()` — `chatcmpl-${Date.now()}-${random base36 slice(2, 8)}`
pub fn make_id() -> String {
    use rand::Rng;
    let millis = chrono::Utc::now().timestamp_millis();
    // base36 digits, 6 chars, like Math.random().toString(36).slice(2, 8)
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut suffix = String::with_capacity(6);
    let mut rng = rand::rng();
    for _ in 0..6 {
        let i: usize = rng.random_range(0..36);
        suffix.push(ALPHABET[i] as char);
    }
    format!("chatcmpl-{millis}-{suffix}")
}

/// Port of `normalizeChoices` in openai-compat.ts:
/// - Flatten array content (Mistral magistral) → join text segments.
/// - Fold reasoning_content/reasoning into empty content when no tool_calls.
pub fn normalize_choices(data: &mut ChatCompletionResponse) {
    for choice in data.choices.iter_mut() {
        let msg = &mut choice.message;
        let has_tool_calls = msg
            .tool_calls
            .as_ref()
            .map(|tcs| !tcs.is_empty())
            .unwrap_or(false);

        // Flatten array content → join text segments.
        if let crate::types::MessageContent::Parts(parts) = &msg.content {
            let mut flat = String::new();
            for seg in parts {
                match (&seg.text, &seg.r#type) {
                    (Some(t), _) => flat.push_str(t),
                    _ => {}
                }
            }
            // String segments inside the parts array are also tolerated by TS
            // (typeof seg === 'string'); serde's untagged ContentPart drops
            // them, which mirrors `{ text: undefined }` → '' anyway.
            msg.content = crate::types::MessageContent::Text(flat);
        }

        // Fold reasoning into content if content is empty/null AND there are
        // no tool_calls (see TS comment).
        let empty_content = match &msg.content {
            crate::types::MessageContent::Text(s) => s.is_empty(),
            crate::types::MessageContent::Null(_) => true,
            crate::types::MessageContent::Parts(_) => false,
        };
        if !has_tool_calls && empty_content {
            let fold = msg
                .extra
                .get("reasoning_content")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    msg.extra
                        .get("reasoning")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .map(|s| s.to_string());
            if let Some(fold) = fold {
                msg.content = crate::types::MessageContent::Text(fold);
            }
        }
    }
}
