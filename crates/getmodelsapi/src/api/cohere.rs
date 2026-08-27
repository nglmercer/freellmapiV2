//! Cohere models API mirroring `getmodelsapi/src/api/cohere.ts`.

use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Fetch models from Cohere's `/models` endpoint.
pub async fn fetch_models_from_cohere(provider: &ProviderConfig) -> Vec<Model> {
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
            if let Some(arr) = value.get("models").and_then(|d| d.as_array()) {
                for m in arr {
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let endpoints: Vec<String> = m
                        .get("endpoints")
                        .and_then(|e| e.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let supports_chat = endpoints.iter().any(|e| e == "chat");
                    models.push(Model {
                        id: name.clone(),
                        name: name.clone(),
                        provider: "cohere".into(),
                        gateway: Some("cohere".into()),
                        context_window: m
                            .get("context_length")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(4096),
                        supported_features: if supports_chat {
                            vec!["chat".into()]
                        } else {
                            vec!["completion".into()]
                        },
                        pricing: None,
                        url: Some(format!("https://docs.cohere.com/docs/models#{name}")),
                        description: None,
                        free_tier: Some(true),
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from Cohere: {e}");
            Vec::new()
        }
    }
}