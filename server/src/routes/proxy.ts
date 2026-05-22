import { Hono } from 'hono';
import { z } from 'zod';
import type { ChatMessage } from '@freellmapi/shared/types.js';
import { routeRequest, recordRateLimitHit, recordSuccess, type RouteResult } from '../services/router.js';
import { apiKeyAuth, validateChatBody, normalizeMessages, estimateInputTokens } from './middleware';
import { recordRequest, recordTokens, setCooldown, getStickyModel } from '../services/ratelimit.js';
import { getDb, getUnifiedApiKey } from '../db/index.js';
import { handleStreamingCompletion, handleStandardCompletion } from './streamHandler.js';
import crypto from 'crypto';

// Virtual "auto" model. Clients like Hermes require a non-empty `model` field
// on every request, but freellmapi's whole point is to pick the model itself.
// Requesting this id means "let the router decide" — identical to omitting
// `model` entirely.
const AUTO_MODEL_ID = 'auto';

function isAutoModel(modelId: string | undefined): boolean {
  return modelId === AUTO_MODEL_ID;
}

// Constant-time string comparison for the unified API key. Plain `===` leaks
// length and per-character timing, which a network attacker could in principle
// use to recover the key one byte at a time.
function timingSafeStringEqual(provided: string, expected: string): boolean {
  const a = Buffer.from(provided);
  const b = Buffer.from(expected);
  // Compare against a same-length buffer regardless of input length so the
  // comparison itself runs in constant time; the explicit length check at the
  // end is what actually decides equality when lengths differ.
  const compareA = a.length === b.length ? a : Buffer.alloc(b.length);
  return crypto.timingSafeEqual(compareA, b) && a.length === b.length;
}

// Sticky sessions: track which model served each "session"
// Key: hash of first user message → model_db_id
// This prevents model switching mid-conversation which causes hallucination
const stickySessionMap = new Map<string, { modelDbId: number; lastUsed: number }>();
const STICKY_TTL_MS = 30 * 60 * 1000; // 30 min session TTL

function getSessionKey(messages: ChatMessage[]): string {
  // Use the first user message as session identifier — clients like Hermes
  // re-send the full conversation each turn, so the first user message is
  // stable across turns. Hash the FULL message (not a 100-char slice) so
  // distinct conversations with identical openings don't collide.
  const firstUser = messages.find(m => m.role === 'user');
  if (!firstUser || typeof firstUser.content !== 'string') return '';
  const hash = crypto.createHash('sha1').update(firstUser.content).digest('hex');
  return `${hash}:${messages.length > 2 ? 'multi' : 'single'}`;
}

function getStickyModel(messages: ChatMessage[]): number | undefined {
  // Only apply sticky for multi-turn (has assistant messages = continuation)
  const hasAssistant = messages.some(m => m.role === 'assistant');
  if (!hasAssistant) return undefined;

  const key = getSessionKey(messages);
  if (!key) return undefined;

  const entry = stickySessionMap.get(key);
  if (!entry) return undefined;

  if (Date.now() - entry.lastUsed > STICKY_TTL_MS) {
    stickySessionMap.delete(key);
    return undefined;
  }
  return entry.modelDbId;
}

function setStickyModel(messages: ChatMessage[], modelDbId: number) {
  const key = getSessionKey(messages);
  if (!key) return;
  stickySessionMap.set(key, { modelDbId, lastUsed: Date.now() });

  // Cleanup old entries
  if (stickySessionMap.size > 500) {
    const now = Date.now();
    for (const [k, v] of stickySessionMap) {
      if (now - v.lastUsed > STICKY_TTL_MS) stickySessionMap.delete(k);
    }
  }
}

// OpenAI-compatible /models endpoint (used by Hermes for metadata)
export const proxyRouter = new Hono();

proxyRouter.get('/models', async (c) => {
  const db = getDb();
  interface ModelRow {
    platform: string;
    model_id: string;
    display_name: string;
    context_window: number | null;
  }
  const models = db.query('SELECT platform, model_id, display_name, context_window FROM models WHERE enabled = 1 ORDER BY intelligence_rank').all() as ModelRow[];
  return c.json({
    object: 'list',
    data: [
      {
        id: AUTO_MODEL_ID,
        object: 'model',
        created: 0,
        owned_by: 'freellmapi',
        name: 'Auto (router picks the best available model)',
        context_window: null,
      },
      ...models.map(m => ({
        id: m.model_id,
        object: 'model',
        created: 0,
        owned_by: m.platform,
        name: m.display_name,
        context_window: m.context_window,
      })),
    ],
  });
});

const MAX_RETRIES = 20;

const toolCallSchema = z.object({
  id: z.string().min(1),
  type: z.literal('function'),
  function: z.object({
    name: z.string().min(1),
    arguments: z.string(),
  }),
  thought_signature: z.string().optional(),
});

const systemMessageSchema = z.object({
  role: z.literal('system'),
  content: z.string(),
  name: z.string().optional(),
});

const userMessageSchema = z.object({
  role: z.literal('user'),
  content: z.string(),
  name: z.string().optional(),
});

const assistantMessageSchema = z.object({
  role: z.literal('assistant'),
  content: z.string().nullable().optional(),
  name: z.string().optional(),
  tool_calls: z.array(toolCallSchema).optional(),
}).refine((msg) => {
  const hasContent = typeof msg.content === 'string' && msg.content.length > 0;
  const hasToolCalls = (msg.tool_calls?.length ?? 0) > 0;
  return hasContent || hasToolCalls;
}, {
  message: 'assistant messages must include non-empty content or tool_calls',
});

