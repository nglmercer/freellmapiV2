//! Scraper orchestration.

pub mod providers;

pub use providers::{ScraperOptions, ScraperResult};

use crate::types::ProviderConfig;
use providers::factory::get_scraper;

/// Scrape one provider, returning no result when scraping is unsupported.
pub async fn scrape_models(provider: &ProviderConfig) -> Vec<ScraperResult> {
    if !provider.supports_scraping {
        return Vec::new();
    }
    match get_scraper(&provider.name, provider.clone()) {
        Some(fut) => vec![fut.await],
        None => Vec::new(),
    }
}

/// Run every scraping-capable provider and flatten the results in registry
/// order.
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
