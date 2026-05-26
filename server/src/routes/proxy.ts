import { Hono } from 'hono';
import { z } from 'zod';
import { routeRequest, recordRateLimitHit, recordSuccess, type RouteResult } from '../services/router.js';
import { apiKeyAuth, validateChatBody, validateCompletionBody, normalizeMessages, estimateInputTokens, chatCompletionSchema, completionSchema } from './middleware';
import { recordRequest, recordTokens, setCooldown, getStickyModel, setStickyModel } from '../services/ratelimit.js';
import { getDb } from '../db/index.js';
import { handleStreamingCompletion, handleStandardCompletion } from './streamHandler.js';
import { handleCompletion } from './completions.js';
import type { StatusCode } from 'hono/utils/http-status';
import type { CompletionRequest } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { eq, asc, and, sql } from 'drizzle-orm';

declare module 'hono' {
  interface ContextVariableMap {
    parsedData: z.infer<typeof chatCompletionSchema> | z.infer<typeof completionSchema>;
  }
}

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
  const models = db.select({
    platform: schema.models.platform,
    model_id: schema.models.modelId,
    display_name: schema.models.displayName,
    context_window: schema.models.contextWindow
  })
  .from(schema.models)
  .where(eq(schema.models.enabled, 1))
  .orderBy(schema.models.intelligenceRank)
  .all();

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

const MAX_RETRIES = 30;
const extractStatus = (err: unknown): StatusCode => {
  if (err instanceof Error && 'status' in err) {
    const statusValue = (err as { status: unknown }).status;
    const parsed = Number(statusValue);

    // Garante que o número está no range válido de HTTP Status Codes
    if (!isNaN(parsed) && parsed >= 100 && parsed < 600) {
      return parsed as StatusCode;
    }
  }
  return 503; // Fallback padrão seguro
};
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
  const data = c.get('parsedData') as z.infer<typeof chatCompletionSchema>;

  const { model: requestedModel, stream, messages: rawMessages, n = 1, ...rest } = data;
  const passthroughOptions = rest as typeof rest & { stream_options?: { include_usage?: boolean } }; 
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
      c.status(extractStatus(err));
      return c.json({ error: { message: err instanceof Error ? err.message : String(err), type: 'routing_error' } });
    }

    try {
      if (stream) {
        const result = await handleStreamingCompletion(c, route, {
          messages,
          options: passthroughOptions,
          estimatedInputTokens,
          start,
          attempt
        });
        recordRequest(route.platform, route.modelId, route.keyId);
        return result;
      } else if (n > 1) {
        const results = await Promise.all(
          Array.from({ length: n }, () =>
            route.provider.chatCompletion(route.apiKey, messages, route.modelId, passthroughOptions)
          )
        );
        const mergedChoices = results.flatMap((r, i) =>
          r.choices.map(ch => ({ ...ch, index: ch.index + i * r.choices.length }))
        );
        const totalUsage = results.reduce(
          (acc, r) => {
            acc.prompt_tokens += r.usage?.prompt_tokens ?? 0;
            acc.completion_tokens += r.usage?.completion_tokens ?? 0;
            acc.total_tokens += r.usage?.total_tokens ?? 0;
            return acc;
          },
          { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }
        );

        recordTokens(route.platform, route.modelId, route.keyId, totalUsage.total_tokens);
        recordSuccess(route.modelDbId);
        setStickyModel(messages, route.modelDbId);
        recordRequest(route.platform, route.modelId, route.keyId);

        c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
        if (attempt > 0) c.header('X-Fallback-Attempts', String(attempt));
        return c.json({
          id: results[0]!.id,
          object: 'chat.completion',
          created: Math.floor(Date.now() / 1000),
          model: route.modelId,
          choices: mergedChoices,
          usage: totalUsage,
        });
      } else {
        const result = await handleStandardCompletion(c, route, {
          messages,
          options: passthroughOptions,
          attempt
        });
        recordRequest(route.platform, route.modelId, route.keyId);
        return result;
      }
     } catch (err: unknown) {
       const errorMessage = err instanceof Error ? err.message : String(err);
       logRequest(route.platform, route.modelId, 'error', estimatedInputTokens, 0, Date.now() - start, errorMessage);

       if (isRetryableError(err)) {
         skipKeys.add(`${route.platform}:${route.modelId}:${route.keyId}`);
         setCooldown(route.platform, route.modelId, route.keyId, 600_000);
         recordRateLimitHit(route.modelDbId);
         lastError = errorMessage;
         console.log(`[Proxy] ${errorMessage.slice(0, 60)} from ${route.displayName}, falling back (${attempt + 1}/${MAX_RETRIES})`);
         continue;
       }

       // Check if this is a 404 error indicating model no longer available
       const isModelNotFound = errorMessage.includes('404') &&
         (errorMessage.includes('no longer available') ||
          errorMessage.includes('paid model') ||
          errorMessage.includes('not found') ||
          errorMessage.includes('model.*not found'));

       if (isModelNotFound) {
         // Mark the key as invalid for this specific model since it's no longer available
         try {
           const db = getDb();
           db.update(schema.apiKeys)
             .set({ status: 'invalid', lastCheckedAt: sql`(datetime('now'))` })
             .where(and(
               eq(schema.apiKeys.platform, route.platform),
               eq(schema.apiKeys.id, route.keyId)
             ))
             .run();

           console.log(`[Proxy] Marked key ${route.keyId} (${route.platform}:${route.modelId}) as invalid due to 404: ${errorMessage}`);
         } catch (dbError) {
           console.error('[Proxy] Failed to update key status:', dbError);
         }

         // Treat as a provider error but don't retry since the model is genuinely unavailable
         c.status(502);
         return c.json({ error: { message: `Provider error (${route.displayName}): ${errorMessage}`, type: 'provider_error' } });
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

// Legacy /v1/completions endpoint — wraps chat completions
proxyRouter.post('/completions', apiKeyAuth, validateCompletionBody, async (c) => {
  const data = c.get('parsedData') as z.infer<typeof completionSchema>;
  return handleCompletion(c, data);
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
    db.insert(schema.requests).values({
      platform,
      modelId,
      status,
      inputTokens,
      outputTokens,
      latencyMs,
      error
    }).run();
  } catch (e) {
    console.error('Failed to log request:', e);
  }
}
