// ---- Platform & Model Types ----

export interface RankingMetric {
  score: number | null;
  rank: number | null;
  source: string | null;
  confidence: number;
  updatedAt: string | null;
  status: 'fresh' | 'stale' | 'unknown';
  ranked: boolean;
}

export interface SpeedMetric {
  tokensPerSecond: number | null;
  rank: number | null;
  source: string | null;
  confidence: number;
  updatedAt: string | null;
  status: 'fresh' | 'stale' | 'unknown';
  ranked: boolean;
  sampleCount: number;
}

export interface ReliabilityMetric {
  successRate: number | null;
  sampleCount: number;
  rateLimitRate: number | null;
}

// Active platforms — must match the Rust provider registry and API-key route
// allowlist.
// Hugging Face, Moonshot, and MiniMax direct integrations were dropped
// in the historical model migrations.
export type Platform =
  | 'google'
  | 'groq'
  | 'cerebras'
  | 'sambanova'
  | 'nvidia'
  | 'mistral'
  | 'openrouter'
  | 'github'
  | 'cohere'
  | 'cloudflare'
  | 'zhipu'
  | 'ollama'
  | 'kilo'
  | 'pollinations'
  | 'llm7';

export interface Model {
  id: number;
  platform: Platform;
  modelId: string;
  displayName: string;
  intelligenceRank: number | null;
  speedRank: number | null;
  sizeLabel: string;
  rpmLimit: number | null;
  rpdLimit: number | null;
  tpmLimit: number | null;
  tpdLimit: number | null;
  monthlyTokenBudget: string;
  contextWindow: number | null;
  enabled: boolean;
  quality?: RankingMetric;
  speed?: SpeedMetric;
  reliability?: ReliabilityMetric;
  rankingConfidence?: number;
  routing?: { balancedScore: number | null };
}

export type KeyStatus = 'healthy' | 'rate_limited' | 'invalid' | 'error' | 'unknown';

export interface ApiKey {
  id: number;
  platform: Platform;
  label: string;
  maskedKey: string;
  status: KeyStatus;
  enabled: boolean;
  createdAt: string;
  lastCheckedAt: string | null;
}

export interface ApiKeyCreate {
  platform: Platform;
  key: string;
  label?: string;
}

// ---- Fallback Config ----

export interface FallbackEntry {
  modelId: number;
  platform: Platform;
  displayName: string;
  intelligenceRank: number | null;
  speedRank: number | null;
  priority: number;
  enabled: boolean;
  manualPriority?: number | null;
  intelligenceScore?: number | null;
  speedTokensPerSec?: number | null;
  rankingSource?: string | null;
  lastRankedAt?: string | null;
  quality?: RankingMetric;
  speed?: SpeedMetric;
  reliability?: ReliabilityMetric;
  rankingConfidence?: number;
  routing?: { balancedScore: number | null };
}

// ---- OpenAI-Compatible Types ----

export interface ContentPart {
  type: string;
  text?: string;
  image_url?: {
    url: string;
    detail?: string;
  };
}

export interface ChatToolCallFunction {
  name: string;
  arguments: string;
}

export interface ChatToolCall {
  id: string;
  type: 'function';
  function: ChatToolCallFunction;
  thought_signature?: string;
}

export interface ChatToolFunctionDefinition {
  name: string;
  description?: string;
  parameters?: Record<string, unknown>;
  strict?: boolean;
}

export interface ChatToolDefinition {
  type: 'function';
  function: ChatToolFunctionDefinition;
}

export type ChatToolChoice =
  | 'none'
  | 'auto'
  | 'required'
  | {
    type: 'function';
    function: {
      name: string;
    };
  };

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string | ContentPart[] | null;
  name?: string;
  tool_call_id?: string;
  tool_calls?: ChatToolCall[];
}

