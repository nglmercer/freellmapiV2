//! Rust port integration tests for the TS `getmodelsapi/tests/scraper.test.ts`.
//!
//! Tests that hit the network are gated behind `GETMODELS_NETWORK_TESTS=1`
//! and skip gracefully otherwise.

use getmodelsapi::config::providers as all_providers;
use getmodelsapi::types::{GetModelsOptions, Model, ProviderConfig, ProviderType};
use getmodelsapi::{clear_cache, get_models, get_providers};
use getmodelsapi::{scraper::providers::factory::get_scraper, utils::cache};

/// Providers that work without any API key (public APIs / doc-page scrapes).
const PUBLIC_PROVIDERS: [&str; 8] = [
    "google",
    "mistral",
    "huggingface",
    "openrouter",
    "kilo",
    "aimlapi",
    "novita",
    "sambanova",
];

fn network_enabled() -> bool {
    matches!(
        std::env::var("GETMODELS_NETWORK_TESTS").as_deref(),
        Ok("1") | Ok("true")
    )
}

fn make_config(name: &str) -> ProviderConfig {
    all_providers()
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("Unknown provider: {name}"))
}

// ── Provider configuration ──

#[test]
fn all_providers_have_required_fields() {
    let providers = all_providers();
    assert!(!providers.is_empty());
    for p in &providers {
        assert!(!p.name.is_empty());
        assert!(matches!(p.type_, ProviderType::Provider | ProviderType::Gateway));
        assert!(p.supports_scraping || !p.supports_scraping); // bool
        assert!(p.free_tier || !p.free_tier); // bool
        assert!(p.priority > 0);
    }
}

#[test]
fn public_providers_exist_and_support_scraping() {
    for name in PUBLIC_PROVIDERS {
        let p = all_providers().into_iter().find(|pr| pr.name == name);
        assert!(p.is_some(), "missing provider {name}");
        assert!(p.unwrap().supports_scraping);
    }
}

#[test]
fn no_scraper_registered_for_auth_only_providers() {
    // together, cohere, groq: no public API, no scraper
    for name in ["together", "cohere", "groq"] {
        let config = make_config(name);
        assert!(
            get_scraper(name, config).is_none(),
            "expected no scraper for {name}"
        );
    }
}

#[test]
fn free_tier_providers_list_is_correct() {
    let free: Vec<String> = all_providers()
        .into_iter()
        .filter(|p| p.free_tier)
        .map(|p| p.name)
        .collect();
    assert!(free.contains(&"google".to_string()));
    assert!(free.contains(&"mistral".to_string()));
    assert!(free.contains(&"together".to_string()));
    assert!(free.contains(&"cohere".to_string()));
}

// ── getProviders() ──

#[tokio::test]
async fn get_providers_returns_all_without_api_key() {
    let providers = get_providers().await;
    assert_eq!(providers.len(), all_providers().len());
    for p in &providers {
        assert!(p.api_key.is_none(), "apiKey must not be exposed");
        assert!(p.free_tier || !p.free_tier);
    }
}

#[tokio::test]
async fn get_providers_sorted_by_priority() {
    let providers = get_providers().await;
    for w in providers.windows(2) {
        assert!(w[1].priority >= w[0].priority);
    }
}

// ── Local (non-network) filter short-circuits ──

#[tokio::test]
async fn exclude_provider_returns_empty_without_network() {
    let models = get_models(GetModelsOptions {
        provider: Some("huggingface".into()),
        exclude: vec!["huggingface".into()],
        ..Default::default()
    })
    .await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn exclude_gateway_returns_empty_without_network() {
    let models = get_models(GetModelsOptions {
        gateway: Some("openrouter".into()),
        exclude: vec!["openrouter".into()],
        ..Default::default()
    })
    .await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn unknown_gateway_returns_empty_without_network() {
    let models = get_models(GetModelsOptions {
        gateway: Some("does-not-exist".into()),
        ..Default::default()
    })
    .await;
    assert!(models.is_empty());
}

// ── Wire format (serde) ──

#[test]
fn model_serializes_with_camel_case_wire_format() {
    let m = Model {
        id: "test/1".into(),
        name: "Test One".into(),
        provider: "test".into(),
        gateway: None,
        context_window: 128_000,
        supported_features: vec!["chat".into()],
        pricing: None,
        url: Some("https://example.com".into()),
        description: None,
        free_tier: Some(true),
    };
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(json["contextWindow"], 128_000);
    assert_eq!(json["supportedFeatures"][0], "chat");
    assert_eq!(json["freeTier"], true);
    // None optional fields are omitted (like JSON.stringify in TS)
    assert!(json.get("gateway").is_none());
    assert!(json.get("pricing").is_none());
    assert!(json.get("description").is_none());
    // It must round-trip
    let back: Model = serde_json::from_value(json).unwrap();
    assert_eq!(back.id, "test/1");
}

#[test]
fn provider_config_never_serializes_api_key() {
    let p = ProviderConfig {
        name: "x".into(),
        type_: ProviderType::Provider,
        base_url: "https://x".into(),
        api_key: Some("secret".into()),
        supports_scraping: true,
        priority: 1,
        free_tier: false,
    };
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["name"], "x");
    assert_eq!(json["type"], "provider");
    assert_eq!(json["baseUrl"], "https://x");
    assert_eq!(json["supportsScraping"], true);
    assert_eq!(json["freeTier"], false);
    assert!(json.get("apiKey").is_none());
}

// ── Disk cache ──

#[test]
fn cache_roundtrips_and_clear() {
    let key = format!("test-roundtrip-{}", std::process::id());
    let path = getmodelsapi::utils::cache::get_cache_path(&key);
    let _ = std::fs::remove_file(&path);

    cache::cache_set(&key, &vec![1u8, 2, 3], cache::ONE_HOUR);
    assert_eq!(cache::cache_get::<Vec<u8>>(&key), Some(vec![1, 2, 3]));

    // clear_cache removes it
    assert!(clear_cache() >= 1);
    assert_eq!(cache::cache_get::<Vec<u8>>(&key), None);

    // Tidy up the empty cache directory we created.
    let dir = std::env::current_dir().unwrap().join(getmodelsapi::utils::cache::CACHE_DIR);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_key_is_sanitized_like_ts() {
    let path = cache::get_cache_path("models:google:{\"limit\":100,\"offset\":0}");
    let fname = path.file_name().unwrap().to_string_lossy().to_string();
    // TS: key.replace(/[^a-z0-9-_.]/gi, "_") — every `:`, `{`, `"`, ` `, `,`
    // becomes an underscore.
    assert!(fname.starts_with("models_google___limit__100__offset__0_"), "{fname}");
    assert!(fname.ends_with(".json"));
    assert!(!fname.contains(':'));
    assert!(!fname.contains('{'));
    assert!(!fname.contains('"'));
}

// ── Scraper execution: public providers must return models (network) ──

async fn assert_scraper_returns_valid_models(name: &str) {
    let config = make_config(name);
    let fut = get_scraper(name, config).expect("expected a scraper");
    let result = fut.await;

    assert!(result.success, "{name}: expected success");
    assert!(!result.models.is_empty(), "{name}: expected >0 models");
    for model in &result.models {
        assert!(!model.id.is_empty(), "{name}: empty id");
        assert!(!model.name.is_empty(), "{name}: empty name");
        assert!(!model.provider.is_empty(), "{name}: empty provider");
        assert!(model.context_window > 0, "{name}: bad context window");
        assert!(!model.supported_features.is_empty());
    }
}

#[tokio::test]
async fn scrapers_return_models_without_api_key() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    for name in PUBLIC_PROVIDERS {
        assert_scraper_returns_valid_models(name).await;
    }
}

// ── getModels() (network) ──

#[tokio::test]
async fn get_models_returns_models_from_public_providers() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    let models = get_models(GetModelsOptions::default()).await;
    assert!(!models.is_empty());
    for m in &models {
        assert!(!m.id.is_empty());
    }
}

