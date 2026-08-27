//! Mistral models API mirroring `getmodelsapi/src/api/mistral.ts`.

use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Fetch models from Mistral's `/models` endpoint.
pub async fn fetch_models_from_mistral(provider: &ProviderConfig) -> Vec<Model> {
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
            if let Some(arr) = value.get("data").and_then(|d| d.as_array()) {
                for m in arr {
                    let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owned_by = m
                        .get("owned_by")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    models.push(Model {
                        id: id.clone(),
                        name: id.clone(),
                        provider: "mistral".into(),
                        gateway: Some("mistral".into()),
                        context_window: m
                            .get("max_context_length")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(32768),
                        supported_features: vec!["chat".into(), "completion".into()],
                        pricing: None,
                        url: Some(format!(
                            "https://docs.mistral.ai/getting-started/models/#{id}"
                        )),
                        description: owned_by.map(|o| format!("Owned by {o}")),
                        free_tier: Some(true),
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from Mistral: {e}");
            Vec::new()
        }
    }
}