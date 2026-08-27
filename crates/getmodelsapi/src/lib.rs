//! Free-tier LLM model discovery (Rust port of the TS `getmodelsapi` package).
//!
//! Discovers models across providers via direct API calls and public-catalog
//! scrapers, with a two-level (in-memory + disk) cache.

pub mod api;
pub mod config;
pub mod scraper;
pub mod types;
pub mod utils;

pub use api::{clear_cache, get_models, get_providers};
pub use types::{GetModelsOptions, Model, Pricing, ProviderConfig, ProviderType};
pub use utils::enrich::{enrich_model, enrich_models};