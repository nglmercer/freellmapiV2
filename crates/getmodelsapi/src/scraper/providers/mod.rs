//! Provider-specific scrapers.

pub mod aimlapi;
pub mod factory;
pub mod google;
pub mod huggingface;
pub mod kilo;
pub mod mistral;
pub mod novita;
pub mod openrouter;
pub mod sambanova;

use crate::types::Model;
use serde::{Deserialize, Serialize};

/// Outcome of a single provider scrape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScraperResult {
    pub success: bool,
    pub models: Vec<Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ScraperResult {
    pub fn ok(models: Vec<Model>) -> Self {
        ScraperResult {
            success: true,
            models,
            error: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        ScraperResult {
            success: false,
            models: vec![],
            error: Some(error.into()),
        }
    }
}

/// Optional scraper settings reserved for provider-specific extensions.
#[derive(Debug, Clone, Default)]
pub struct ScraperOptions {
    pub timeout: Option<std::time::Duration>,
    pub user_agent: Option<String>,
}
