//! Port of `server/src/providers/google.ts` — Gemini native API (v1beta).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use tokio::sync::mpsc;

use super::base::{http_client, make_id, send_request, ChunkReceiver, Provider, ProviderError};
use crate::types::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionChoice, ChatCompletionDelta,
    ChatCompletionResponse, ChatMessage, ChatToolCall, ChatToolCallFunction, ChatToolChoice,
    ChatToolDefinition, CompletionOptions, MessageContent, RoutedVia, TokenUsage,
};

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
const DEFAULT_TIMEOUT_MS: u64 = 15000;
const VALIDATE_TIMEOUT_MS: u64 = 10000;

// ---- Gemini wire types (response side) ----

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiCandidateContent>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
struct GeminiCandidateContent {
    #[serde(default)]
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thought_signature: Option<String>,
    #[serde(default)]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    args: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    #[serde(default)]
    prompt_token_count: i64,
    #[serde(default)]
    candidates_token_count: i64,
    #[serde(default)]
    total_token_count: i64,
}

// ---- Translation helpers (mirrors the TS module functions) ----

/// `safeParseObject(raw)`: try JSON.parse; object → itself, otherwise wrapped
/// in `{ value }`; parse failure returns `{ value: raw }`.
fn safe_parse_object(raw: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
        Ok(other) => serde_json::json!({ "value": other }),
        Err(_) => serde_json::json!({ "value": raw }),
    }
}

/// `normalizeGeminiArgs(args)`: string passthrough, else JSON.stringify with
/// null/undefined collapsed to `{}`.
fn normalize_gemini_args(args: Option<&serde_json::Value>) -> String {
    match args {
        None => "{}".to_string(),
        Some(v) if v.is_string() => v.as_str().unwrap().to_string(),
        Some(v) if v.is_null() => "{}".to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// `toGeminiFinishReason(finishReason)`.
fn to_gemini_finish_reason(finish_reason: Option<&str>) -> &'static str {
    let r = finish_reason.unwrap_or("").to_ascii_uppercase();
    match r.as_str() {
        "" => "stop",
        "MAX_TOKENS" => "length",
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            "content_filter"
        }
        _ => "stop",
    }
}

/// `encodeURIComponent` — leaves `A-Za-z0-9-_.!~*'()` unescaped, uppercases
/// percent-escapes.
fn encode_uri_component(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len() * 3);
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 0x0f)] as char);
        }
    }
    out
}

/// `toGeminiTools(tools)` → `[{ functionDeclarations: [...] }]`, omitting the
/// optional description/parameters (TS drops undefined keys).
fn to_gemini_tools(tools: Option<&Vec<ChatToolDefinition>>) -> Option<serde_json::Value> {
    let tools = tools?;
    if tools.is_empty() {
        return None;
    }
    let mut declarations: Vec<serde_json::Value> = Vec::with_capacity(tools.len());
    for t in tools {
        let mut decl = serde_json::Map::new();
        decl.insert("name".to_string(), serde_json::json!(t.function.name));
        if let Some(desc) = &t.function.description {
            decl.insert("description".to_string(), serde_json::json!(desc));
        }
        if let Some(params) = &t.function.parameters {
            decl.insert("parameters".to_string(), params.clone());
        }
        declarations.push(serde_json::Value::Object(decl));
    }
    Some(serde_json::json!([{ "functionDeclarations": declarations }]))
}

/// `toGeminiToolConfig(toolChoice)`.
fn to_gemini_tool_config(tool_choice: Option<&ChatToolChoice>) -> Option<serde_json::Value> {
    let tc = tool_choice?;
    match tc {
        ChatToolChoice::Mode(mode) => {
            let m = match mode.as_str() {
                "none" => "NONE",
                "required" => "ANY",
                _ => "AUTO",
            };
            Some(serde_json::json!({ "functionCallingConfig": { "mode": m } }))
        }
        ChatToolChoice::Named { function, .. } => Some(serde_json::json!({
            "functionCallingConfig": { "mode": "ANY", "allowedFunctionNames": [function.name] }
        })),
    }
}