const toolMessageSchema = z.object({
  role: z.literal('tool'),
  content: z.string(),
  tool_call_id: z.string().min(1),
  name: z.string().optional(),
});

const toolDefinitionSchema = z.object({
  type: z.literal('function'),
  function: z.object({
    name: z.string().min(1),
    description: z.string().optional(),
    parameters: z.record(z.string(), z.unknown()).optional(),
    strict: z.boolean().optional(),
  }),
});

const toolChoiceSchema = z.union([
  z.enum(['none', 'auto', 'required']),
  z.object({
    type: z.literal('function'),
    function: z.object({
      name: z.string().min(1),
    }),
  }),
]);

const chatCompletionSchema = z.object({
  messages: z.array(z.union([
    systemMessageSchema,
    userMessageSchema,
    assistantMessageSchema,
    toolMessageSchema,
  ])).min(1),
  model: z.string().optional(),
  temperature: z.number().min(0).max(2).optional(),
  max_tokens: z.number().int().positive().optional(),
  top_p: z.number().min(0).max(1).optional(),
  stream: z.boolean().optional(),
  tools: z.array(toolDefinitionSchema).optional(),
  tool_choice: toolChoiceSchema.optional(),
  parallel_tool_calls: z.boolean().optional(),
});

function isRetryableError(err: unknown): boolean {
  if (err && typeof err === 'object' && 'message' in err && typeof err.message === 'string') {
    const msg = err.message.toLowerCase();
    return msg.includes('429') || msg.includes('rate limit') || msg.includes('too many requests')
      || msg.includes('quota') || msg.includes('resource_exhausted')
      || msg.includes('aborted') || msg.includes('timeout') || msg.includes('etimedout')
      || msg.includes('econnrefused') || msg.includes('econnreset')
      || msg.includes('503') || msg.includes('unavailable')
      || msg.includes('500') || msg.includes('internal server error');
  }
  return false;
}

proxyRouter.post('/chat/completions', apiKeyAuth, validateChatBody, async (c) => {
  const start = Date.now();
  const data = c.get('parsedData'); // Retreived from validation middleware

  const { model: requestedModel, stream, messages: rawMessages, ...passthroughOptions } = data;
  const messages = normalizeMessages(rawMessages);

  const estimatedInputTokens = estimateInputTokens(messages);
  const estimatedTotal = estimatedInputTokens + (passthroughOptions.max_tokens ?? 1000);

  // Model validation / lookup
  const { preferredModel, errorResponse } = resolvePreferredModel(requestedModel, messages);
  if (errorResponse) {
    c.status(errorResponse.status);
    return c.json(errorResponse.json);
  }

  const skipKeys = new Set<string>();
  let lastError: string | null = null;

  // Resilient Retry Loop
  for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
    let route: RouteResult;
    try {
      route = routeRequest(estimatedTotal, skipKeys.size > 0 ? skipKeys : undefined, preferredModel);
    } catch (err) {
      if (lastError) {
        c.status(429);
        return c.json({ error: { message: `All models rate-limited. Last error: ${lastError}`, type: 'rate_limit_error' } });
      }
      c.status(err instanceof Error && 'status' in err ? (err as any).status : 503);
      return c.json({ error: { message: err instanceof Error ? err.message : String(err), type: 'routing_error' } });
    }

    recordRequest(route.platform, route.modelId, route.keyId);

    try {
      if (stream) {
        return await handleStreamingCompletion(c, route, {
          messages,
          options: passthroughOptions,
          estimatedInputTokens,
          start,
          attempt
        });
      } else {
        return await handleStandardCompletion(c, route, {
          messages,
          options: passthroughOptions,
          attempt
        });
      }
    } catch (err: unknown) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      logRequest(route.platform, route.modelId, 'error', estimatedInputTokens, 0, Date.now() - start, errorMessage);

      if (isRetryableError(err)) {
        skipKeys.add(`${route.platform}:${route.modelId}:${route.keyId}`);
        setCooldown(route.platform, route.modelId, route.keyId, 120_000);
        recordRateLimitHit(route.modelDbId);
        lastError = errorMessage;
        console.log(`[Proxy] ${errorMessage.slice(0, 60)} from ${route.displayName}, falling back (${attempt + 1}/${MAX_RETRIES})`);
        continue;
      }

      // Non-retryable error structural failures
      c.status(502);
      return c.json({ error: { message: `Provider error (${route.displayName}): ${errorMessage}`, type: 'provider_error' } });
    }
  }

  // Exhausted Retries
  c.status(429);
  return c.json({ error: { message: `All models rate-limited after ${MAX_RETRIES} attempts. Last: ${lastError}`, type: 'rate_limit_error' } });
});
function logRequest(
  platform: string,
  modelId: string,
  status: string,
  inputTokens: number,
  outputTokens: number,
  latencyMs: number,
  error: string | null,
) {
  try {
    const db = getDb();
    db.query(`
      INSERT INTO requests (platform, model_id, status, input_tokens, output_tokens, latency_ms, error)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run([platform, modelId, status, inputTokens, outputTokens, latencyMs, error]);
  } catch (e) {
    console.error('Failed to log request:', e);
  }
}
