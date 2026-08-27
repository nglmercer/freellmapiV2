//! HuggingFace scraper, mirroring `getmodelsapi/src/scraper/providers/huggingface.ts`.
//!
//! Note: the TS module calls `configureRetry({retries: 3, ...})` at import
//! time; we apply the same global retry config here (idempotent).

use super::ScraperResult;
use crate::types::{Model, ProviderConfig};
use crate::utils::http::{configure_retry, get_json, HttpConfig};
use std::sync::OnceLock;
use std::time::Duration;

fn retry_configured() -> &'static OnceLock<()> {
    static CONFIGURED: OnceLock<()> = OnceLock::new();
    CONFIGURED.get_or_init(|| {
        configure_retry(3, true);
    });
    &CONFIGURED
}

/// Scrape the public HuggingFace models API.
pub async fn scrape_huggingface(config: ProviderConfig) -> ScraperResult {
    retry_configured();
    let url = "https://huggingface.co/api/models?full=true&limit=100";
    let http_config = HttpConfig {
        headers: vec![(
            "Authorization".into(),
            format!("Bearer {}", config.api_key.as_deref().unwrap_or("")),
        )],
        timeout: Duration::from_secs(10),
        ..Default::default()
    };

    match get_json(url, &http_config).await {
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
                        provider: "huggingface".into(),
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
            ScraperResult::ok(models)
        }
        Err(e) => {
            tracing::error!("Failed to scrape HuggingFace: {e}");
            ScraperResult::err(e.to_string())
        }
    }
}