/// `contentToGeminiParts(content)`.
fn content_to_gemini_parts(content: &MessageContent) -> Vec<serde_json::Value> {
    match content {
        MessageContent::Text(s) => vec![serde_json::json!({ "text": s })],
        MessageContent::Parts(parts) => {
            let mut out: Vec<serde_json::Value> = Vec::new();
            for part in parts {
                if part.r#type == "text" {
                    if let Some(text) = &part.text {
                        out.push(serde_json::json!({ "text": text }));
                    }
                } else if part.r#type == "image_url" {
                    if let Some(image_url) = &part.image_url {
                        let url = &image_url.url;
                        if url.starts_with("data:") {
                            let (header, data) = match url.find(',') {
                                Some(comma) => (&url[..comma], &url[comma + 1..]),
                                // JS `url.slice(0, -1)` on a comma-less data URL
                                None => (
                                    &url[..url.len().saturating_sub(1)],
                                    url.as_str(),
                                ),
                            };
                            out.push(serde_json::json!({
                                "inlineData": { "mimeType": data_url_mime(header), "data": data }
                            }));
                        } else {
                            out.push(serde_json::json!({
                                "fileData": { "mimeType": guess_mime_type(url), "fileUri": url }
                            }));
                        }
                    }
                }
            }
            if out.is_empty() {
                out.push(serde_json::json!({ "text": "" }));
            }
            out
        }
        MessageContent::Null(_) => vec![serde_json::json!({ "text": "" })],
    }
}

/// `header.match(/data:(image\/\w+);base64/)` group 1, else 'image/jpeg'.
fn data_url_mime(header: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"data:(image/\w+);base64").unwrap());
    re.captures(header)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "image/jpeg".to_string())
}

/// `guessMimeType(url)` — naive last-dot-segment lookup, defaults to png.
fn guess_mime_type(url: &str) -> &'static str {
    let ext = url.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

/// Assistant → `role: 'model'` with text + functionCall parts (or None when
/// nothing usable, matching the TS `.filter(entry => entry !== null)`).
fn gemini_assistant_entry(m: &ChatMessage) -> Option<serde_json::Value> {
    let mut parts: Vec<serde_json::Value> = Vec::new();
    if let Some(text) = m.content.as_text() {
        if !text.is_empty() {
            parts.push(serde_json::json!({ "text": text }));
        }
    }
    for call in m.tool_calls.iter().flatten() {
        let mut fc = serde_json::Map::new();
        if !call.id.is_empty() {
            fc.insert("id".to_string(), serde_json::json!(call.id));
        }
        fc.insert("name".to_string(), serde_json::json!(call.function.name));
        fc.insert(
            "args".to_string(),
            safe_parse_object(&call.function.arguments),
        );
        let mut part = serde_json::Map::new();
        if let Some(ts) = &call.thought_signature {
            part.insert("thoughtSignature".to_string(), serde_json::json!(ts));
        }
        part.insert("functionCall".to_string(), serde_json::Value::Object(fc));
        parts.push(serde_json::Value::Object(part));
    }
    if parts.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "role": "model", "parts": parts }))
}

/// Tool result → `role: 'user'` with a functionResponse part.
fn gemini_tool_entry(
    m: &ChatMessage,
    tool_name_by_call_id: &HashMap<String, String>,
) -> Option<serde_json::Value> {
    let tool_call_id = m.tool_call_id.as_deref().filter(|s| !s.is_empty())?;
    let tool_name = m
        .name
        .clone()
        .or_else(|| tool_name_by_call_id.get(tool_call_id).cloned())
        .unwrap_or_else(|| "tool".to_string());
    let content_str = m.content.as_text().map(|s| s.to_string()).unwrap_or_default();
    let response = safe_parse_object(&content_str);
    Some(serde_json::json!({
        "role": "user",
        "parts": [{
            "functionResponse": {
                "id": tool_call_id,
                "name": tool_name,
                "response": response,
            }
        }]
    }))
}

/// One message → a Gemini `{ role, parts }` entry, or None to drop it.
fn gemini_content_entry(
    m: &ChatMessage,
    tool_name_by_call_id: &HashMap<String, String>,
) -> Option<serde_json::Value> {
    match m.role.as_str() {
        "assistant" => gemini_assistant_entry(m),
        "tool" => gemini_tool_entry(m, tool_name_by_call_id),
        _ => Some(serde_json::json!({
            "role": "user",
            "parts": content_to_gemini_parts(&m.content),
        })),
    }
}