export interface ChatCompletionRequest {
  model?: string;
  messages: ChatMessage[];
  temperature?: number;
  max_tokens?: number;
  n?: number;
  stream?: boolean;
  stream_options?: {
    include_usage?: boolean;
  };
  top_p?: number;
  seed?: number;
  frequency_penalty?: number;
  presence_penalty?: number;
  user?: string;
  response_format?: {
    type: 'text' | 'json_object' | 'json_schema';
    json_schema?: Record<string, unknown>;
  };
  tools?: ChatToolDefinition[];
  tool_choice?: ChatToolChoice;
  parallel_tool_calls?: boolean;
  logprobs?: boolean;
  top_logprobs?: number;
}

// ---- OpenAI-Compatible Completions (Legacy) ----

export interface LogprobToken {
  token: string;
  logprob: number;
  bytes: number[] | null;
  top_logprobs?: {
    token: string;
    logprob: number;
    bytes: number[] | null;
  }[];
}

export interface Logprobs {
  text_offset?: number[];
  token_logprobs?: number[];
  tokens?: string[];
  top_logprobs?: Record<string, number>[];
}

export interface CompletionRequest {
  model: string;
  prompt: string[];
  suffix?: string;
  max_tokens?: number;
  temperature?: number;
  top_p?: number;
  n?: number;
  stream?: boolean;
  stream_options?: {
    include_usage?: boolean;
  };
  stop?: string[];
  echo?: boolean;
  best_of?: number;
  seed?: number;
  frequency_penalty?: number;
  presence_penalty?: number;
  user?: string;
  logprobs?: number;
}

export interface CompletionChoice {
  text: string;
  index: number;
  logprobs: Logprobs | null;
  finish_reason: string | null;
}

export interface CompletionResponse {
  id: string;
  object: 'text_completion';
  created: number;
  model: string;
  choices: CompletionChoice[];
  usage: TokenUsage;
  _routed_via?: {
    platform: Platform;
    model: string;
  };
}

export interface CompletionChunk {
  id: string;
  object: 'text_completion.chunk';
  created: number;
  model: string;
  choices: {
    text: string;
    index: number;
    logprobs: Logprobs | null;
    finish_reason: string | null;
  }[];
  usage?: TokenUsage;
}

export interface ChatCompletionChoice {
  index: number;
  message: ChatMessage;
  finish_reason: string | null;
  logprobs?: {
    content: LogprobToken[] | null;
  } | null;
}

export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface ChatCompletionResponse {
  id: string;
  object: 'chat.completion';
  created: number;
  model: string;
  choices: ChatCompletionChoice[];
  usage: TokenUsage;
  _routed_via?: {
    platform: Platform;
    model: string;
  };
}

export interface ChatCompletionChunk {
  id: string;
  object: 'chat.completion.chunk';
  created: number;
  model: string;
  choices: {
    index: number;
    delta: {
      role?: 'assistant';
      content?: string;
      tool_calls?: ChatToolCall[];
    };
    finish_reason: string | null;
    logprobs?: {
      content: LogprobToken[] | null;
    } | null;
  }[];
  usage?: TokenUsage;
}

// ---- Analytics Types ----

export interface AnalyticsSummary {
  totalRequests: number;
  successRate: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  avgLatencyMs: number;
  estimatedCostSavings: number;
}

export interface PlatformStats {
  platform: Platform;
  requests: number;
  successRate: number;
  avgLatencyMs: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  [key: string]: string | number;
}

export interface TimelinePoint {
  timestamp: string;
  requests: number;
  successCount: number;
  failureCount: number;
  [key: string]: string | number;
}

export interface RequestLog {
  id: number;
  platform: Platform;
  modelId: string;
  status: 'success' | 'error';
  inputTokens: number;
  outputTokens: number;
  latencyMs: number;
  error: string | null;
  createdAt: string;
}

// ---- Rate Limit Types ----

export interface RateLimitStatus {
  platform: Platform;
  modelId: string;
  rpm: { used: number; limit: number | null };
  rpd: { used: number; limit: number | null };
  tpm: { used: number; limit: number | null };
  available: boolean;
  nextResetAt: string | null;
}
