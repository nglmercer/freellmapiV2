//! Static provider registry, mirroring `getmodelsapi/src/config/providers.ts`.

use crate::types::{ProviderConfig, ProviderType};

/// Read an env var the way `process.env.X` behaves in TS.
///
/// Unlike the TS version (which keeps `""`), we also drop empty strings so the
/// common `if (!provider.apiKey) return []` guards behave identically (an
/// empty string was falsy in JS too).
fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// The full `PROVIDERS` list. API keys are resolved from the environment at
/// call time (equivalent to the TS module reading `process.env` at load).
pub fn providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            name: "google".into(),
            type_: ProviderType::Provider,
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key: env("GOOGLE_API_KEY"),
            supports_scraping: true,
            priority: 1,
            free_tier: true,
        },
        ProviderConfig {
            name: "mistral".into(),
            type_: ProviderType::Provider,
            base_url: "https://api.mistral.ai/v1".into(),
            api_key: env("MISTRAL_API_KEY"),
            supports_scraping: true,
            priority: 2,
            free_tier: true,
        },
        ProviderConfig {
            name: "aimlapi".into(),
            type_: ProviderType::Gateway,
            base_url: "https://api.aimlapi.com/v1".into(),
            api_key: env("AIMLAPI_API_KEY"),
            supports_scraping: true,
            priority: 3,
            free_tier: false,
        },
        ProviderConfig {
            name: "novita".into(),
            type_: ProviderType::Gateway,
            base_url: "https://api.novita.ai/v3/openai".into(),
            api_key: env("NOVITA_API_KEY"),
            supports_scraping: true,
            priority: 4,
            free_tier: false,
        },
        ProviderConfig {
            name: "openrouter".into(),
            type_: ProviderType::Gateway,
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key: env("OPENROUTER_API_KEY"),
            supports_scraping: true,
            priority: 5,
            free_tier: false,
        },
        ProviderConfig {
            name: "sambanova".into(),
            type_: ProviderType::Provider,
            base_url: "https://api.sambanova.ai/v1".into(),
            api_key: env("SAMBANOVA_API_KEY"),
            supports_scraping: true,
            priority: 6,
            free_tier: false,
        },
        ProviderConfig {
            name: "together".into(),
            type_: ProviderType::Gateway,
            base_url: "https://api.together.xyz/v1".into(),
            api_key: env("TOGETHER_API_KEY"),
            supports_scraping: false,
            priority: 7,
            free_tier: true,
        },
        ProviderConfig {
            name: "cohere".into(),
            type_: ProviderType::Provider,
            base_url: "https://api.cohere.ai/v1".into(),
            api_key: env("COHERE_API_KEY"),
            supports_scraping: false,
            priority: 8,
            free_tier: true,
        },
        ProviderConfig {
            name: "kilo".into(),
            type_: ProviderType::Gateway,
            base_url: "https://kilo.ai".into(),
            api_key: env("KILO_API_KEY"),
            supports_scraping: true,
            priority: 9,
            free_tier: false,
        },
        ProviderConfig {
            name: "groq".into(),
            type_: ProviderType::Gateway,
            base_url: "https://api.groq.com/openai/v1".into(),
            api_key: env("GROQ_API_KEY"),
            supports_scraping: false,
            priority: 10,
            free_tier: false,
        },
        ProviderConfig {
            name: "huggingface".into(),
            type_: ProviderType::Provider,
            base_url: "https://huggingface.co".into(),
            api_key: env("HUGGINGFACE_API_KEY"),
            supports_scraping: true,
            priority: 11,
            free_tier: false,
        },
    ]
}