/// `toGeminiContents(messages)` → `(contents, systemInstruction)`.
fn to_gemini_contents(
    messages: &[ChatMessage],
) -> (Vec<serde_json::Value>, Option<serde_json::Value>) {
    // System messages must be plain strings; joined with blank lines.
    let system_parts: Vec<String> = messages
        .iter()
        .filter(|m| m.role == "system")
        .filter_map(|m| m.content.as_text())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(serde_json::json!({ "parts": [{ "text": system_parts.join("\n\n") }] }))
    };

    // tool call id → function name, from every message's tool_calls.
    let mut tool_name_by_call_id: HashMap<String, String> = HashMap::new();
    for m in messages {
        for tc in m.tool_calls.iter().flatten() {
            tool_name_by_call_id.insert(tc.id.clone(), tc.function.name.clone());
        }
    }

    let contents: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .filter_map(|m| gemini_content_entry(m, &tool_name_by_call_id))
        .collect();

    (contents, system_instruction)
}

/// `extractText(parts)` from a candidate — joins text parts, null if empty.
fn extract_text(parts: Option<&[GeminiPart]>) -> Option<String> {
    let parts = parts?;
    let mut text = String::new();
    for p in parts {
        if let Some(t) = &p.text {
            text.push_str(t);
        }
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// `extractToolCalls(parts)` from a candidate.
fn extract_tool_calls(parts: Option<&[GeminiPart]>) -> Vec<ChatToolCall> {
    let mut calls: Vec<ChatToolCall> = Vec::new();
    let Some(parts) = parts else {
        return calls;
    };
    let mut fallback_index: usize = 0;
    for part in parts {
        let Some(fc) = &part.function_call else { continue };
        let Some(name) = fc.name.as_deref().filter(|n| !n.is_empty()) else {
            continue;
        };
        let id = match &fc.id {
            Some(id) => id.clone(),
            None => {
                let now = chrono::Utc::now().timestamp_millis();
                let idx = fallback_index;
                fallback_index += 1;
                format!("call_{now}_{idx}")
            }
        };
        let arguments = normalize_gemini_args(fc.args.as_ref());
        calls.push(ChatToolCall {
            id,
            r#type: "function".to_string(),
            function: ChatToolCallFunction {
                name: name.to_string(),
                arguments,
                extra: serde_json::Map::new(),
            },
            thought_signature: part.thought_signature.clone(),
            index: None,
            extra: serde_json::Map::new(),
        });
    }
    calls
}

/// Streaming dedup: `key = id:name:arguments`, first occurrence wins.
fn dedup_tool_calls(
    calls: Vec<ChatToolCall>,
    seen: &mut HashSet<String>,
) -> Vec<ChatToolCall> {
    calls
        .into_iter()
        .filter(|call| {
            let key = format!(
                "{}:{}:{}",
                call.id, call.function.name, call.function.arguments
            );
            seen.insert(key)
        })
        .collect()
}

/// The full Gemini request body for both `generateContent` and the SSE
/// streaming variant (they share one object literal in the TS source).
fn gemini_request_body(
    messages: &[ChatMessage],
    options: &CompletionOptions,
) -> serde_json::Value {
    let (contents, system_instruction) = to_gemini_contents(messages);

    let mut generation_config = serde_json::Map::new();
    if let Some(t) = options.temperature {
        generation_config.insert("temperature".to_string(), serde_json::json!(t));
    }
    if let Some(m) = options.max_tokens {
        generation_config.insert("maxOutputTokens".to_string(), serde_json::json!(m));
    }
    if let Some(p) = options.top_p {
        generation_config.insert("topP".to_string(), serde_json::json!(p));
    }
    if let Some(s) = options.seed {
        generation_config.insert("seed".to_string(), serde_json::json!(s));
    }
    if let Some(f) = options.frequency_penalty {
        generation_config.insert("frequencyPenalty".to_string(), serde_json::json!(f));
    }
    if let Some(p) = options.presence_penalty {
        generation_config.insert("presencePenalty".to_string(), serde_json::json!(p));
    }

    let mut body = serde_json::Map::new();
    body.insert("contents".to_string(), serde_json::Value::Array(contents));
    body.insert(
        "generationConfig".to_string(),
        serde_json::Value::Object(generation_config),
    );
    if let Some(tools) = to_gemini_tools(options.tools.as_ref()) {
        body.insert("tools".to_string(), tools);
    }
    if let Some(cfg) = to_gemini_tool_config(options.tool_choice.as_ref()) {
        body.insert("toolConfig".to_string(), cfg);
    }
    if let Some(si) = system_instruction {
        body.insert("systemInstruction".to_string(), si);
    }
    if let Some(rf) = &options.response_format {
        if rf.r#type == "json_object" || rf.r#type == "json_schema" {
            let schema = rf
                .json_schema
                .clone()
                .unwrap_or_else(|| serde_json::json!({ "type": "OBJECT" }));
            body.insert("responseSchema".to_string(), schema);
        }
    }
    serde_json::Value::Object(body)
}

/// Convert a parsed Gemini response into the OpenAI-compatible response shape
/// (`id`/`created` are passed in to make the conversion testable offline).
fn gemini_to_openai(
    data: &GeminiResponse,
    id: String,
    created: i64,
    model_id: &str,
) -> ChatCompletionResponse {
    let candidate = data.candidates.as_ref().and_then(|c| c.first());
    let parts = candidate
        .and_then(|c| c.content.as_ref())
        .and_then(|c| c.parts.as_deref());
    let tool_calls = extract_tool_calls(parts);
    let text = extract_text(parts);

    let usage = TokenUsage {
        prompt_tokens: data
            .usage_metadata
            .as_ref()
            .map(|u| u.prompt_token_count)
            .unwrap_or(0),
        completion_tokens: data
            .usage_metadata
            .as_ref()
            .map(|u| u.candidates_token_count)
            .unwrap_or(0),
        total_tokens: data
            .usage_metadata
            .as_ref()
            .map(|u| u.total_token_count)
            .unwrap_or(0),
    };

    let content = match text {
        Some(t) => MessageContent::Text(t),
        None => MessageContent::Null(()),
    };

    let finish_reason = if tool_calls.is_empty() {
        to_gemini_finish_reason(candidate.and_then(|c| c.finish_reason.as_deref())).to_string()
    } else {
        "tool_calls".to_string()
    };

    ChatCompletionResponse {
        id,
        object: "chat.completion".to_string(),
        created,
        model: model_id.to_string(),
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content,
                name: None,
                tool_call_id: None,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                extra: serde_json::Map::new(),
            },
            finish_reason: Some(finish_reason),
            logprobs: None,
            extra: serde_json::Map::new(),
        }],
        usage,
        _routed_via: Some(RoutedVia {
            platform: "google".to_string(),
            model: model_id.to_string(),
        }),
        extra: serde_json::Map::new(),
    }
}

