//! Modality-aware candidate selection and fallback for non-chat operations.
//!
//! Candidates come only from the explicit `media_models` table, filtered by
//! the provider's declared capabilities and by usable API keys. Fallback moves
//! to the next candidate only when the upstream failure is safe to retry; it
//! never switches output formats or feature sets.

use std::future::Future;
use std::sync::Arc;

use rusqlite::Connection;

use crate::crypto::decrypt;
use crate::error::ApiError;
use crate::providers::{get_provider_with_conn, Capability, Provider, ProviderError};

#[derive(Clone)]
pub struct MediaCandidate {
    pub provider: Arc<dyn Provider>,
    pub platform: String,
    pub model_id: String,
    pub key_id: i64,
    pub api_key: String,
}

/// Ordered candidates for `capability` and `model_id`.
///
/// Errors are explicit: an undeclared model is a 400, a provider without the
/// capability is a 400, and a model with no usable key is a 503.
pub fn candidates(
    conn: &Connection,
    capability: Capability,
    model_id: &str,
) -> Result<Vec<MediaCandidate>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT platform FROM media_models \
             WHERE model_id = ?1 AND capability = ?2 AND enabled = 1 \
             ORDER BY priority ASC, id ASC",
        )
        .map_err(db_error)?;
    let platforms: Vec<String> = stmt
        .query_map(rusqlite::params![model_id, capability.as_str()], |r| {
            r.get::<_, String>(0)
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(db_error)?;

    if platforms.is_empty() {
        return Err(ApiError::new(
            400,
            format!(
                "Model '{model_id}' is not enabled for '{}'. Declare it in the media model settings.",
                capability.as_str()
            ),
        ));
    }

    let mut out = Vec::new();
    let mut unsupported: Vec<String> = Vec::new();
    for platform in platforms {
        let Some(provider) = get_provider_with_conn(conn, &platform) else {
            continue;
        };
        if !provider.capabilities().contains(capability) {
            unsupported.push(platform);
            continue;
        }
        let mut keys = conn
            .prepare(
                "SELECT id, encrypted_key, iv, auth_tag FROM api_keys \
                 WHERE platform = ?1 AND enabled = 1 AND status NOT IN ('invalid', 'rate_limited') \
                 ORDER BY id ASC",
            )
            .map_err(db_error)?;
        let rows: Vec<(i64, String, String, String)> = keys
            .query_map(rusqlite::params![platform], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<_>>()
            .map_err(db_error)?;
        for (key_id, encrypted, iv, tag) in rows {
            if let Ok(api_key) = decrypt(&encrypted, &iv, &tag) {
                out.push(MediaCandidate {
                    provider: provider.clone(),
                    platform: platform.clone(),
                    model_id: model_id.to_string(),
                    key_id,
                    api_key,
                });
            }
        }
    }

    if out.is_empty() {
        if !unsupported.is_empty()
            && unsupported.len() == count_platforms(conn, capability, model_id)?
        {
            return Err(ApiError::new(
                400,
                format!(
                    "Provider '{}' does not support '{}'",
                    unsupported.join("', '"),
                    capability.as_str()
                ),
            ));
        }
        return Err(ApiError::new(
            503,
            format!("No usable API key for model '{model_id}'. Add one in the dashboard first."),
        ));
    }
    Ok(out)
}

fn count_platforms(
    conn: &Connection,
    capability: Capability,
    model_id: &str,
) -> Result<usize, ApiError> {
    conn.query_row(
        "SELECT count(*) FROM media_models WHERE model_id = ?1 AND capability = ?2 AND enabled = 1",
        rusqlite::params![model_id, capability.as_str()],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as usize)
    .map_err(db_error)
}

fn db_error(e: rusqlite::Error) -> ApiError {
    ApiError::new(500, e.to_string())
}

/// Whether an upstream failure may be retried on another candidate.
///
/// `ambiguous_ok` controls transport failures (no HTTP status): the request
/// may have reached the provider, so only operations that are cheap and
/// idempotent-enough should retry on them.
pub fn may_fall_back(err: &ProviderError, ambiguous_ok: bool) -> bool {
    match err.status {
        Some(429) | Some(500..=599) => true,
        Some(_) => false,
        None => ambiguous_ok,
    }
}

/// Runs `op` against candidates in order. Returns the first success, or the
/// last error when no candidate could complete the operation.
pub async fn run_with_fallback<T, F, Fut>(
    candidates: Vec<MediaCandidate>,
    ambiguous_ok: bool,
    mut op: F,
) -> Result<(T, MediaCandidate), ProviderError>
where
    F: FnMut(MediaCandidate) -> Fut,
    Fut: Future<Output = Result<T, ProviderError>>,
{
    let mut last: Option<ProviderError> = None;
    for candidate in candidates {
        let winner = candidate.clone();
        match op(candidate).await {
            Ok(value) => return Ok((value, winner)),
            Err(err) => {
                let retry = may_fall_back(&err, ambiguous_ok);
                tracing::warn!(
                    platform = %winner.platform,
                    model = %winner.model_id,
                    status = ?err.status,
                    retry,
                    "media request failed: {}",
                    err.message
                );
                last = Some(err);
                if !retry {
                    break;
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| ProviderError::new("No media candidates available")))
}

/// Records a media call in the shared request log so usage shows up with
/// the other operations.
pub fn record_media_request(
    conn: &Connection,
    candidate: &MediaCandidate,
    status: &str,
    latency_ms: i64,
    error: Option<&str>,
) {
    let _ = conn.execute(
        "INSERT INTO requests (platform, model_id, status, latency_ms, error) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            candidate.platform,
            candidate.model_id,
            status,
            latency_ms,
            error
        ],
    );
}

/// Maps a provider failure to an HTTP status and JSON error body.
pub fn provider_error_status(err: &ProviderError) -> u16 {
    err.status
        .filter(|s| (400..=499).contains(s))
        .unwrap_or(502)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_policy_by_failure_kind() {
        let http = |s| ProviderError::http(s, "x");
        assert!(may_fall_back(&http(429), false));
        assert!(may_fall_back(&http(503), false));
        // Client errors never fall back: the request itself is the problem.
        assert!(!may_fall_back(&http(400), true));
        assert!(!may_fall_back(&http(404), true));
        // Transport failures are ambiguous: only retried when allowed.
        let transport = ProviderError::new("Request failed: connection reset");
        assert!(may_fall_back(&transport, true));
        assert!(!may_fall_back(&transport, false));
    }

    #[test]
    fn provider_status_mapping() {
        assert_eq!(provider_error_status(&ProviderError::http(400, "bad")), 400);
        assert_eq!(
            provider_error_status(&ProviderError::http(429, "slow")),
            429
        );
        assert_eq!(
            provider_error_status(&ProviderError::http(500, "boom")),
            502
        );
        assert_eq!(provider_error_status(&ProviderError::new("timeout")), 502);
    }

    #[tokio::test]
    async fn fallback_stops_on_non_retryable_error() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let candidates = vec![candidate("a"), candidate("b")];
        let result: Result<((), MediaCandidate), _> = run_with_fallback(candidates, true, |_c| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(ProviderError::http(400, "unsupported format")) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fallback_moves_to_next_candidate_on_rate_limit() {
        let candidates = vec![candidate("a"), candidate("b")];
        let (value, winner) = run_with_fallback(candidates, false, |c| async move {
            if c.platform == "a" {
                Err(ProviderError::http(429, "slow down"))
            } else {
                Ok(42)
            }
        })
        .await
        .unwrap();
        assert_eq!(value, 42);
        assert_eq!(winner.platform, "b");
    }

    fn candidate(platform: &str) -> MediaCandidate {
        MediaCandidate {
            provider: Arc::new(NoopProvider),
            platform: platform.to_string(),
            model_id: "m".to_string(),
            key_id: 1,
            api_key: "k".to_string(),
        }
    }

    struct NoopProvider;

    #[async_trait::async_trait]
    impl Provider for NoopProvider {
        fn platform(&self) -> String {
            "noop".into()
        }
        fn name(&self) -> String {
            "Noop".into()
        }
        async fn chat_completion(
            &self,
            _: &str,
            _: &[crate::types::ChatMessage],
            _: &str,
            _: &crate::types::CompletionOptions,
        ) -> Result<crate::types::ChatCompletionResponse, ProviderError> {
            Err(ProviderError::new("unused"))
        }
        async fn stream_chat_completion(
            &self,
            _: &str,
            _: &[crate::types::ChatMessage],
            _: &str,
            _: &crate::types::CompletionOptions,
        ) -> Result<crate::providers::ChunkReceiver, ProviderError> {
            Err(ProviderError::new("unused"))
        }
        async fn validate_key(&self, _: &str) -> Result<bool, ProviderError> {
            Ok(true)
        }
    }
}
