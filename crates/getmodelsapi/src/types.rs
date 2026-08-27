//! Shared model-discovery types.
//!
//! All structs/enums serialize to (and deserialize from) the same JSON wire
//! format (camelCase field names, optional fields
//! omitted when absent).

use serde::{Deserialize, Serialize};

/// Pricing in USD per one million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Pricing {
    pub prompt: f64,
    pub completion: f64,
}

/// A discovered LLM model. Field names are camelCase on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    #[serde(rename = "contextWindow")]
    pub context_window: i64,
    #[serde(rename = "supportedFeatures", default)]
    pub supported_features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<Pricing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "freeTier", skip_serializing_if = "Option::is_none")]
    pub free_tier: Option<bool>,
}

/// Whether a `ProviderConfig` is a first-party provider or an aggregation gateway.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProviderType {
    #[serde(rename = "provider")]
    Provider,
    #[serde(rename = "gateway")]
    Gateway,
}

/// A registered provider/gateway configuration.
///
/// `api_key` is never serialized in provider listings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ProviderType,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(rename = "apiKey", skip_serializing)]
    pub api_key: Option<String>,
    #[serde(rename = "supportsScraping")]
    pub supports_scraping: bool,
    pub priority: i64,
    #[serde(rename = "freeTier")]
    pub free_tier: bool,
}

/// Options for [`crate::get_models`].
///
/// The defaults for `limit`/`offset` (100 / 0) are applied inside
/// `get_models`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetModelsOptions {
    pub provider: Option<String>,
    pub gateway: Option<String>,
    pub search: Option<String>,
    pub free: Option<bool>,
    pub exclude: Vec<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Subset of `GetModelsOptions` used as filter params per provider fetch.
///
/// Used to build the exact `JSON.stringify`-compatible cache keys.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct FetchParams<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}
