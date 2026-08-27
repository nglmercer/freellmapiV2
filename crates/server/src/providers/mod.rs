//! Platform-to-provider registry for built-ins and database-backed custom
//! providers.

pub mod base;
pub mod cloudflare;
pub mod cohere;
pub mod custom;
pub mod google;
pub mod openai_compat;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use rusqlite::Connection;

pub use base::{
    http_client, make_id, normalize_choices, send_request, ChunkReceiver, ChunkSender, Provider,
    ProviderError,
};
pub use cloudflare::CloudflareProvider;
pub use cohere::CohereProvider;
pub use custom::{
    has_custom_provider, load_all_custom_providers, load_custom_provider, platform_to_provider_id,
    provider_id_to_platform,
};
pub use google::GoogleProvider;
pub use openai_compat::{OpenAICompatOptions, OpenAICompatProvider};

fn openai_compat(platform: &str, name: &str, base_url: &str) -> Arc<dyn Provider> {
    Arc::new(OpenAICompatProvider::new(OpenAICompatOptions {
        platform: platform.to_string(),
        name: name.to_string(),
        base_url: base_url.to_string(),
        extra_headers: Vec::new(),
        validate_url: None,
        timeout_ms: 15000,
    }))
}

/// Built-in registry, constructed once during the first lookup.
fn builtins() -> &'static HashMap<String, Arc<dyn Provider>> {
    static REGISTRY: OnceLock<HashMap<String, Arc<dyn Provider>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m: HashMap<String, Arc<dyn Provider>> = HashMap::new();
        let mut register = |p: Arc<dyn Provider>| {
            m.insert(p.platform(), p);
        };

        // Google - unique Gemini API format
        register(Arc::new(GoogleProvider::new()));

        // Groq - OpenAI-compatible
        register(openai_compat(
            "groq",
            "Groq",
            "https://api.groq.com/openai/v1",
        ));

        // Cerebras - OpenAI-compatible
        register(openai_compat(
            "cerebras",
            "Cerebras",
            "https://api.cerebras.ai/v1",
        ));

        // SambaNova - OpenAI-compatible
        register(openai_compat(
            "sambanova",
            "SambaNova",
            "https://api.sambanova.ai/v1",
        ));

        // NVIDIA NIM - OpenAI-compatible
        register(openai_compat(
            "nvidia",
            "NVIDIA NIM",
            "https://integrate.api.nvidia.com/v1",
        ));

        // Mistral - OpenAI-compatible
        register(openai_compat(
            "mistral",
            "Mistral",
            "https://api.mistral.ai/v1",
        ));

        // OpenRouter - OpenAI-compatible with extra headers
        register(Arc::new(OpenAICompatProvider::new(OpenAICompatOptions {
            platform: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            extra_headers: vec![
                (
                    "HTTP-Referer".to_string(),
                    "http://localhost:3001".to_string(),
                ),
                ("X-Title".to_string(), "FreeLLMAPI".to_string()),
            ],
            validate_url: None,
            timeout_ms: 15000,
        })));

        // GitHub Models — OpenAI-compatible. Catalog uses `<publisher>/<model>`
        // ids (e.g. `openai/gpt-4.1`); the old Azure endpoint rejects that
        // prefix with "Unknown model", so route to the current
        // models.github.ai endpoint.
        register(openai_compat(
            "github",
            "GitHub Models",
            "https://models.github.ai/inference",
        ));

        // Cohere - OpenAI-compatible via Cohere compatibility endpoint
        register(Arc::new(CohereProvider::new()));

        // Cloudflare Workers AI - OpenAI-compatible endpoint
        // (key = "account_id:token")
        register(Arc::new(CloudflareProvider::new()));

        // Zhipu (Z.ai / bigmodel.cn) - OpenAI-compatible
        register(openai_compat(
            "zhipu",
            "Zhipu AI",
            "https://open.bigmodel.cn/api/paas/v4",
        ));

        // Hugging Face, Moonshot, MiniMax direct integrations were dropped in
        // V4 — HF tool-call format issues; Moonshot moved to paid; MiniMax
        // superseded by the OpenRouter route
        // (openrouter/minimax/minimax-m2.5:free).

        // Ollama Cloud — OpenAI-compatible. Free plan: 1 concurrent model, 5h
        // session caps, GPU-time-based quota (not per-token). Many catalog
        // models on the /v1/models list are subscription-only — Free returns
        // 403 with an explicit "this model requires a subscription" message.
        // Catalog rows are filtered to confirmed-Free entries.
        //
        // Frontier reasoning models (glm-4.7, kimi-k2-thinking,
        // cogito-2.1:671b) regularly take 30-90s on Ollama Cloud Free, so the
        // timeout is bumped from the default 15s. Ollama returns reasoning in
        // `message.reasoning` (not `reasoning_content`) — handled by
        // normalize_choices.
        register(Arc::new(OpenAICompatProvider::new(OpenAICompatOptions {
            platform: "ollama".to_string(),
            name: "Ollama Cloud".to_string(),
            base_url: "https://ollama.com/v1".to_string(),
            extra_headers: Vec::new(),
            validate_url: None,
            timeout_ms: 120000,
        })));

        // Kilo AI Gateway — OpenAI-compatible aggregator. Anonymous access
        // works (200 req/hr per IP) for the few :free routes still active; a
        // Kilo API key raises the limit. Most named "free" routes in the docs
        // have transitioned to paid ("free period ended") — probe before
        // adding catalog rows.
        register(openai_compat(
            "kilo",
            "Kilo Gateway",
            "https://api.kilo.ai/api/gateway/v1",
        ));

        // Pollinations — OpenAI-compatible, anonymous tier. The chat
        // completions endpoint lives at `/openai/v1/chat/completions` (NOT
        // `/v1/...` — the `/openai` prefix is mandatory). Public model list
        // returns one anonymous model (`openai-fast` = GPT-OSS 20B on OVH,
        // tools=true).
        register(openai_compat(
            "pollinations",
            "Pollinations",
            "https://text.pollinations.ai/openai/v1",
        ));

        // LLM7.io — OpenAI-compatible aggregator. 100 req/hr free; anonymous
        // access also works for basic models. Wraps a handful of upstream
        // models behind one token (GPT-OSS, Llama 3.1 Turbo via Meta,
        // Codestral via Mistral, Ministral, GLM-4.6V-Flash).
        register(openai_compat("llm7", "LLM7", "https://api.llm7.io/v1"));

        // Chutes was evaluated for V11 and dropped: probe with a free-tier
        // key returned 402 on every model — "Quota exceeded and account
        // balance is $0.0, please pay with fiat or send tao". The "free" tier
        // requires a non-zero balance, which conflicts with the project's
        // no-card criterion.

        m
    })
}

/// Resolve built-ins first, then database-backed custom providers.
/// Requires a DB connection handle (custom providers live in SQLite); use
/// [`get_provider_async`] when no connection is already locked.
pub fn get_provider_with_conn(conn: &Connection, platform: &str) -> Option<Arc<dyn Provider>> {
    if let Some(p) = builtins().get(platform) {
        return Some(p.clone());
    }
    load_custom_provider(conn, platform)
}

pub async fn get_provider(platform: &str) -> Option<Arc<dyn Provider>> {
    if let Some(p) = builtins().get(platform) {
        return Some(p.clone());
    }
    let conn = crate::db::db().lock().await;
    load_custom_provider(&conn, platform)
}

/// `getAllProviders()`
pub async fn get_all_providers() -> Vec<Arc<dyn Provider>> {
    let mut all: Vec<Arc<dyn Provider>> = builtins().values().cloned().collect();
    let conn = crate::db::db().lock().await;
    all.extend(load_all_custom_providers(&conn).into_values());
    all
}

/// `hasProvider(platform)`
pub async fn has_provider(platform: &str) -> bool {
    if builtins().contains_key(platform) {
        return true;
    }
    let conn = crate::db::db().lock().await;
    load_custom_provider(&conn, platform).is_some()
}
