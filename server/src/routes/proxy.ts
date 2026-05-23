import { Hono } from 'hono';
import type { ChatCompletionChunk } from '@freellmapi/shared/types.js';
import { routeRequest, recordRateLimitHit, recordSuccess, type RouteResult } from '../services/router.js';
import { apiKeyAuth, validateChatBody, normalizeMessages, estimateInputTokens, chatCompletionSchema, timingSafeStringEqual } from './middleware';
import { recordRequest, recordTokens, setCooldown, getStickyModel, setStickyModel } from '../services/ratelimit.js';
import { getDb } from '../db/index.js';
import { handleStreamingCompletion, handleStandardCompletion } from './streamHandler.js';
import type { CompletionOptions } from '../providers/base.js';

// Virtual "auto" model. Clients like Hermes require a non-empty `model` field
// on every request, but freellmapi's whole point is to pick the model itself.
// Requesting this id means "let the router decide" — identical to omitting
// `model` entirely.
const AUTO_MODEL_ID = 'auto';

function isAutoModel(modelId: string | undefined): boolean {
  return modelId === AUTO_MODEL_ID;
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
  const data = c.get('parsedData');

  const { model: requestedModel, stream, messages: rawMessages, ...passthroughOptions } = data;
  const messages = normalizeMessages(rawMessages);

  const estimatedInputTokens = estimateInputTokens(messages);
  const estimatedTotal = estimatedInputTokens + (passthroughOptions.max_tokens ?? 1000);

  // Resolve preferred model: use sticky session for multi-turn, auto for new conversations
  const preferredModel = isAutoModel(requestedModel) ? getStickyModel(messages) : undefined;

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
