//! Model selection, fallback routing, and penalty tracking.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::decrypt;
use crate::db::schema::{ApiKeyRow, ModelRow, MODEL_COLS};
use crate::error::ApiError;
use crate::providers::{get_provider_with_conn, Provider};
use crate::services::rankings::routing::{self, RoutingCandidate, RoutingStrategy};
use crate::services::ratelimit::{can_make_request, can_use_tokens, is_on_cooldown, Limits};

#[derive(Clone)]
pub struct RouteResult {
    pub provider: Arc<dyn Provider>,
    pub model_id: String,
    pub model_db_id: i64,
    pub api_key: String,
    pub key_id: i64,
    pub platform: String,
    pub display_name: String,
}

#[derive(Default)]
struct RouterState {
    // Round-robin index per platform
    round_robin_index: HashMap<String, i64>,
    // ── Dynamic priority: track 429s per model and demote accordingly ──
    // Key: model_db_id → { count, lastHit, penalty }
    rate_limit_penalties: HashMap<i64, Penalty>,
}

// Penalty decays over time so models recover
const PENALTY_PER_429: i64 = 5; // each 429 adds this many priority positions
const MAX_PENALTY: i64 = 15; // cap so a model doesn't sink forever
const DECAY_INTERVAL_MS: i64 = 5 * 60 * 1000; // penalty decays every 5 minutes
const DECAY_AMOUNT: i64 = 1; // remove this much penalty per decay interval

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Penalty {
    pub count: i64,
    pub last_hit: i64,
    pub penalty: i64,
}

fn state() -> &'static Mutex<RouterState> {
    static STATE: OnceLock<Mutex<RouterState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RouterState::default()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Record a 429 for a model — increases its penalty so it sinks in priority.
pub fn record_rate_limit_hit(model_db_id: i64) {
    let mut s = state().lock().unwrap();
    let now = now_ms();
    let entry = s
        .rate_limit_penalties
        .entry(model_db_id)
        .or_insert(Penalty {
            count: 0,
            last_hit: now,
            penalty: 0,
        });
    entry.count += 1;
    entry.last_hit = now;
    entry.penalty = (entry.penalty + PENALTY_PER_429).min(MAX_PENALTY);
}

/// Record a success for a model — reduces its penalty so it rises back up.
pub fn record_success(model_db_id: i64) {
    let mut s = state().lock().unwrap();
    if let Some(entry) = s.rate_limit_penalties.get_mut(&model_db_id) {
        entry.penalty = (entry.penalty - 1).max(0);
        if entry.penalty == 0 {
            s.rate_limit_penalties.remove(&model_db_id);
        }
    }
}

/// Get the current penalty for a model (with time-based decay).
fn get_penalty(s: &mut RouterState, model_db_id: i64) -> i64 {
    let Some(entry) = s.rate_limit_penalties.get_mut(&model_db_id) else {
        return 0;
    };

    // Apply time-based decay
    let now = now_ms();
    let elapsed = now - entry.last_hit;
    let decay_steps = elapsed / DECAY_INTERVAL_MS;
    if decay_steps > 0 {
        entry.penalty = (entry.penalty - decay_steps * DECAY_AMOUNT).max(0);
        entry.last_hit = now; // reset so we don't double-decay
        if entry.penalty == 0 {
            s.rate_limit_penalties.remove(&model_db_id);
            return 0;
        }
    }

    entry.penalty
}

