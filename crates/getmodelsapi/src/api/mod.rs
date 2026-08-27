//! Main API, mirroring `getmodelsapi/src/api/index.ts`.

pub mod cohere;
pub mod google;
pub mod groq;
pub mod kilo;
pub mod mistral;
pub mod providers;
pub mod together;

use crate::config;
use crate::scraper::scrape_all_providers;
use crate::types::{FetchParams, GetModelsOptions, Model, ProviderConfig, ProviderType};
use crate::utils::cache::{cache_get, cache_set, ONE_HOUR};
use providers::fetch_by_provider;
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

// Two-level cache: in-memory (5-min TTL) + disk (1 hour, shared with TS).
const FIVE_MINUTES: i64 = 300_000;

type MemEntry = (Vec<Model>, i64);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn mem_cache() -> &'static Mutex<std::collections::HashMap<String, MemEntry>> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, MemEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn get_mem_cached(key: &str) -> Option<Vec<Model>> {
    let mut cache = mem_cache().lock().unwrap();
    let entry = cache.get(key)?;
    let now = now_ms();
    if now - entry.1 > FIVE_MINUTES {
        cache.remove(key);
        return None;
    }
    Some(entry.0.clone())
}

fn set_mem_cache(key: &str, data: Vec<Model>) {
    if let Ok(mut cache) = mem_cache().lock() {
        cache.insert(key.to_string(), (data, now_ms()));
    }
}

/// Mirror of `getCacheKey` — `models:<provider>:<JSON.stringify(params)>`.
fn get_cache_key(provider: &str, params: &FetchParams<'_>) -> String {
    let json = serde_json::to_string(params).unwrap_or_else(|_| "{}".into());
    format!("models:{provider}:{json}")
}

fn sort_providers_by_free_tier(mut providers: Vec<ProviderConfig>) -> Vec<ProviderConfig> {
    providers.sort_by(|a, b| {
        let a_score = if a.free_tier && a.api_key.is_some() {
            0
        } else if a.free_tier {
            1
        } else {
            2
        };
        let b_score = if b.free_tier && b.api_key.is_some() {
            0
        } else if b.free_tier {
            1
        } else {
            2
        };
        a_score.cmp(&b_score).then(a.priority.cmp(&b.priority))
    });
    providers
}

/// Search predicate shared by `fetch_models` and the final `get_models`
/// filter: case-insensitive substring match on name/description/id.
fn matches_search(m: &Model, q: &str) -> bool {
    m.name.to_lowercase().contains(q)
        || m.description
            .as_ref()
            .map(|d| d.to_lowercase().contains(q))
            .unwrap_or(false)
        || m.id.to_lowercase().contains(q)
}

async fn fetch_models(provider: &str, params: &FetchParams<'_>) -> Vec<Model> {
    let key = get_cache_key(provider, params);

    // 1. Try memory cache
    if let Some(mem_result) = get_mem_cached(&key) {
        return mem_result;
    }

    // 2. Try disk cache
    if let Some(disk_result) = cache_get::<Vec<Model>>(&key) {
        set_mem_cache(&key, disk_result.clone());
        return disk_result;
    }

    // 3. Fetch from provider
    let mut models: Vec<Model> = Vec::new();
    let provider_config = config::providers().into_iter().find(|p| p.name == provider);

    if let Some(ref cfg) = provider_config {
        if let Some(mods) = fetch_by_provider(provider, cfg).await {
            models = mods;
        }
    }

    if models.is_empty() {
        if let Some(ref cfg) = provider_config {
            if cfg.supports_scraping {
                let scraper_results = scrape_all_providers(std::slice::from_ref(cfg)).await;
                for result in scraper_results {
                    if result.success && !result.models.is_empty() {
                        models.extend(result.models);
                    }
                }
            }
        }
    }

    // Apply filters
    if let Some(q) = &params.search {
        let q = q.to_lowercase();
        models.retain(|m| matches_search(m, &q));
    }
    if let Some(limit) = params.limit {
        if limit > 0 {
            models.truncate(limit);
        }
    }
    if let Some(offset) = params.offset {
        if offset > 0 && !models.is_empty() {
            let offset = offset.min(models.len());
            models.drain(0..offset);
        }
    }

    // Cache: memory + disk
    set_mem_cache(&key, models.clone());
    cache_set(&key, &models, ONE_HOUR);

    models
}

/// Main entry point, mirroring `getModels`.
pub async fn get_models(options: GetModelsOptions) -> Vec<Model> {
    let provider = options.provider;
    let gateway = options.gateway;
    let search = options.search;
    let free = options.free;
    let exclude_list: Vec<String> = options.exclude;
    let limit = options.limit.unwrap_or(100);
    let offset = options.offset.unwrap_or(0);

    if let Some(provider) = provider {
        if exclude_list.iter().any(|e| e == &provider) {
            return Vec::new();
        }
        let params = FetchParams {
            search: search.as_deref(),
            limit: Some(limit),
            offset: Some(offset),
        };
        return fetch_models(&provider, &params).await;
    }

    if let Some(gateway) = gateway {
        if exclude_list.iter().any(|e| e == &gateway) {
            return Vec::new();
        }
        let gw = config::providers()
            .into_iter()
            .find(|p| p.name == gateway && p.type_ == ProviderType::Gateway);
        let Some(gw) = gw else {
            return Vec::new();
        };
        let params = FetchParams {
            search: search.as_deref(),
            limit: Some(limit),
            offset: Some(offset),
        };
        return fetch_models(&gw.name, &params).await;
    }

    // All providers: sort, fetch sequentially, dedup by model id (last write wins).
    let sorted = sort_providers_by_free_tier(config::providers());
    let mut all_models: Vec<Model> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for p in sorted {
        if exclude_list.iter().any(|e| e == &p.name) {
            continue;
        }
        let params = FetchParams {
            search: search.as_deref(),
            limit: Some(limit),
            offset: Some(offset),
        };
        let models = fetch_models(&p.name, &params).await;
        for m in models {
            if seen_ids.contains(&m.id) {
                // "allModels.set(m.id, m)" — last write wins, position preserved.
                if let Some(existing) = all_models.iter_mut().find(|x| x.id == m.id) {
                    *existing = m;
                }
            } else {
                seen_ids.insert(m.id.clone());
                all_models.push(m);
            }
        }
    }

    let mut models = all_models;

    if let Some(q) = &search {
        let q = q.to_lowercase();
        models.retain(|m| matches_search(m, &q));
    }

    if free == Some(true) {
        models.retain(|m| m.free_tier == Some(true));
    }

    if limit > 0 {
        models.truncate(limit);
    }
    if offset > 0 && !models.is_empty() {
        let offset = offset.min(models.len());
        models.drain(0..offset);
    }

    models
}

/// Mirror of `getProviders`: every registered provider with `apiKey` stripped
/// (it can never be serialized anyway).
pub async fn get_providers() -> Vec<ProviderConfig> {
    config::providers()
        .into_iter()
        .map(|mut p| {
            p.api_key = None;
            p
        })
        .collect()
}

pub use crate::utils::cache::clear_cache;