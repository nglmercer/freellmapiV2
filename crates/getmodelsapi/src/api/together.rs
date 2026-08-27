//! Together models API client.

use crate::types::{Model, Pricing, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Fetch models from Together's `/models` endpoint.
pub async fn fetch_models_from_together(provider: &ProviderConfig) -> Vec<Model> {
    let Some(api_key) = &provider.api_key else {
        return Vec::new();
    };

    let url = format!("{}/models", provider.base_url);
    let config = HttpConfig {
        headers: vec![("Authorization".into(), format!("Bearer {api_key}"))],
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(&url, &config).await {
        Ok(value) => {
            let mut models = Vec::new();
            if let Some(arr) = value.as_array() {
                for m in arr {
                    let id = m
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = m
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&id)
                        .to_string();
                    let pricing_json = m.get("pricing");
                    let has_pricing = pricing_json.map(|p| !p.is_null()).unwrap_or(false);
                    // A string "0" is not treated as numeric zero for the
                    // free-tier check, so retain the provider's value type.
                    let pricing = if has_pricing {
                        Some(Pricing {
                            prompt: numeric_or_zero(pricing_json.and_then(|p| p.get("input"))),
                            completion: numeric_or_zero(pricing_json.and_then(|p| p.get("output"))),
                        })
                    } else {
                        None
                    };
                    // Models without pricing or with numeric zero pricing are
                    // considered free-tier entries.
                    let free_tier = if has_pricing {
                        let input = pricing_json.and_then(|p| p.get("input"));
                        let output = pricing_json.and_then(|p| p.get("output"));
                        is_num_zero(input) && is_num_zero(output)
                    } else {
                        true
                    };
                    models.push(Model {
                        id: id.clone(),
                        name,
                        provider: "together".into(),
                        gateway: Some("together".into()),
                        context_window: m
                            .get("context_length")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(4096),
                        supported_features: vec!["chat".into(), "completion".into()],
                        pricing,
                        url: Some(format!("https://api.together.xyz/models/{id}")),
                        description: None,
                        free_tier: Some(free_tier),
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from Together: {e}");
            Vec::new()
        }
    }
}

fn numeric_or_zero(v: Option<&serde_json::Value>) -> f64 {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Only a JSON number zero counts; a string `"0"` remains distinct.
fn is_num_zero(v: Option<&serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64() == Some(0.0),
        _ => false,
    }
}
