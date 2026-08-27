//! Port of `server/src/providers/cohere.ts` — OpenAI-compatible via the
//! Cohere `/compatibility/v1` endpoint.

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use super::base::{http_client, send_request, ChunkReceiver, Provider, ProviderError};
use crate::types::{
    ChatCompletionChunk, ChatCompletionResponse, ChatMessage, CompletionOptions, RoutedVia,
};

const API_BASE: &str = "https://api.cohere.ai/compatibility/v1";
const DEFAULT_TIMEOUT_MS: u64 = 15000;
const VALIDATE_TIMEOUT_MS: u64 = 10000;

pub struct CohereProvider;

impl CohereProvider {
    pub fn new() -> Self {
        Self
    }

    fn body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        options: &CompletionOptions,
        stream: bool,
    ) -> Value {
        // Mirrors the TS body object: undefined options are dropped.
        let mut obj = serde_json::Map::new();
        obj.insert("model".to_string(), serde_json::json!(model_id));
        obj.insert("messages".to_string(), serde_json::to_value(messages).unwrap());
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
        opt_put!("logprobs", options.logprobs);
        opt_put!("top_logprobs", options.top_logprobs);
        if stream {
            obj.insert("stream".to_string(), serde_json::json!(true));
        }
        Value::Object(obj)
    }

    async fn post(
        &self,
        api_key: &str,
        body: Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let req = http_client()
            .post(format!("{API_BASE}/chat/completions"))
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body);
        send_request(req, Some(DEFAULT_TIMEOUT_MS)).await
    }

    /// `Cohere API error ${status}: ${err.error?.message ?? res.statusText}`
    async fn error_from_response(res: reqwest::Response) -> ProviderError {
        let status = res.status().as_u16();
        let status_text = res.status().canonical_reason().unwrap_or("").to_string();
        let text = res.text().await.unwrap_or_default();
        let message = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or(status_text);
        ProviderError::http(status, format!("Cohere API error {status}: {message}"))
    }
}

#[async_trait]
impl Provider for CohereProvider {
    fn platform(&self) -> String {
        "cohere".to_string()
    }

    fn name(&self) -> String {
        "Cohere".to_string()
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
            return Err(Self::error_from_response(res).await);
        }
        let mut data: ChatCompletionResponse = res
            .json()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))?;
        data._routed_via = Some(RoutedVia {
            platform: "cohere".to_string(),
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
            return Err(Self::error_from_response(res).await);
        }

        let (tx, rx) = mpsc::channel::<Result<ChatCompletionChunk, ProviderError>>(64);
        let err_name = self.name();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = res.bytes_stream();
            // Buffer bytes (not chars) so multi-byte UTF-8 sequences split
            // across TCP chunks aren't corrupted — mirrors TS TextDecoder.
            let mut buffer: Vec<u8> = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(Err(ProviderError::new(format!("{err_name} stream error: {e}"))))
                            .await;
                        return;
                    }
                };
                buffer.extend_from_slice(&chunk);
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
                    // Skip malformed chunks, like the TS try/catch.
                }
            }
        });
        Ok(rx)
    }

    async fn validate_key(&self, api_key: &str) -> Result<bool, ProviderError> {
        // Transport errors propagate — health checks mark 'error' without
        // counting toward auto-disable; only confirmed 401/403 disables a key.
        let res = send_request(
            http_client()
                .get(format!("{API_BASE}/models"))
                .header("Authorization", format!("Bearer {api_key}")),
            Some(VALIDATE_TIMEOUT_MS),
        )
        .await?;
        Ok(res.status().as_u16() != 401 && res.status().as_u16() != 403)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    #[test]
    fn body_includes_only_present_options() {
        let p = CohereProvider::new();
        let opts = CompletionOptions {
            temperature: Some(0.7),
            max_tokens: Some(50),
            user: Some("u1".into()),
            ..CompletionOptions::default()
        };
        let body = p.body("command-r", &[ChatMessage::text("user", "hi")], &opts, false);
        assert_eq!(body["model"].as_str(), Some("command-r"));
        assert_eq!(body["temperature"].as_f64(), Some(0.7));
        assert_eq!(body["max_tokens"].as_i64(), Some(50));
        assert_eq!(body["user"].as_str(), Some("u1"));
        assert_eq!(body["messages"][0]["role"].as_str(), Some("user"));
        assert_eq!(body["messages"][0]["content"].as_str(), Some("hi"));
        assert!(body.get("seed").is_none());
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn body_stream_flag_added() {
        let p = CohereProvider::new();
        let body = p.body("m", &[], &CompletionOptions::default(), true);
        assert_eq!(body["stream"].as_bool(), Some(true));
    }

    #[test]
    fn parses_openai_compat_response() {
        let raw = r#"{
          "id": "chatcmpl-abc",
          "object": "chat.completion",
          "created": 1710000000,
          "model": "command-r",
          "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Hi there" },
            "finish_reason": "stop"
          }],
          "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        }"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.choices[0].message.content.as_text(), Some("Hi there"));
        assert_eq!(parsed.usage.total_tokens, 7);
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
    }
}