//! Cloudflare Workers AI OpenAI-compatible provider.
//! API keys are `"account_id:api_token"`.

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use super::base::{http_client, send_request, ChunkReceiver, Provider, ProviderError};
use crate::types::{
    ChatCompletionChunk, ChatCompletionResponse, ChatMessage, CompletionOptions, MessageContent,
    RoutedVia,
};

const CHAT_URL: &str = "https://api.cloudflare.com/client/v4";
const TOKEN_VERIFY_URL: &str = "https://api.cloudflare.com/client/v4/user/tokens/verify";
const DEFAULT_TIMEOUT_MS: u64 = 15000;
const VALIDATE_TIMEOUT_MS: u64 = 10000;

#[derive(Default)]
pub struct CloudflareProvider;

impl CloudflareProvider {
    pub fn new() -> Self {
        Self
    }

    /// Parse the documented `account_id:api_token` key format.
    fn parse_key(api_key: &str) -> Result<(String, String), ProviderError> {
        match api_key.find(':') {
            Some(sep) => Ok((api_key[..sep].to_string(), api_key[sep + 1..].to_string())),
            None => Err(ProviderError::new(
                "Cloudflare key must be in format \"account_id:api_token\"",
            )),
        }
    }

    fn url(account_id: &str) -> String {
        format!("{CHAT_URL}/accounts/{account_id}/ai/v1/chat/completions")
    }

    /// `normalizeMessages` — Cloudflare rejects `content: null` on assistant
    /// messages that carry tool_calls, so null content becomes `''`.
    fn normalize_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
        messages
            .iter()
            .map(|m| {
                let mut c = m.clone();
                if matches!(c.content, MessageContent::Null(_)) {
                    c.content = MessageContent::Text(String::new());
                }
                c
            })
            .collect()
    }

    fn body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        options: &CompletionOptions,
        stream: bool,
    ) -> Value {
        // Cloudflare accepts parallel tool calls; omit options that were not
        // supplied by the caller.
        let normalized = Self::normalize_messages(messages);
        let mut obj = serde_json::Map::new();
        obj.insert("model".to_string(), serde_json::json!(model_id));
        obj.insert(
            "messages".to_string(),
            serde_json::to_value(normalized).unwrap(),
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
        if stream {
            obj.insert("stream".to_string(), serde_json::json!(true));
        }
        Value::Object(obj)
    }

    async fn post(
        &self,
        url: &str,
        token: &str,
        body: Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let req = http_client()
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body);
        send_request(req, Some(DEFAULT_TIMEOUT_MS)).await
    }

    /// `Cloudflare API error ${status}: ${err.error?.message ??
    /// err.errors?.[0]?.message ?? res.statusText}`
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
                    .or_else(|| {
                        v.get("errors")
                            .and_then(|e| e.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|first| first.get("message"))
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .unwrap_or(status_text);
        ProviderError::http(status, format!("Cloudflare API error {status}: {message}"))
    }
}

#[async_trait]
impl Provider for CloudflareProvider {
    fn platform(&self) -> String {
        "cloudflare".to_string()
    }

    fn name(&self) -> String {
        "Cloudflare Workers AI".to_string()
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChatCompletionResponse, ProviderError> {
        let (account_id, token) = Self::parse_key(api_key)?;
        let url = Self::url(&account_id);
        let body = self.body(model_id, messages, options, false);
        let res = self.post(&url, &token, body).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res).await);
        }
        let mut data: ChatCompletionResponse = res
            .json()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))?;
        data._routed_via = Some(RoutedVia {
            platform: "cloudflare".to_string(),
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
        let (account_id, token) = Self::parse_key(api_key)?;
        let url = Self::url(&account_id);
        let body = self.body(model_id, messages, options, true);
        let res = self.post(&url, &token, body).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res).await);
        }

        let (tx, rx) = mpsc::channel::<Result<ChatCompletionChunk, ProviderError>>(64);
        let err_name = self.name();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = res.bytes_stream();
            // Buffer bytes (not chars) so multi-byte UTF-8 sequences split
            // across TCP chunks aren't corrupted.
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
        // Only confirmed 401/403 (or an inactive token) disables a key; other
        // non-2xx responses keep the key enabled.
        let (_, token) = Self::parse_key(api_key)?;
        let res = send_request(
            http_client()
                .get(TOKEN_VERIFY_URL)
                .header("Authorization", format!("Bearer {token}")),
            Some(VALIDATE_TIMEOUT_MS),
        )
        .await?;
        let status = res.status().as_u16();
        if status == 401 || status == 403 {
            return Ok(false);
        }
        if !res.status().is_success() {
            return Ok(true);
        }
        let data: Value = res
            .json()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))?;
        let active = data
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && data
                .get("result")
                .and_then(|r| r.get("status"))
                .and_then(|s| s.as_str())
                .map(|s| s == "active")
                .unwrap_or(false);
        Ok(active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    #[test]
    fn parse_key_splits_on_first_colon() {
        assert_eq!(
            CloudflareProvider::parse_key("abc123:token123").unwrap(),
            ("abc123".to_string(), "token123".to_string())
        );
        // extra colons are part of the token
        assert_eq!(
            CloudflareProvider::parse_key("acc:x:y:z").unwrap(),
            ("acc".to_string(), "x:y:z".to_string())
        );
        let err = CloudflareProvider::parse_key("notoken").unwrap_err();
        assert_eq!(
            err.message,
            "Cloudflare key must be in format \"account_id:api_token\""
        );
        assert!(err.status.is_none());
    }

    #[test]
    fn url_builds_from_account_id() {
        assert_eq!(
            CloudflareProvider::url("acc"),
            "https://api.cloudflare.com/client/v4/accounts/acc/ai/v1/chat/completions"
        );
    }

    #[test]
    fn normalize_messages_replaces_null_content() {
        let mut null_msg = ChatMessage::text("assistant", "");
        null_msg.content = MessageContent::Null(());
        let text_msg = ChatMessage::text("assistant", "hello");
        let normalized = CloudflareProvider::normalize_messages(&[null_msg, text_msg]);
        assert_eq!(normalized[0].content.as_text(), Some(""));
        assert_eq!(normalized[1].content.as_text(), Some("hello"));
    }

    #[test]
    fn body_includes_parallel_tool_calls_and_normalized_messages() {
        let p = CloudflareProvider::new();
        let mut m = ChatMessage::text("assistant", "");
        m.content = MessageContent::Null(());
        let opts = CompletionOptions {
            parallel_tool_calls: Some(true),
            ..CompletionOptions::default()
        };
        let body = p.body("m", &[m], &opts, true);
        assert_eq!(body["model"].as_str(), Some("m"));
        assert_eq!(body["parallel_tool_calls"].as_bool(), Some(true));
        assert_eq!(body["stream"].as_bool(), Some(true));
        assert_eq!(body["messages"][0]["content"].as_str(), Some(""));
        assert!(body.get("stop").is_none());
    }

    #[test]
    fn parses_openai_compat_response() {
        let raw = r#"{
          "id": "chatcmpl-cf",
          "object": "chat.completion",
          "created": 1710000000,
          "model": "@cf/meta/llama-3.1-8b",
          "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "Hi from CF" },
            "finish_reason": "stop"
          }],
          "usage": { "prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4 }
        }"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(
            parsed.choices[0].message.content.as_text(),
            Some("Hi from CF")
        );
        assert_eq!(parsed.usage.prompt_tokens, 3);
    }
}
