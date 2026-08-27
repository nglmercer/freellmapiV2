//! Port of `server/src/services/ratelimit.ts` — in-memory sliding window
//! rate limit tracker.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::types::ChatMessage;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TokenStamp {
    pub ts: i64,
    pub tokens: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub timestamps: Vec<i64>,
    pub token_count: i64,
    pub token_timestamps: Vec<TokenStamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StickyEntry {
    pub model_db_id: i64,
    pub last_used: i64,
}

#[derive(Default)]
struct State {
    // Key format: "platform:modelId:keyId:type" where type is rpm|rpd|tpm|tpd
    windows: HashMap<String, Window>,
    // Cooldown: when a provider returns 429, block that model+key for a period.
    // key -> expiry timestamp
    cooldowns: HashMap<String, i64>,
    // Sticky sessions: track which model served each "session"
    // Key: hash of first user message → model_db_id
    sticky_session_map: HashMap<String, StickyEntry>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub const MINUTE: i64 = 60 * 1000;
pub const DAY: i64 = 24 * 60 * MINUTE;

pub struct Limits {
    pub rpm: Option<i64>,
    pub rpd: Option<i64>,
    pub tpm: Option<i64>,
    pub tpd: Option<i64>,
}

fn get_window<'a>(s: &'a mut State, key: &str) -> &'a mut Window {
    s.windows.entry(key.to_string()).or_default()
}

fn prune_timestamps(timestamps: &mut Vec<i64>, window_ms: i64, now: i64) {
    let cutoff = now - window_ms;
    timestamps.retain(|ts| *ts > cutoff);
}

pub fn can_make_request(
    platform: &str,
    model_id: &str,
    key_id: i64,
    limits: &Limits,
) -> bool {
    let now = now_ms();
    let mut s = state().lock().unwrap();

    if let Some(rpm) = limits.rpm {
        let key = format!("{platform}:{model_id}:{key_id}:rpm");
        let w = get_window(&mut s, &key);
        prune_timestamps(&mut w.timestamps, MINUTE, now);
        if w.timestamps.len() as i64 >= rpm {
            return false;
        }
    }

    if let Some(rpd) = limits.rpd {
        let key = format!("{platform}:{model_id}:{key_id}:rpd");
        let w = get_window(&mut s, &key);
        prune_timestamps(&mut w.timestamps, DAY, now);
        if w.timestamps.len() as i64 >= rpd {
            return false;
        }
    }

    true
}

pub fn can_use_tokens(
    platform: &str,
    model_id: &str,
    key_id: i64,
    estimated_tokens: i64,
    limits_tpm_tpd: (Option<i64>, Option<i64>),
) -> bool {
    let now = now_ms();
    let (tpm, tpd) = limits_tpm_tpd;
    let mut s = state().lock().unwrap();

    if let Some(tpm) = tpm {
        let key = format!("{platform}:{model_id}:{key_id}:tpm");
        let w = get_window(&mut s, &key);
        w.token_timestamps.retain(|t| t.ts > now - MINUTE);
        let used: i64 = w.token_timestamps.iter().map(|t| t.tokens).sum();
        if used + estimated_tokens > tpm {
            return false;
        }
    }

    if let Some(tpd) = tpd {
        let key = format!("{platform}:{model_id}:{key_id}:tpd");
        let w = get_window(&mut s, &key);
        w.token_timestamps.retain(|t| t.ts > now - DAY);
        let used: i64 = w.token_timestamps.iter().map(|t| t.tokens).sum();
        if used + estimated_tokens > tpd {
            return false;
        }
    }

    true
}

pub fn record_request(platform: &str, model_id: &str, key_id: i64) {
    let now = now_ms();
    let mut s = state().lock().unwrap();

    let rpm_key = format!("{platform}:{model_id}:{key_id}:rpm");
    get_window(&mut s, &rpm_key).timestamps.push(now);

    let rpd_key = format!("{platform}:{model_id}:{key_id}:rpd");
    get_window(&mut s, &rpd_key).timestamps.push(now);
}

pub fn record_tokens(platform: &str, model_id: &str, key_id: i64, tokens: i64) {
    let now = now_ms();
    let mut s = state().lock().unwrap();

    let tpm_key = format!("{platform}:{model_id}:{key_id}:tpm");
    get_window(&mut s, &tpm_key)
        .token_timestamps
        .push(TokenStamp { ts: now, tokens });

    let tpd_key = format!("{platform}:{model_id}:{key_id}:tpd");
    get_window(&mut s, &tpd_key)
        .token_timestamps
        .push(TokenStamp { ts: now, tokens });
}

pub fn set_cooldown(platform: &str, model_id: &str, key_id: i64, duration_ms: i64) {
    let key = format!("{platform}:{model_id}:{key_id}:cooldown");
    state()
        .lock()
        .unwrap()
        .cooldowns
        .insert(key, now_ms() + duration_ms);
}

