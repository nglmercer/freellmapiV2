//! Scraper dispatch, mirroring `getmodelsapi/src/scraper/providers/factory.ts`.

use super::{
    aimlapi, google, huggingface, kilo, mistral, novita, openrouter, sambanova,
    ScraperResult,
};
use crate::types::ProviderConfig;
use std::future::Future;
use std::pin::Pin;

/// A boxed scraper future.
pub type ScraperFuture = Pin<Box<dyn Future<Output = ScraperResult> + Send>>;

/// Mirror of `getScraper(providerName, config)`. Returns `None` for providers
/// without a registered scraper (TS throws for those). Takes ownership of the
/// config so the returned future is `'static` and awaitable directly.
pub fn get_scraper(provider_name: &str, config: ProviderConfig) -> Option<ScraperFuture> {
    match provider_name {
        "huggingface" => Some(Box::pin(huggingface::scrape_huggingface(config))),
        "google" => Some(Box::pin(google::scrape_google(config))),
        "mistral" => Some(Box::pin(mistral::scrape_mistral(config))),
        "openrouter" => Some(Box::pin(openrouter::scrape_openrouter(config))),
        "kilo" => Some(Box::pin(kilo::scrape_kilo(config))),
        "aimlapi" => Some(Box::pin(aimlapi::scrape_aimlapi(config))),
        "novita" => Some(Box::pin(novita::scrape_novita(config))),
        "sambanova" => Some(Box::pin(sambanova::scrape_sambanova(config))),
        _ => None,
    }
}