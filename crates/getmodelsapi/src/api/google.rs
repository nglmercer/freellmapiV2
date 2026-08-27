//! Google Gemini models API mirroring `getmodelsapi/src/api/google.ts`.

use crate::types::{Model, ProviderConfig};
use crate::utils::http::{get_json, HttpConfig};
use std::time::Duration;

/// Fetch models from the Gemini API `/models` endpoint (needs an API key).
pub async fn fetch_models_from_google(provider: &ProviderConfig) -> Vec<Model> {
    let Some(api_key) = &provider.api_key else {
        return Vec::new();
    };

    let url = format!("{}/models", provider.base_url);
    let config = HttpConfig {
        params: vec![("key".into(), api_key.clone())],
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(&url, &config).await {
        Ok(value) => {
            let mut models = Vec::new();
            if let Some(arr) = value.get("models").and_then(|d| d.as_array()) {
                for m in arr {
                    let full_name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let id = full_name.replacen("models/", "", 1);
                    let name = m
                        .get("displayName")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(full_name)
                        .to_string();
                    let methods: Vec<String> = m
                        .get("supportedGenerationMethods")
                        .and_then(|e| e.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    // TS: m.supportedGenerationMethods?.includes("generateContent")
                    let supports_chat = methods.iter().any(|mt| mt == "generateContent");
                    models.push(Model {
                        id: id.clone(),
                        name,
                        provider: "google".into(),
                        gateway: Some("google".into()),
                        context_window: m
                            .get("inputTokenLimit")
                            .and_then(|v| v.as_i64())
                            .filter(|&n| n != 0)
                            .unwrap_or(32768),
                        supported_features: if supports_chat {
                            vec!["chat".into()]
                        } else {
                            vec!["completion".into()]
                        },
                        pricing: None,
                        url: Some(format!(
                            "https://ai.google.dev/gemini-api/docs/models#{id}"
                        )),
                        description: m
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        free_tier: Some(true),
                    });
                }
            }
            models
        }
        Err(e) => {
            tracing::error!("Failed to fetch models from Google: {e}");
            Vec::new()
        }
    }
}