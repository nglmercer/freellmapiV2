//! Model enrichment:
//! cross-reference Google context windows with OpenRouter and scrape the
//! individual Google model page when the context window is still small.

use crate::types::Model;
use crate::utils::cache::{cache_get, cache_set, ONE_HOUR};
use crate::utils::http::{get_json, get_text, HttpConfig};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Stored value for `enrich-google-<id>`.
#[derive(Debug, Serialize, Deserialize)]
struct ContextWindow {
    #[serde(rename = "contextWindow")]
    context_window: i64,
}

fn strip_google_ctx_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"-preview-\d{2}-\d{4}$").unwrap())
}

/// Build a Google model-to-context-length map from OpenRouter and cache it for
/// one hour.
async fn get_google_context_windows() -> HashMap<String, i64> {
    const KEY: &str = "crossref-openrouter-context";
    if let Some(entries) = cache_get::<Vec<(String, i64)>>(KEY) {
        return entries.into_iter().collect();
    }

    let mut map = HashMap::new();
    let config = HttpConfig {
        timeout: Duration::from_secs(15),
        ..Default::default()
    };
    if let Ok(value) = get_json("https://openrouter.ai/api/v1/models", &config).await {
        if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
            for m in arr {
                let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = m
                    .get("context_length")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if id.starts_with("google/") && ctx > 0 {
                    // "google/gemini-2.5-flash" → "gemini-2.5-flash"
                    let name = id.replacen("google/", "", 1);
                    map.insert(name.clone(), ctx);
                    // Also index by slug variants (drop "-preview-MM-YYYY")
                    let stub = strip_google_ctx_re().replace_all(&name, "").into_owned();
                    map.insert(stub, ctx);
                }
            }
        }
    }

    let entries: Vec<(String, i64)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    cache_set(KEY, &entries, ONE_HOUR);
    map
}

/// Extract a context window from a single Google model documentation page.
/// Returns `None` on failure or when no supported pattern matches.
async fn scrape_google_model_page(model_id: &str) -> Option<i64> {
    let url = format!("https://ai.google.dev/gemini-api/docs/models/{model_id}");
    let config = HttpConfig {
        headers: vec![("User-Agent".into(), "Mozilla/5.0 GetModels".into())],
        timeout: Duration::from_secs(8),
        ..Default::default()
    };
    let html = get_text(&url, &config).await.ok()?;

    // "1M token context window" style
    if let Some(caps) = Regex::new(r"(?i)(\d+)\s*[mM]\s*(?:token|context)")
        .unwrap()
        .captures(&html)
    {
        if let Some(g) = caps.get(1) {
            if let Ok(n) = g.as_str().parse::<i64>() {
                return Some(n * 1_000_000);
            }
        }
    }

    // "128k token" style
    if let Some(caps) = Regex::new(r"(?i)(\d+)\s*[kK]\s*(?:token|context)")
        .unwrap()
        .captures(&html)
    {
        if let Some(g) = caps.get(1) {
            if let Ok(n) = g.as_str().parse::<i64>() {
                return Some(n * 1_000);
            }
        }
    }

    // Raw number with "input token"
    if let Some(caps) = Regex::new(r"(?i)input\s+token\s+limit[^<]*?(\d[\d,]*)")
        .unwrap()
        .captures(&html)
    {
        if let Some(g) = caps.get(1) {
            let clean = g.as_str().replace(',', "");
            if let Ok(n) = clean.parse::<i64>() {
                return Some(n);
            }
        }
    }

    // Context window number
    if let Some(caps) = Regex::new(r"(?i)context\s*(?:length|window)\s*(?::|is|of)?\s*(\d[\d,]*)")
        .unwrap()
        .captures(&html)
    {
        if let Some(g) = caps.get(1) {
            let clean = g.as_str().replace(',', "");
            if let Ok(n) = clean.parse::<i64>() {
                return Some(n);
            }
        }
    }

    None
}

async fn enrich_google_model(mut model: Model) -> Model {
    if model.context_window > 40_000 {
        return model; // already enriched
    }

    let cache_key = format!("enrich-google-{}", model.id);
    if let Some(cached) = cache_get::<ContextWindow>(&cache_key) {
        model.context_window = cached.context_window;
        return model;
    }

    // Method 1: Cross-reference with OpenRouter (cached)
    let ctx_map = get_google_context_windows().await;
    if let Some(&ctx) = ctx_map.get(&model.id) {
        if ctx > model.context_window {
            let cw = ContextWindow {
                context_window: ctx,
            };
            cache_set(&cache_key, &cw, ONE_HOUR);
            model.context_window = ctx;
            return model;
        }
    }

    // Method 2: Direct page scrape for unknown models
    if let Some(scraped) = scrape_google_model_page(&model.id).await {
        if scraped > model.context_window {
            let cw = ContextWindow {
                context_window: scraped,
            };
            cache_set(&cache_key, &cw, ONE_HOUR);
            model.context_window = scraped;
        }
    }

    model
}

/// Enrich one model when additional context metadata is available.
pub async fn enrich_model(model: Model) -> Model {
    if model.provider == "google" && model.context_window < 40_000 {
        return enrich_google_model(model).await;
    }
    model
}

/// Enrich a batch while preserving input ordering.
pub async fn enrich_models(models: Vec<Model>) -> Vec<Model> {
    let mut out = Vec::with_capacity(models.len());
    for model in models {
        out.push(enrich_model(model).await);
    }
    out
}
