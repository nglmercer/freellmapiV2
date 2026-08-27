//! Generic OpenAI-compatible provider implementation.

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;

use super::base::{normalize_choices, send_request, ChunkReceiver, Provider, ProviderError};
use crate::types::{ChatCompletionChunk, ChatCompletionResponse, ChatMessage, CompletionOptions};

/// Generic provider for platforms that use an OpenAI-compatible API.
/// Covers: Groq, Cerebras, SambaNova, NVIDIA NIM, Mistral, OpenRouter,
/// GitHub Models, Zhipu, Ollama Cloud, Kilo, Pollinations, LLM7, and any
/// custom provider.
pub struct OpenAICompatProvider {
    pub platform: String,
    pub provider_name: String,
    pub base_url: String,
    pub extra_headers: Vec<(String, String)>,
    pub validate_url: Option<String>,
    /// Per-provider HTTP timeout override. Cloud APIs finish in ~15s;
    /// locally-hosted inference (llama.cpp / vLLM on CPU) can take 30-120s
    /// for long prompts. Default 15000.
    pub timeout_ms: u64,
}

pub struct OpenAICompatOptions {
    pub platform: String,
    pub name: String,
    pub base_url: String,
    pub extra_headers: Vec<(String, String)>,
    pub validate_url: Option<String>,
    pub timeout_ms: u64,
}

impl OpenAICompatProvider {
    pub fn new(opts: OpenAICompatOptions) -> Self {
        Self {
            platform: opts.platform,
            provider_name: opts.name,
            base_url: opts.base_url,
            extra_headers: opts.extra_headers,
            validate_url: opts.validate_url,
            timeout_ms: opts.timeout_ms,
        }
    }

    fn body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        options: &CompletionOptions,
        stream: bool,
    ) -> serde_json::Value {
        // Send only the supported fields, with model and messages first;
        // absent options are omitted.
        let mut obj = serde_json::Map::new();
        obj.insert("model".to_string(), json!(model_id));
        obj.insert(
            "messages".to_string(),
            serde_json::to_value(messages).unwrap(),
        );
        macro_rules! opt_put {
            ($k:expr, $v:expr) => {
                if let Some(v) = $v {
                    obj.insert($k.to_string(), serde_json::to_value(v).unwrap());
                }
            };
        }
        opt_put!("temperature", options.temperature);
        opt_put!("max_tokens", options.max_tokens);
        opt_put!("top_p", options.top_p);
        opt_put!("seed", options.seed);
        opt_put!("frequency_penalty", options.frequency_penalty);
        opt_put!("presence_penalty", options.presence_penalty);
        opt_put!("user", options.user.as_ref());
        opt_put!("response_format", options.response_format.as_ref());
        opt_put!("tools", options.tools.as_ref());
        opt_put!("tool_choice", options.tool_choice.as_ref());
        opt_put!("parallel_tool_calls", options.parallel_tool_calls);
        opt_put!("logprobs", options.logprobs);
        opt_put!("top_logprobs", options.top_logprobs);
        opt_put!("stop", options.stop.as_ref());
        if stream {
            obj.insert("stream".to_string(), json!(true));
        }
        serde_json::Value::Object(obj)
    }

    async fn post(
        &self,
        api_key: &str,
        body: serde_json::Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let mut req = super::base::http_client()
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json");
        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = send_request(req.json(&body), Some(self.timeout_ms)).await?;
        Ok(res)
    }

    async fn error_from_response(res: reqwest::Response, name: &str) -> ProviderError {
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("HTTP {status}"));
        ProviderError::http(status, format!("{name} API error {status}: {message}"))
    }
}

#[async_trait]
impl Provider for OpenAICompatProvider {
    fn platform(&self) -> String {
        self.platform.clone()
    }

    fn name(&self) -> String {
        self.provider_name.clone()
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChatCompletionResponse, ProviderError> {
        let body = self.body(model_id, messages, options, false);
        let res = self.post(api_key, body).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res, &self.provider_name).await);
        }
        let mut data: ChatCompletionResponse = res
            .json()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))?;
        normalize_choices(&mut data);
        data._routed_via = Some(crate::types::RoutedVia {
            platform: self.platform.clone(),
            model: model_id.to_string(),
        });
        Ok(data)
    }

    async fn stream_chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChunkReceiver, ProviderError> {
        let body = self.body(model_id, messages, options, true);
        let res = self.post(api_key, body).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res, &self.provider_name).await);
        }

        let (tx, rx) = mpsc::channel::<Result<ChatCompletionChunk, ProviderError>>(64);
        let err_name = self.provider_name.clone();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = res.bytes_stream();
            // Buffer bytes (not chars) so multi-byte UTF-8 sequences split
            // across TCP chunks aren't corrupted during streaming decode.
            let mut buffer: Vec<u8> = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(Err(ProviderError::new(format!(
                                "{err_name} stream error: {e}"
                            ))))
                            .await;
                        return;
                    }
                };
                buffer.extend_from_slice(&chunk);
                // Split on '\n', keep trailing incomplete piece in buffer
                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = buffer.drain(..=pos).collect();
                    let line = line.trim_ascii();
                    if line.is_empty() || !line.starts_with(b"data: ") {
                        continue;
                    }
                    let data = &line[6..];
                    if data == b"[DONE]" {
                        return; // channel closes, generator returns
                    }
                    let text = String::from_utf8_lossy(data);
                    if let Ok(parsed) = serde_json::from_str::<ChatCompletionChunk>(&text) {
                        if tx.send(Ok(parsed)).await.is_err() {
                            return; // consumer dropped
                        }
                    }
                    // Skip malformed chunks rather than aborting the stream.
                }
            }
        });
        Ok(rx)
    }

    async fn validate_key(&self, api_key: &str) -> Result<bool, ProviderError> {
        // Note: transport errors (DNS / timeout / TLS) propagate to the caller.
        // health.rs catches them and marks status='error' WITHOUT incrementing
        // the consecutive-failure counter — only confirmed 401/403 disables a
        // key.
        let url = self
            .validate_url
            .clone()
            .unwrap_or_else(|| format!("{}/models", self.base_url));
        let mut req = super::base::http_client()
            .get(&url)
            .header("Authorization", format!("Bearer {api_key}"));
        for (k, v) in &self.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = send_request(req, Some(10000)).await?;
        Ok(res.status().as_u16() != 401 && res.status().as_u16() != 403)
    }
}