#[tokio::test]
async fn get_models_free_only_returns_free_tier() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    let models = get_models(GetModelsOptions {
        free: Some(true),
        ..Default::default()
    })
    .await;
    for m in &models {
        assert_eq!(m.free_tier, Some(true));
    }
}

async fn assert_provider_models(provider: &str) {
    let models = get_models(GetModelsOptions {
        provider: Some(provider.into()),
        ..Default::default()
    })
    .await;
    assert!(!models.is_empty(), "{provider}: expected models");
    for m in &models {
        assert_eq!(m.provider, provider);
    }
}

#[tokio::test]
async fn get_models_google() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    assert_provider_models("google").await;
}

#[tokio::test]
async fn get_models_mistral() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    assert_provider_models("mistral").await;
}

#[tokio::test]
async fn get_models_huggingface() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    assert_provider_models("huggingface").await;
}

#[tokio::test]
async fn search_filters_results() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    // Pull huggingface's full list once, pick a stable search term, and assert
    // the filtered result is a subset.
    let models = get_models(GetModelsOptions {
        provider: Some("huggingface".into()),
        limit: Some(50),
        ..Default::default()
    })
    .await;
    if models.is_empty() {
        eprintln!("skipping (no huggingface models via cache/network)");
        return;
    }
    let first = &models[0];
    let term = first
        .name
        .split('/')
        .next_back()
        .unwrap_or(&first.name)
        .split('-')
        .next()
        .unwrap_or(&first.name)
        .to_string();
    if term.is_empty() {
        return;
    }
    let searched = get_models(GetModelsOptions {
        provider: Some("huggingface".into()),
        search: Some(term.clone()),
        limit: Some(50),
        ..Default::default()
    })
    .await;
    assert!(!searched.is_empty(), "search={term} returned nothing");
    for m in &searched {
        let q = term.to_lowercase();
        assert!(
            m.name.to_lowercase().contains(&q)
                || m.id.to_lowercase().contains(&q)
                || m.description.as_ref().map(|d| d.to_lowercase().contains(&q)).unwrap_or(false)
        );
    }
}

#[tokio::test]
async fn limit_caps_results() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    let models = get_models(GetModelsOptions {
        provider: Some("huggingface".into()),
        limit: Some(5),
        ..Default::default()
    })
    .await;
    assert!(models.len() <= 5);
}

// ── Deduplication (network) ──

#[tokio::test]
async fn no_duplicate_model_ids() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    let models = get_models(GetModelsOptions::default()).await;
    let mut seen = std::collections::HashSet::new();
    for m in &models {
        assert!(seen.insert(&m.id), "duplicate id: {}", m.id);
    }
}

// ── Cache (network): second call must be near-instant ──

#[tokio::test]
async fn cached_call_returns_near_instantly() {
    if !network_enabled() {
        eprintln!("skipping (set GETMODELS_NETWORK_TESTS=1)");
        return;
    }
    let opts = GetModelsOptions {
        provider: Some("huggingface".into()),
        limit: Some(10),
        offset: Some(99),
        ..Default::default()
    };

    // First call populates the caches.
    let _ = get_models(opts.clone()).await;

    // Second call should be near-instant from the memory cache.
    let t1 = std::time::Instant::now();
    let _ = get_models(opts).await;
    let second_duration = t1.elapsed();

    assert!(
        second_duration.as_millis() < 10,
        "expected a cache hit (took {:?})",
        second_duration
    );
}
