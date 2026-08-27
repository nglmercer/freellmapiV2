//! Provider fetch dispatch table + OpenRouter / Ollama / HuggingFace fetchers,
//! mirroring `getmodelsapi/src/api/providers.ts`.

use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

use super::cohere::fetch_models_from_cohere;
use super::google::fetch_models_from_google;
use super::groq::fetch_models_from_groq;
use super::kilo::fetch_models_from_kilo;
use super::mistral::fetch_models_from_mistral;
use super::together::fetch_models_from_together;

/// Read a numeric field (numbers directly, strings coerced).
pub(crate) fn price(v: Option<&serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Fetch models from OpenRouter's `/models` endpoint (requires an API key).
pub async fn fetch_models_from_openrouter(provider: &ProviderConfig) -> Vec<Model> {
    let Some(api_key) = &provider.api_key else {
        return Vec::new();
    };

    let url = format!("{}/models", provider.base_url);
    let referer = std::env::var("HTTP_REFERER").unwrap_or_else(|_| "http://localhost:3000".into());
    let config = HttpConfig {
        headers: vec![
            ("Authorization".into(), format!("Bearer {api_key}")),
            ("HTTP-Referer".into(), referer),
            ("X-Title".into(), "GetModels API".into()),
        ],
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(&url, &config).await {
        Ok(value) => {
            let mut models = Vec::new();
            if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
                for m in arr {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let provider_name = m
                        .get("provider")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| provider.name.clone());
                    // TS: model.context_window || model.context_length (may be
                    // undefined in TS; we default to 0 in Rust).
                    // TS: model.context_window || model.context_length (falsy => next, then 0)
                    let context_window = m
                        .get("context_window")
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .or_else(|| {
                            m.get("context_length")
                                .and_then(|v| v.as_i64())
                                .filter(|&n| n != 0)
                        })
                        .unwrap_or(0);
                    let pricing = m.get("pricing").and_then(|p| {
                        if p.is_null() {
                            None
                        } else {
                            Some(Pricing {
                                prompt: price(p.get("prompt")),
                                completion: price(p.get("completion")),
                            })
                        }
                    });
                    models.push(Model {
                        id: id.clone(),
                        name,
                        provider: provider_name,
                        gateway: None,
                        context_window,
                        supported_features: vec!["chat".into(), "completion".into()],
                        pricing,
                        url: Some(format!("https://openrouter.ai/models/{id}")),
                        description: m
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        free_tier: None,
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from OpenRouter: {e}");
            Vec::new()
        }
    }
}

/// Fetch models from a local Ollama instance `/api/tags` (mirrors the TS fn).
pub async fn fetch_models_from_ollama(provider: &ProviderConfig) -> Vec<Model> {
    let url = format!("{}/api/tags", provider.base_url);
    let config = HttpConfig {
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(&url, &config).await {
        Ok(value) => {
            let mut models = Vec::new();
            if let Some(arr) = value.get("models").and_then(|d| d.as_array()) {
                for m in arr {
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    models.push(Model {
                        id: name.clone(),
                        name,
                        provider: provider.name.clone(),
                        gateway: None,
                        context_window: 4096,
                        supported_features: vec!["chat".into(), "completion".into()],
                        pricing: None,
                        url: Some(format!(
                            "http://localhost:11434/api/show/{}",
                            m.get("name").and_then(|v| v.as_str()).unwrap_or("")
                        )),
                        description: None,
                        free_tier: None,
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from Ollama: {e}");
            Vec::new()
        }
    }
}

/// Fetch models from the public HuggingFace models API (mirrors the TS fn).
pub async fn fetch_models_from_huggingface(provider: &ProviderConfig) -> Vec<Model> {
    let url = "https://huggingface.co/api/models?full=true&limit=100";
    let config = HttpConfig {
        headers: vec![(
            "Authorization".into(),
            format!("Bearer {}", provider.api_key.as_deref().unwrap_or("")),
        )],
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(url, &config).await {
        Ok(value) => {
            let mut models = Vec::new();
            if let Some(arr) = value.as_array() {
                for m in arr {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let config_obj = m.get("config");
                    let context_window = config_obj
                        .and_then(|c| c.get("max_position_embeddings"))
                        .and_then(|v| v.as_i64())
                        .filter(|&n| n != 0)
                        .unwrap_or(4096);
                    let supports_completion = config_obj
                        .and_then(|c| c.get("supports_prompt_completion_protocol"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    models.push(Model {
                        id: id.clone(),
                        name: id.clone(),
                        provider: provider.name.clone(),
                        gateway: None,
                        context_window,
                        supported_features: if supports_completion {
                            vec!["chat".into(), "completion".into()]
                        } else {
                            vec!["completion".into()]
                        },
                        pricing: None,
                        url: Some(format!("https://huggingface.co/{id}")),
                        description: m
                            .get("cardData")
                            .and_then(|d| d.get("description"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        free_tier: None,
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from HuggingFace: {e}");
            Vec::new()
        }
    }
}

/// Mirror of the TS `fetchByProvider` record. Returns `None` for providers that
/// have no API module (they fall through to their scraper).
pub async fn fetch_by_provider(provider: &str, config: &ProviderConfig) -> Option<Vec<Model>> {
    match provider {
        "openrouter" => Some(fetch_models_from_openrouter(config).await),
        "kilo" => Some(fetch_models_from_kilo(config).await),
        "groq" => Some(fetch_models_from_groq(config).await),
        "huggingface" => Some(fetch_models_from_huggingface(config).await),
        "google" => Some(fetch_models_from_google(config).await),
        "mistral" => Some(fetch_models_from_mistral(config).await),
        "together" => Some(fetch_models_from_together(config).await),
        "cohere" => Some(fetch_models_from_cohere(config).await),
        _ => None,
    }
}