/// Get current penalties for all models (for the API/dashboard).
pub fn get_all_penalties() -> Vec<PenaltyInfo> {
    let mut s = state().lock().unwrap();
    let mut result: Vec<PenaltyInfo> = Vec::new();
    let ids: Vec<i64> = s.rate_limit_penalties.keys().copied().collect();
    for model_db_id in ids {
        let penalty = get_penalty(&mut s, model_db_id);
        if penalty > 0 {
            let count = s
                .rate_limit_penalties
                .get(&model_db_id)
                .map(|p| p.count)
                .unwrap_or(0);
            result.push(PenaltyInfo {
                model_db_id,
                count,
                penalty,
            });
        }
    }
    result.sort_by_key(|entry| std::cmp::Reverse(entry.penalty));
    result
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PenaltyInfo {
    pub model_db_id: i64,
    pub count: i64,
    pub penalty: i64,
}

/// Return a conservative availability signal for a provider/model pair.
/// Provider key status and the in-memory model/key cooldowns are intentionally
/// kept separate from quality and speed benchmarks.
pub(crate) fn route_availability(conn: &Connection, platform: &str, model_id: &str) -> f64 {
    let keys: Vec<(i64, String)> = conn
        .prepare(
            "SELECT id, status FROM api_keys
             WHERE platform = ?1 AND enabled = 1",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![platform], |row| {
                Ok((row.get(0)?, row.get::<_, String>(1)?))
            })
            .ok()
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>().ok())
        })
        .unwrap_or_default();
    if keys.is_empty() {
        return 0.0;
    }
    let has_usable_key = keys.iter().any(|(key_id, status)| {
        !matches!(status.as_str(), "invalid" | "rate_limited")
            && !is_on_cooldown(platform, model_id, *key_id)
    });
    let recent_failure = conn
        .query_row(
            "SELECT last_failure_at, last_success_at FROM model_performance
             WHERE platform = ?1 AND model_id = ?2",
            rusqlite::params![platform, model_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .ok()
        .and_then(|(failure, success)| {
            let failure = failure?
                .parse::<chrono::DateTime<chrono::FixedOffset>>()
                .ok()?;
            let success = success.and_then(|timestamp| {
                timestamp
                    .parse::<chrono::DateTime<chrono::FixedOffset>>()
                    .ok()
            });
            let failure_is_newer = success.is_none_or(|success| failure > success);
            let age_ms = chrono::Utc::now()
                .signed_duration_since(failure.with_timezone(&chrono::Utc))
                .num_milliseconds();
            (failure_is_newer && (0..=5 * 60 * 1000).contains(&age_ms)).then_some(true)
        })
        .unwrap_or(false);
    if has_usable_key {
        if recent_failure {
            0.5
        } else {
            1.0
        }
    } else if keys.iter().any(|(_, status)| status != "invalid") {
        if recent_failure {
            0.1
        } else {
            0.25
        }
    } else {
        0.0
    }
}

/// Route a request to the best available model.
/// Models are sorted by (base_priority + rate_limit_penalty) so frequently
/// rate-limited models automatically sink below working ones.
///
/// If `preferred_model_db_id` is set, that model gets tried FIRST (sticky
/// sessions). This prevents hallucination from model switching
/// mid-conversation.
///
/// - `estimated_tokens` — estimated total tokens for rate limit check
/// - `skip_keys` — set of "platform:modelId:keyId" to skip (failed on this
///   request)
pub fn route_request(
    conn: &Connection,
    estimated_tokens: i64,
    skip_keys: Option<&HashSet<String>>,
    preferred_model_db_id: Option<i64>,
) -> Result<RouteResult, ApiError> {
    let total_key_count: i64 = conn
        .query_row("SELECT count(*) FROM api_keys WHERE enabled = 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    if total_key_count == 0 {
        return Err(ApiError::new(
            503,
            "No API keys configured. Add at least one key in the dashboard first.",
        ));
    }

    // Pre-compute which platforms have valid keys (optimization to skip
    // platforms with no keys)
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT platform FROM api_keys \
             WHERE enabled = 1 AND status NOT IN ('invalid', 'rate_limited')",
        )
        .map_err(|e| ApiError::new(500, e.to_string()))?;
    let platforms_with_keys: HashSet<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| ApiError::new(500, e.to_string()))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| ApiError::new(500, e.to_string()))?;

    // Get fallback chain ordered by priority
    let mut stmt = conn
        .prepare(
            "SELECT id, model_db_id, priority, manual_priority, enabled \
             FROM fallback_config ORDER BY priority ASC",
        )
        .map_err(|e| ApiError::new(500, e.to_string()))?;
    let fallback_chain: Vec<crate::db::schema::FallbackRow> = stmt
        .query_map([], crate::db::schema::FallbackRow::from_row)
        .map_err(|e| ApiError::new(500, e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let strategy = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'fallback_strategy'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|value| RoutingStrategy::parse(&value))
        .unwrap_or(RoutingStrategy::Manual);

    // Manual mode uses the user-maintained order. Automatic modes score the
    // provider/model pair from quality, local telemetry, reliability, and
    // current key availability; missing dimensions are handled by the
    // routing engine and do not become zero-valued fake benchmarks.
    let mut sorted_chain: Vec<(i64, i64, i64, i64)> = if strategy == RoutingStrategy::Manual {
        let mut s = state().lock().unwrap();
        let mut chain = fallback_chain
            .iter()
            .map(|entry| {
                let effective = entry.priority + get_penalty(&mut s, entry.model_db_id);
                (
                    effective,
                    entry.manual_priority.unwrap_or(entry.priority),
                    entry.model_db_id,
                    entry.enabled,
                )
            })
            .collect::<Vec<_>>();
        chain
            .sort_by_key(|(effective, manual, model_db_id, _)| (*effective, *manual, *model_db_id));
        chain
    } else {
        let mut candidates = Vec::new();
        let fallback_meta: HashMap<i64, (i64, i64)> = fallback_chain
            .iter()
            .map(|entry| {
                (
                    entry.model_db_id,
                    (
                        entry.manual_priority.unwrap_or(entry.priority),
                        entry.enabled,
                    ),
                )
            })
            .collect();
        for entry in &fallback_chain {
            if entry.enabled != 1 {
                continue;
            }
            let model = conn
                .query_row(
                    &format!("SELECT {MODEL_COLS} FROM models WHERE id = ?1"),
                    rusqlite::params![entry.model_db_id],
                    ModelRow::from_row,
                )
                .ok();
            let Some(model) = model else { continue };
            let provider_available = get_provider_with_conn(conn, &model.platform).is_some();
            let performance = crate::services::telemetry::load_performance(
                conn,
                &model.platform,
                &model.model_id,
            )
            .ok()
            .flatten();
            let mut state_guard = state().lock().unwrap();
            let penalty = get_penalty(&mut state_guard, entry.model_db_id);
            drop(state_guard);
            candidates.push(RoutingCandidate {
                model_db_id: entry.model_db_id,
                manual_priority: entry.manual_priority.unwrap_or(entry.priority),
                quality: model.intelligence_score,
                speed: performance
                    .as_ref()
                    .and_then(|performance| performance.ewma_output_tps)
                    .or(model.observed_speed_tps)
                    .or(model.external_speed_tps)
                    .or(model.speed_tokens_per_sec),
                reliability: performance.as_ref().and_then(|performance| {
                    routing::reliability_score(performance.success_count, performance.sample_count)
                }),
                availability: if provider_available {
                    route_availability(conn, &model.platform, &model.model_id)
                } else {
                    0.0
                },
                penalty,
            });
        }
        routing::score_candidates(&candidates, strategy)
            .into_iter()
            .enumerate()
            .filter_map(|(position, scored)| {
                fallback_meta
                    .get(&scored.model_db_id)
                    .map(|(manual, enabled)| {
                        (position as i64 + 1, *manual, scored.model_db_id, *enabled)
                    })
            })
            .collect()
    };

    // Sticky session: move preferred model to front of chain
    if let Some(preferred) = preferred_model_db_id {
        if let Some(idx) = sorted_chain
            .iter()
            .position(|(_, _, id, _)| *id == preferred)
        {
            if idx > 0 {
                let entry = sorted_chain.remove(idx);
                sorted_chain.insert(0, entry);
            }
        }
    }

    for (_effective, _priority, model_db_id, fb_enabled) in sorted_chain {
        if fb_enabled != 1 {
            continue;
        }

        // Get model details
        let model = conn
            .query_row(
                &format!(
                    "SELECT {} FROM models WHERE id = ?1 AND enabled = 1",
                    crate::db::schema::MODEL_COLS
                ),
                rusqlite::params![model_db_id],
                crate::db::schema::ModelRow::from_row,
            )
            .ok();
        let Some(model) = model else {
            tracing::debug!(
                "[ROUTER DEBUG] Skipping: model not found or disabled (modelDbId={model_db_id})"
            );
            continue;
        };

        // Early skip: platform has no valid keys (optimization to avoid
        // iterating through 70+ entries)
        if !platforms_with_keys.contains(&model.platform) {
            continue;
        }

        tracing::debug!(
            "[ROUTER DEBUG] Trying: {}/{} (model.enabled={})",
            model.platform,
            model.model_id,
            model.enabled
        );

        // Check if we have a provider for this platform
        let Some(provider) = get_provider_with_conn(conn, &model.platform) else {
            continue;
        };

        // Get all healthy, enabled keys for this platform
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM api_keys \
                 WHERE platform = ?1 AND enabled = 1 \
                 AND status NOT IN ('invalid', 'rate_limited')",
                crate::db::schema::API_KEY_COLS
            ))
            .map_err(|e| ApiError::new(500, e.to_string()))?;
        let keys: Vec<ApiKeyRow> = stmt
            .query_map(rusqlite::params![model.platform], ApiKeyRow::from_row)
            .map_err(|e| ApiError::new(500, e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if keys.is_empty() {
            tracing::debug!(
                "[ROUTER DEBUG] Skipping {}/{}: no keys for platform {}",
                model.platform,
                model.model_id,
                model.platform
            );
            continue;
        }
        tracing::debug!(
            "[ROUTER DEBUG] Found {} key(s) for {}/{}, attempting...",
            keys.len(),
            model.platform,
            model.model_id
        );

        // Get limits once for this model
        let limits = Limits {
            rpm: model.rpm_limit,
            rpd: model.rpd_limit,
            tpm: model.tpm_limit,
            tpd: model.tpd_limit,
        };

        // Try all keys for this model before giving up on it
        let rr_key = format!("{}:{}", model.platform, model.model_id);
        let mut idx = {
            let s = state().lock().unwrap();
            s.round_robin_index.get(&rr_key).copied().unwrap_or(0)
        };

        for _attempt in 0..keys.len() {
            let key = &keys[(idx.rem_euclid(keys.len() as i64)) as usize];
            idx += 1;

            let skip_id = format!("{}:{}:{}", model.platform, model.model_id, key.id);
            if skip_keys.is_some_and(|sk| sk.contains(&skip_id)) {
                continue;
            }

            // Check cooldown (from previous 429s)
            if is_on_cooldown(&model.platform, &model.model_id, key.id) {
                continue;
            }

            if !can_make_request(&model.platform, &model.model_id, key.id, &limits) {
                continue;
            }
            if !can_use_tokens(
                &model.platform,
                &model.model_id,
                key.id,
                estimated_tokens,
                (limits.tpm, limits.tpd),
            ) {
                continue;
            }

            // We found a working key for this model!
            state()
                .lock()
                .unwrap()
                .round_robin_index
                .insert(rr_key.clone(), idx);
            let decrypted_key = match decrypt(&key.encrypted_key, &key.iv, &key.auth_tag) {
                Ok(k) => k,
                Err(_) => {
                    // Decryption failed (mismatched encryption key). Skip
                    // this key.
                    continue;
                }
            };

            return Ok(RouteResult {
                provider,
                model_id: model.model_id,
                model_db_id: model.id,
                api_key: decrypted_key,
                key_id: key.id,
                platform: model.platform,
                display_name: model.display_name,
            });
        }

        // If we reach here, this specific model has NO available keys.
        // Update round-robin index even if we failed so we don't get stuck.
        state()
            .lock()
            .unwrap()
            .round_robin_index
            .insert(rr_key, idx);

        // We don't explicitly penalize the model here because the fact that we
        // couldn't find a key means we will naturally move to the next model
        // in the sorted chain for THIS specific request.
    }

    // Check if there are any non-invalid keys at all to give a better
    // diagnostic
    let decryptable_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM api_keys WHERE enabled = 1 AND status != 'invalid'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let message = if decryptable_count == 0 {
        "All configured API keys are invalid. Check your keys in the dashboard."
    } else if total_key_count > 0 && skip_keys.is_some_and(|sk| sk.len() as i64 >= total_key_count)
    {
        // Keep this diagnostic stable for clients and operators.
        return Err(ApiError::new(
            429,
            format!("All {total_key_count} API key(s) have been tried and failed. Wait for rate-limit cooldown or add more keys."),
        ));
    } else {
        "All models are currently unavailable due to rate limits or cooldowns. Try again later."
    };

    Err(ApiError::new(429, message))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RouterSnapshot {
    pub round_robin_index: Vec<(String, i64)>,
    pub rate_limit_penalties: Vec<(i64, Penalty)>,
}

pub fn snapshot_router_state() -> RouterSnapshot {
    let s = state().lock().unwrap();
    RouterSnapshot {
        round_robin_index: s
            .round_robin_index
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        rate_limit_penalties: s
            .rate_limit_penalties
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect(),
    }
}

pub fn restore_router_state(snapshot: RouterSnapshot) {
    let mut s = state().lock().unwrap();
    s.round_robin_index.clear();
    for (k, v) in snapshot.round_robin_index {
        s.round_robin_index.insert(k, v);
    }
    s.rate_limit_penalties.clear();
    for (k, v) in snapshot.rate_limit_penalties {
        s.rate_limit_penalties.insert(k, v);
    }
}
