//! Scraper orchestration, mirroring `getmodelsapi/src/scraper/index.ts`.

pub mod providers;

pub use providers::{ScraperOptions, ScraperResult};

use crate::types::ProviderConfig;
use providers::factory::get_scraper;

/// Mirror of `scrapeModels`: returns `[]` when the provider doesn't support
/// scraping, otherwise the single scraper result wrapped in a Vec.
pub async fn scrape_models(provider: &ProviderConfig) -> Vec<ScraperResult> {
    if !provider.supports_scraping {
        return Vec::new();
    }
    match get_scraper(&provider.name, provider.clone()) {
        Some(fut) => vec![fut.await],
        None => Vec::new(),
    }
}

/// Mirror of `scrapeAllProviders`: run every scraping-capable provider and
/// flatten the results.
///
/// The TS version uses `Promise.all`; we run them sequentially (same results,
/// same order, no added parallelism).
pub async fn scrape_all_providers(providers: &[ProviderConfig]) -> Vec<ScraperResult> {
    let mut results = Vec::new();
    for provider in providers {
        if !provider.supports_scraping {
            continue;
        }
        results.extend(scrape_models(provider).await);
    }
    results
}