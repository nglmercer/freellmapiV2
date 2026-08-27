//! Kilo models API client.

use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Read a numeric pricing field, coercing JSON strings.
fn price(v: Option<&serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Fetch models from Kilo's `/models` endpoint.
pub async fn fetch_models_from_kilo(provider: &ProviderConfig) -> Vec<Model> {
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
            if let Some(arr) = value.get("models").and_then(|d| d.as_array()) {
                for m in arr {
                    let id = m
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = m
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let provider_name = m
                        .get("provider")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| provider.name.clone());
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
                        gateway: Some(provider.name.clone()),
                        context_window: m
                            .get("context_window")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(4096),
                        supported_features: vec!["chat".into(), "completion".into()],
                        pricing,
                        url: Some(format!("https://kilo.ai/models/{id}")),
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
            tracing::error!("Failed to fetch models from Kilo: {e}");
            Vec::new()
        }
    }
}
