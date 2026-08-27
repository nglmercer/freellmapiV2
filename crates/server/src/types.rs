//! OpenAI-compatible wire types use snake_case; admin and dashboard DTOs use
//! camelCase to preserve the public JSON contract.

use serde::{Deserialize, Serialize};

/// Active provider platforms accepted by the API-key and model routes.
pub const PLATFORMS: &[&str] = &[
    "google",
    "groq",
    "cerebras",
    "sambanova",
    "nvidia",
    "mistral",
    "openrouter",
    "github",
    "cohere",
    "cloudflare",
    "zhipu",
    "ollama",
    "kilo",
    "pollinations",
    "llm7",
];

// ---- Platform & Model Types ----

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: i64,
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    pub intelligence_rank: Option<i64>,
    pub speed_rank: Option<i64>,
    pub size_label: String,
    pub rpm_limit: Option<i64>,
    pub rpd_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub tpd_limit: Option<i64>,
    pub monthly_token_budget: String,
    pub context_window: Option<i64>,
    pub enabled: bool,
}

pub type KeyStatus = str; // 'healthy' | 'rate_limited' | 'invalid' | 'error' | 'unknown'
pub const KEY_STATUSES: &[&str] = &["healthy", "rate_limited", "invalid", "error", "unknown"];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: i64,
    pub platform: String,
    pub label: String,
    pub masked_key: String,
    pub status: String,
    pub enabled: bool,
    pub created_at: String,
    pub last_checked_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyCreate {
    pub platform: String,
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
}

// ---- Fallback Config ----

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FallbackEntry {
    pub model_id: i64,
    pub platform: String,
    pub display_name: String,
    pub intelligence_rank: Option<i64>,
    pub speed_rank: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
}

// ---- OpenAI-Compatible Types ----

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContentPart {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    /// Unknown provider-specific fields pass through the JSON representation.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageUrl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// `string | ContentPart[] | null` from shared/types.ts.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
    /// Matches JSON `null` (unit `()` deserializes from serde's `unit`).
    Null(()),
}

impl MessageContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, MessageContent::Null(_))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChatToolCallFunction {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatToolCall {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(rename = "type", skip_serializing_if = "String::is_empty", default)]
    pub r#type: String, // "function"
    pub function: ChatToolCallFunction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ChatToolCall {
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl Default for ChatToolCall {
    fn default() -> Self {
        Self {
            id: String::new(),
            r#type: String::new(),
            function: ChatToolCallFunction::default(),
            thought_signature: None,
            index: None,
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatToolFunctionDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatToolDefinition {
    pub r#type: String, // "function"
    pub function: ChatToolFunctionDefinition,
}

/// `'none' | 'auto' | 'required' | { type: 'function', function: { name } }`
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ChatToolChoice {
    Mode(String),
    Named {
        r#type: String,
        function: ChatToolChoiceFunction,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatToolChoiceFunction {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    #[serde(default = "default_role")]
    pub role: String, // 'system' | 'user' | 'assistant' | 'tool'
    #[serde(default = "null_content")]
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn null_content() -> MessageContent {
    MessageContent::Null(())
}

fn default_role() -> String {
    "assistant".to_string()
}

impl ChatMessage {
    pub fn text(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: MessageContent::Text(content.to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResponseFormat {
    pub r#type: String, // 'text' | 'json_object' | 'json_schema'
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatCompletionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatToolDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,
}

/// Options passed to an individual provider call.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CompletionOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatToolDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,
    /// `stop` sequences — used by the legacy completions route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

impl CompletionOptions {
    pub fn from_request(req: &ChatCompletionRequest) -> Self {
        Self {
            model: req.model.clone(),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            top_p: req.top_p,
            seed: req.seed,
            frequency_penalty: req.frequency_penalty,
            presence_penalty: req.presence_penalty,
            user: req.user.clone(),
            response_format: req.response_format.clone(),
            tools: req.tools.clone(),
            tool_choice: req.tool_choice.clone(),
            parallel_tool_calls: req.parallel_tool_calls,
            stream_options: req.stream_options.clone(),
            logprobs: req.logprobs,
            top_logprobs: req.top_logprobs,
            stop: None,
        }
    }
}

// ---- OpenAI-Compatible Completions (Legacy) ----

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogprobToken {
    pub token: String,
    pub logprob: f64,
    pub bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<LogprobToken>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Logprobs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_offset: Option<Vec<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_logprobs: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<serde_json::Map<String, serde_json::Value>>>,
}

/// `prompt` accepts a string or an array of strings.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum PromptField {
    Text(String),
    List(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: PromptField,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_of: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<i64>,
}

/// `stop` accepts a string or an array of strings.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum StopField {
    Text(String),
    List(Vec<String>),
}

impl StopField {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            StopField::Text(s) => vec![s.clone()],
            StopField::List(v) => v.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionChoice {
    pub text: String,
    pub index: i64,
    pub logprobs: Option<Logprobs>,
    pub finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

pub fn default_usage() -> TokenUsage {
    TokenUsage::default()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoutedVia {
    pub platform: String,
    pub model: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String, // "text_completion"
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _routed_via: Option<RoutedVia>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionChunk {
    pub id: String,
    pub object: String, // "text_completion.chunk"
    pub created: i64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

// ---- Chat completion responses ----

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatCompletionChoice {
    #[serde(default)]
    pub index: i64,
    pub message: ChatMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<serde_json::Value>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatCompletionResponse {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String, // "chat.completion"
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(default = "default_usage")]
    pub usage: TokenUsage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub _routed_via: Option<RoutedVia>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChatCompletionDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatCompletionChunkChoice {
    #[serde(default)]
    pub index: i64,
    #[serde(default)]
    pub delta: ChatCompletionDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<serde_json::Value>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatCompletionChunk {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: String, // "chat.completion.chunk"
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<ChatCompletionChunkChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ---- Analytics Types ----

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSummary {
    pub total_requests: i64,
    pub success_rate: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub avg_latency_ms: f64,
    pub estimated_cost_savings: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStats {
    pub platform: String,
    pub requests: i64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub timestamp: String,
    pub requests: i64,
    pub success_count: i64,
    pub failure_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RequestLog {
    pub id: i64,
    pub platform: String,
    pub model_id: String,
    pub status: String, // 'success' | 'error'
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub error: Option<String>,
    pub created_at: String,
}

// ---- Rate Limit Types ----

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageWindow {
    pub used: i64,
    pub limit: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitStatus {
    pub platform: String,
    pub model_id: String,
    pub rpm: UsageWindow,
    pub rpd: UsageWindow,
    pub tpm: UsageWindow,
    pub available: bool,
    pub next_reset_at: Option<String>,
}