/// Build an upstream chunk with `created = now` (like the yield sites in TS).
fn google_chunk(
    id: &str,
    model: &str,
    delta: ChatCompletionDelta,
    finish_reason: Option<&str>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChatCompletionChunkChoice {
            index: 0,
            delta,
            finish_reason: finish_reason.map(|s| s.to_string()),
            logprobs: None,
            extra: serde_json::Map::new(),
        }],
        usage: None,
        extra: serde_json::Map::new(),
    }
}

#[derive(Default)]
pub struct GoogleProvider;

impl GoogleProvider {
    pub fn new() -> Self {
        Self
    }

    async fn generate(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
        stream: bool,
    ) -> Result<reqwest::Response, ProviderError> {
        let body = gemini_request_body(messages, options);
        let url = if stream {
            format!(
                "{API_BASE}/models/{}:streamGenerateContent?alt=sse",
                encode_uri_component(model_id)
            )
        } else {
            format!("{API_BASE}/models/{}:generateContent", encode_uri_component(model_id))
        };
        let req = http_client()
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body);
        send_request(req, Some(DEFAULT_TIMEOUT_MS)).await
    }

    /// `Google API error ${status}: ${err.error?.message ?? res.statusText}`
    async fn error_from_response(res: reqwest::Response) -> ProviderError {
        let status = res.status().as_u16();
        let status_text = res.status().canonical_reason().unwrap_or("").to_string();
        let text = res.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or(status_text);
        ProviderError::http(status, format!("Google API error {status}: {message}"))
    }
}

#[async_trait]
impl Provider for GoogleProvider {
    fn platform(&self) -> String {
        "google".to_string()
    }