pub fn is_on_cooldown(platform: &str, model_id: &str, key_id: i64) -> bool {
    let key = format!("{platform}:{model_id}:{key_id}:cooldown");
    let mut s = state().lock().unwrap();
    let expiry = s.cooldowns.get(&key).copied();
    match expiry {
        None => false,
        Some(expiry) => {
            if now_ms() > expiry {
                s.cooldowns.remove(&key);
                false
            } else {
                true
            }
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitUsage {
    pub rpm: crate::types::UsageWindow,
    pub rpd: crate::types::UsageWindow,
    pub tpm: crate::types::UsageWindow,
}

pub fn get_rate_limit_status(
    platform: &str,
    model_id: &str,
    key_id: i64,
    limits: &Limits,
) -> RateLimitUsage {
    let now = now_ms();
    let mut s = state().lock().unwrap();

    let rpm_key = format!("{platform}:{model_id}:{key_id}:rpm");
    let rpm_w = get_window(&mut s, &rpm_key);
    prune_timestamps(&mut rpm_w.timestamps, MINUTE, now);
    let rpm_used = rpm_w.timestamps.len() as i64;

    let rpd_key = format!("{platform}:{model_id}:{key_id}:rpd");
    let rpd_w = get_window(&mut s, &rpd_key);
    prune_timestamps(&mut rpd_w.timestamps, DAY, now);
    let rpd_used = rpd_w.timestamps.len() as i64;

    let tpm_key = format!("{platform}:{model_id}:{key_id}:tpm");
    let tpm_w = get_window(&mut s, &tpm_key);
    tpm_w.token_timestamps.retain(|t| t.ts > now - MINUTE);
    let tpm_used: i64 = tpm_w.token_timestamps.iter().map(|t| t.tokens).sum();

    RateLimitUsage {
        rpm: crate::types::UsageWindow { used: rpm_used, limit: limits.rpm },
        rpd: crate::types::UsageWindow { used: rpd_used, limit: limits.rpd },
        tpm: crate::types::UsageWindow { used: tpm_used, limit: limits.tpm },
    }
}

const STICKY_TTL_MS: i64 = 30 * 60 * 1000; // 30 min session TTL

pub fn get_session_key(messages: &[ChatMessage]) -> String {
    let first_user = messages.iter().find(|m| m.role == "user");

    // Guard clause checking that content is a valid, non-null string before
    // hashing.
    let Some(crate::types::MessageContent::Text(content)) = first_user.map(|m| &m.content) else {
        return String::new();
    };

    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(content.as_bytes());
    let hash = hex::encode(hasher.finalize());
    format!("{hash}:{}", if messages.len() > 2 { "multi" } else { "single" })
}

pub fn get_sticky_model(messages: &[ChatMessage]) -> Option<i64> {
    let has_assistant = messages.iter().any(|m| m.role == "assistant");
    if !has_assistant {
        return None;
    }

    let key = get_session_key(messages);
    if key.is_empty() {
        return None;
    }

    let mut s = state().lock().unwrap();
    let entry = s.sticky_session_map.get(&key).cloned();
    match entry {
        None => None,
        Some(entry) => {
            if now_ms() - entry.last_used > STICKY_TTL_MS {
                s.sticky_session_map.remove(&key);
                None
            } else {
                Some(entry.model_db_id)
            }
        }
    }
}

pub fn set_sticky_model(messages: &[ChatMessage], model_db_id: i64) {
    let key = get_session_key(messages);
    if key.is_empty() {
        return;
    }
    let mut s = state().lock().unwrap();
    s.sticky_session_map.insert(
        key,
        StickyEntry { model_db_id, last_used: now_ms() },
    );

    if s.sticky_session_map.len() > 500 {
        let now = now_ms();
        s.sticky_session_map.retain(|_, v| now - v.last_used <= STICKY_TTL_MS);
    }
}

// ---- Persistence snapshots (runtime-state.json compatible) ----

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitSnapshot {
    pub windows: Vec<(String, Window)>,
    pub cooldowns: Vec<(String, i64)>,
    pub sticky_sessions: Vec<(String, StickyEntry)>,
}

pub fn snapshot_rate_limit_state() -> RateLimitSnapshot {
    let s = state().lock().unwrap();
    RateLimitSnapshot {
        windows: s.windows.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        cooldowns: s.cooldowns.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        sticky_sessions: s
            .sticky_session_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

pub fn restore_rate_limit_state(snapshot: RateLimitSnapshot) {
    let mut s = state().lock().unwrap();
    s.windows.clear();
    for (k, v) in snapshot.windows {
        s.windows.insert(k, v);
    }
    s.cooldowns.clear();
    for (k, v) in snapshot.cooldowns {
        s.cooldowns.insert(k, v);
    }
    s.sticky_session_map.clear();
    for (k, v) in snapshot.sticky_sessions {
        s.sticky_session_map.insert(k, v);
    }
}