    fn name(&self) -> String {
        "Google AI Studio".to_string()
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChatCompletionResponse, ProviderError> {
        let res = self.generate(api_key, messages, model_id, options, false).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res).await);
        }
        let data: GeminiResponse = res
            .json()
            .await
            .map_err(|e| ProviderError::new(format!("Failed to parse response: {e}")))?;
        let id = make_id();
        let created = chrono::Utc::now().timestamp();
        Ok(gemini_to_openai(&data, id, created, model_id))
    }

    async fn stream_chat_completion(
        &self,
        api_key: &str,
        messages: &[ChatMessage],
        model_id: &str,
        options: &CompletionOptions,
    ) -> Result<ChunkReceiver, ProviderError> {
        let res = self.generate(api_key, messages, model_id, options, true).await?;
        if !res.status().is_success() {
            return Err(Self::error_from_response(res).await);
        }

        let (tx, rx) = mpsc::channel::<Result<ChatCompletionChunk, ProviderError>>(64);
        let err_name = self.name();
        let model = model_id.to_string();
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = res.bytes_stream();
            // Buffer bytes, split on '\n'; keeps multi-byte UTF-8 sequences
            // split across TCP chunks intact (mirrors TS TextDecoder).
            let mut buffer: Vec<u8> = Vec::new();
            let id = make_id();
            let emitted_finish = false;
            let mut saw_tool_calls = false;
            let mut seen_tool_call_keys: HashSet<String> = HashSet::new();

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
                    let raw = &line[6..];
                    if raw == b"[DONE]" {
                        if !emitted_finish {
                            let finish = if saw_tool_calls { "tool_calls" } else { "stop" };
                            let out = google_chunk(&id, &model, ChatCompletionDelta::default(), Some(finish));
                            if tx.send(Ok(out)).await.is_err() {
                                return;
                            }
                        }
                        return;
                    }

                    // Skip malformed SSE frames instead of aborting the stream
                    // (matches the defensive parse in the TS generators).
                    let text = String::from_utf8_lossy(raw);
                    let parsed: GeminiResponse = match serde_json::from_str(&text) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    let candidate = parsed.candidates.as_ref().and_then(|c| c.first());
                    let parts = candidate
                        .and_then(|c| c.content.as_ref())
                        .and_then(|c| c.parts.as_deref());

                    let text_part = extract_text(parts);
                    let tool_calls =
                        dedup_tool_calls(extract_tool_calls(parts), &mut seen_tool_call_keys);

                    if text_part.is_some() || !tool_calls.is_empty() {
                        if !tool_calls.is_empty() {
                            saw_tool_calls = true;
                        }
                        let mut delta = ChatCompletionDelta::default();
                        if let Some(t) = &text_part {
                            delta.content = Some(t.clone());
                        }
                        if !tool_calls.is_empty() {
                            delta.tool_calls = Some(tool_calls);
                        }
                        let out = google_chunk(&id, &model, delta, None);
                        if tx.send(Ok(out)).await.is_err() {
                            return;
                        }
                    }

                    let reason = candidate
                        .and_then(|c| c.finish_reason.as_deref())
                        .filter(|s| !s.is_empty());
                    if let Some(reason) = reason {
                        if !emitted_finish {
                            let finish = if saw_tool_calls {
                                "tool_calls"
                            } else {
                                to_gemini_finish_reason(Some(reason))
                            };
                            let out = google_chunk(&id, &model, ChatCompletionDelta::default(), Some(finish));
                            if tx.send(Ok(out)).await.is_err() {
                                return;
                            }
                            return;
                        }
                    }
                }
            }

            if !emitted_finish {
                let finish = if saw_tool_calls { "tool_calls" } else { "stop" };
                let out = google_chunk(&id, &model, ChatCompletionDelta::default(), Some(finish));
                let _ = tx.send(Ok(out)).await;
            }
        });
        Ok(rx)
    }

    async fn validate_key(&self, api_key: &str) -> Result<bool, ProviderError> {
        // Transport errors propagate — health checks treat them as 'error'
        // without counting toward auto-disable. Only 401/403 disables a key.
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
    use serde_json::{json, Value};
    use crate::types::{
        ChatToolCall, ChatToolCallFunction, ChatToolChoice, ChatToolChoiceFunction,
        ChatToolDefinition, ChatToolFunctionDefinition, ContentPart, ImageUrl, ResponseFormat,
    };

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage::text(role, content)
    }

    fn text_part(s: &str) -> ContentPart {
        ContentPart {
            r#type: "text".to_string(),
            text: Some(s.to_string()),
            image_url: None,
            extra: serde_json::Map::new(),
        }
    }

    fn image_part(url: &str) -> ContentPart {
        ContentPart {
            r#type: "image_url".to_string(),
            text: None,
            image_url: Some(ImageUrl {
                url: url.to_string(),
                detail: None,
            }),
            extra: serde_json::Map::new(),
        }
    }

    fn tool_call(id: &str, name: &str, args: &str) -> ChatToolCall {
        ChatToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: ChatToolCallFunction {
                name: name.to_string(),
                arguments: args.to_string(),
                extra: serde_json::Map::new(),
            },
            thought_signature: None,
            index: None,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn safe_parse_object_handles_objects_arrays_scalars_and_invalid() {
        assert_eq!(safe_parse_object(r#"{"x":1}"#), json!({"x": 1}));
        assert_eq!(safe_parse_object("42"), json!({"value": 42}));
        assert_eq!(safe_parse_object(r#""hi""#), json!({"value": "hi"}));
        assert_eq!(safe_parse_object("not json"), json!({"value": "not json"}));
        assert_eq!(safe_parse_object(""), json!({"value": ""}));
        assert_eq!(safe_parse_object("null"), json!({"value": null}));
    }

    #[test]
    fn normalize_gemini_args_variants() {
        assert_eq!(normalize_gemini_args(None), "{}");
        assert_eq!(normalize_gemini_args(Some(&Value::Null)), "{}");
        assert_eq!(normalize_gemini_args(Some(&json!("abc"))), "abc");
        assert_eq!(normalize_gemini_args(Some(&json!({"a": 1}))), r#"{"a":1}"#);
    }

    #[test]
    fn finish_reason_mapping() {
        assert_eq!(to_gemini_finish_reason(None), "stop");
        assert_eq!(to_gemini_finish_reason(Some("STOP")), "stop");
        assert_eq!(to_gemini_finish_reason(Some("MAX_TOKENS")), "length");
        assert_eq!(to_gemini_finish_reason(Some("safety")), "content_filter");
        assert_eq!(to_gemini_finish_reason(Some("RECITATION")), "content_filter");
        assert_eq!(to_gemini_finish_reason(Some("BLOCKLIST")), "content_filter");
        assert_eq!(to_gemini_finish_reason(Some("PROHIBITED_CONTENT")), "content_filter");
        assert_eq!(to_gemini_finish_reason(Some("SPII")), "content_filter");
        assert_eq!(to_gemini_finish_reason(Some("OTHER")), "stop");
    }

    #[test]
    fn encode_uri_component_matches_ts() {
        assert_eq!(encode_uri_component("gemini-2.0-flash"), "gemini-2.0-flash");
        assert_eq!(encode_uri_component("models/gemini"), "models%2Fgemini");
        assert_eq!(encode_uri_component("a b~c"), "a%20b~c");
        assert_eq!(encode_uri_component("résumé"), "r%C3%A9sum%C3%A9");
    }

    #[test]
    fn content_parts_text_and_null() {
        assert_eq!(
            content_to_gemini_parts(&MessageContent::Text("hi".into())),
            vec![json!({"text": "hi"})]
        );
        assert_eq!(
            content_to_gemini_parts(&MessageContent::Null(())),
            vec![json!({"text": ""})]
        );
        assert_eq!(
            content_to_gemini_parts(&MessageContent::Parts(vec![])),
            vec![json!({"text": ""})]
        );
    }

    #[test]
    fn content_parts_images() {
        let parts = vec![
            text_part("look"),
            image_part("data:image/png;base64,AAAA"),
            // svg data URLs end up as image/jpeg (regex can't match svg+xml)
            image_part("data:image/svg+xml;base64,BBBB"),
            image_part("https://example.com/photo.png"),
            // query string survives the naive extension lookup → defaults png
            image_part("https://example.com/photo.jpeg?x=1"),
        ];
        let got = content_to_gemini_parts(&MessageContent::Parts(parts));
        assert_eq!(
            json!(got),
            json!([
                {"text": "look"},
                {"inlineData": {"mimeType": "image/png", "data": "AAAA"}},
                {"inlineData": {"mimeType": "image/jpeg", "data": "BBBB"}},
                {"fileData": {"mimeType": "image/png", "fileUri": "https://example.com/photo.png"}},
                {"fileData": {"mimeType": "image/png", "fileUri": "https://example.com/photo.jpeg?x=1"}}
            ])
        );
    }

    #[test]
    fn gemini_contents_translation() {
        let messages = vec![
            msg("system", "You are helpful"),
            msg("system", "Be concise"),
            msg("user", "hi"),
            ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text("Let me check".into()),
                name: None,
                tool_call_id: None,
                tool_calls: Some(vec![tool_call("call_1", "weather", r#"{"city":"Paris"}"#)]),
                extra: serde_json::Map::new(),
            },
            ChatMessage {
                role: "tool".to_string(),
                content: MessageContent::Text(r#"{"temp":22}"#.into()),
                name: None,
                tool_call_id: Some("call_1".into()),
                tool_calls: None,
                extra: serde_json::Map::new(),
            },
        ];
        let (contents, system) = to_gemini_contents(&messages);
        assert_eq!(
            system,
            Some(json!({"parts": [{"text": "You are helpful\n\nBe concise"}]}))
        );
        assert_eq!(
            json!(contents),
            json!([
                {"role": "user", "parts": [{"text": "hi"}]},
                {"role": "model", "parts": [
                    {"text": "Let me check"},
                    {"functionCall": {"id": "call_1", "name": "weather", "args": {"city": "Paris"}}}
                ]},
                {"role": "user", "parts": [
                    {"functionResponse": {"id": "call_1", "name": "weather", "response": {"temp": 22}}}
                ]}
            ])
        );
    }

    #[test]
    fn gemini_contents_drops_skipped_entries() {
        // Assistant with null content and no tool calls → dropped entirely.
        let messages = vec![ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::Null(()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            extra: serde_json::Map::new(),
        }];
        let (contents, system) = to_gemini_contents(&messages);
        assert_eq!(contents.len(), 0);
        assert_eq!(system, None);
    }

    #[test]
    fn request_body_with_options() {
        let messages = vec![msg("system", "Be nice"), msg("user", "Hi")];
        let options = CompletionOptions {
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: Some(0.9),
            seed: Some(42),
            frequency_penalty: Some(0.5),
            presence_penalty: Some(-0.1),
            ..CompletionOptions::default()
        };
        let body = gemini_request_body(&messages, &options);
        assert_eq!(
            body,
            json!({
                "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
                "generationConfig": {
                    "temperature": 0.7,
                    "maxOutputTokens": 100,
                    "topP": 0.9,
                    "seed": 42,
                    "frequencyPenalty": 0.5,
                    "presencePenalty": -0.1
                },
                "systemInstruction": {"parts": [{"text": "Be nice"}]}
            })
        );
    }

    #[test]
    fn request_body_response_schema() {
        let options = CompletionOptions {
            response_format: Some(ResponseFormat {
                r#type: "json_object".into(),
                json_schema: Some(json!({"type": "OBJECT"})),
            }),
            ..CompletionOptions::default()
        };
        let body = gemini_request_body(&[], &options);
        assert_eq!(body["responseSchema"], json!({"type": "OBJECT"}));

        // json_schema absent → Gemini `{ type: 'OBJECT' }` default
        let options2 = CompletionOptions {
            response_format: Some(ResponseFormat {
                r#type: "json_schema".into(),
                json_schema: None,
            }),
            ..CompletionOptions::default()
        };
        let b2 = gemini_request_body(&[], &options2);
        assert_eq!(b2["responseSchema"], json!({"type": "OBJECT"}));

        // 'text' type → no responseSchema key
        let options3 = CompletionOptions {
            response_format: Some(ResponseFormat {
                r#type: "text".into(),
                json_schema: None,
            }),
            ..CompletionOptions::default()
        };
        let b3 = gemini_request_body(&[], &options3);
        assert!(b3.get("responseSchema").is_none());
    }

    #[test]
    fn tools_and_tool_config() {
        let tools = vec![ChatToolDefinition {
            r#type: "function".into(),
            function: ChatToolFunctionDefinition {
                name: "get_weather".into(),
                description: Some("Get weather".into()),
                parameters: Some(json!({"type": "object"})),
                strict: None,
            },
        }];
        assert_eq!(
            to_gemini_tools(Some(&tools)),
            Some(json!([
                {"functionDeclarations": [
                    {"name": "get_weather", "description": "Get weather", "parameters": {"type": "object"}}
                ]}
            ]))
        );
        assert_eq!(to_gemini_tools(Some(&vec![])), None);
        assert_eq!(to_gemini_tools(None), None);

        assert_eq!(
            to_gemini_tool_config(Some(&ChatToolChoice::Mode("none".into()))),
            Some(json!({"functionCallingConfig": {"mode": "NONE"}}))
        );
        assert_eq!(
            to_gemini_tool_config(Some(&ChatToolChoice::Mode("required".into()))),
            Some(json!({"functionCallingConfig": {"mode": "ANY"}}))
        );
        assert_eq!(
            to_gemini_tool_config(Some(&ChatToolChoice::Mode("auto".into()))),
            Some(json!({"functionCallingConfig": {"mode": "AUTO"}}))
        );
        let named = ChatToolChoice::Named {
            r#type: "function".into(),
            function: ChatToolChoiceFunction {
                name: "get_weather".into(),
            },
        };
        assert_eq!(
            to_gemini_tool_config(Some(&named)),
            Some(json!({"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": ["get_weather"]}}))
        );
        assert_eq!(to_gemini_tool_config(None), None);
    }

    #[test]
    fn parses_gemini_response_and_maps_to_openai() {
        let raw = r#"{
          "candidates": [{
            "content": { "role": "model", "parts": [ { "text": "Hello world" }, { "text": "!" } ] },
            "finishReason": "STOP"
          }],
          "usageMetadata": { "promptTokenCount": 10, "candidatesTokenCount": 5, "totalTokenCount": 15 }
        }"#;
        let data: GeminiResponse = serde_json::from_str(raw).unwrap();
        let resp = gemini_to_openai(&data, "chatcmpl-x".to_string(), 12345, "gemini-2.0-flash");
        assert_eq!(resp.id, "chatcmpl-x");
        assert_eq!(resp.model, "gemini-2.0-flash");
        assert_eq!(resp.object, "chat.completion");
        assert_eq!(resp.choices.len(), 1);
        let choice = &resp.choices[0];
        assert_eq!(choice.message.content.as_text(), Some("Hello world!"));
        assert_eq!(choice.finish_reason.as_deref(), Some("stop"));
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.completion_tokens, 5);
        assert_eq!(resp.usage.total_tokens, 15);
        assert!(choice.message.tool_calls.is_none());
        assert_eq!(resp._routed_via.as_ref().unwrap().platform, "google");
    }

    #[test]
    fn parses_function_call_parts() {
        let raw = r#"{
          "candidates": [{
            "content": { "role": "model", "parts": [
              { "thoughtSignature": "sig1", "functionCall": { "id": "call_9", "name": "get_weather", "args": { "city": "Paris" } } }
            ]},
            "finishReason": "STOP"
          }]
        }"#;
        let data: GeminiResponse = serde_json::from_str(raw).unwrap();
        let resp = gemini_to_openai(&data, "id".into(), 1, "m");
        let choice = &resp.choices[0];
        assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
        assert!(choice.message.content.is_null());
        let calls = choice.message.tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_9");
        assert_eq!(calls[0].r#type, "function");
        assert_eq!(calls[0].function.name, "get_weather");
        assert_eq!(calls[0].function.arguments, r#"{"city":"Paris"}"#);
        assert_eq!(calls[0].thought_signature.as_deref(), Some("sig1"));
    }

    #[test]
    fn finish_reason_uses_max_tokens_mapping() {
        let raw = r#"{"candidates": [{"content": {"parts": [{"text": "x"}]}, "finishReason": "MAX_TOKENS"}]}"#;
        let data: GeminiResponse = serde_json::from_str(raw).unwrap();
        let resp = gemini_to_openai(&data, "id".into(), 1, "m");
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("length"));
    }

    #[test]
    fn stream_dedups_tool_calls() {
        let a = tool_call("call_1", "weather", r#"{"city":"Paris"}"#);
        let b = tool_call("call_2", "temp", "{}");
        let mut seen = HashSet::new();
        let first = dedup_tool_calls(vec![a.clone(), a.clone(), b.clone()], &mut seen);
        assert_eq!(first.len(), 2);
        let again = dedup_tool_calls(vec![a, b], &mut seen);
        assert!(again.is_empty());
    }
}
