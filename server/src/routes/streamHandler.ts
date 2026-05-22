import { Context } from 'hono';
import type { ChatMessage } from '@freellmapi/shared/types.js';
import type { RouteResult } from '../services/router.js';
import { recordTokens } from '../services/ratelimit.js';
import { recordSuccess } from '../services/router.js';
// Handle SSE Stream Generation
export async function handleStreamingCompletion(
  c: Context,
  route: RouteResult,
  payload: { messages: ChatMessage[]; options: any; estimatedInputTokens: number; start: number; attempt: number }
) {
  let totalOutputTokens = 0;
  let streamStarted = false;

  try {
    const gen = route.provider.streamChatCompletion(
      route.apiKey,
      payload.messages,
      route.modelId,
      payload.options
    );

    // Set streaming headers cleanly
    c.header('Content-Type', 'text/event-stream');
    c.header('Cache-Control', 'no-cache');
    c.header('Connection', 'keep-alive');
    c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
    if (payload.attempt > 0) c.header('X-Fallback-Attempts', String(payload.attempt));

    for await (const chunk of gen) {
      streamStarted = true;
      const text = chunk.choices[0]?.delta?.content ?? '';
      totalOutputTokens += Math.ceil(text.length / 4);
      await c.body.write(`data: ${JSON.stringify(chunk)}\n\n`);
    }

    await c.body.write('data: [DONE]\n\n');
    await c.body.close();

    // Side-effects / Bookkeeping
    recordTokens(route.platform, route.modelId, route.keyId, payload.estimatedInputTokens + totalOutputTokens);
    recordSuccess(route.modelDbId);
    setStickyModel(payload.messages, route.modelDbId);
    console.log(route.platform, route.modelId, 'success', payload.estimatedInputTokens, totalOutputTokens, Date.now() - payload.start, null);

  } catch (streamErr: unknown) {
    const streamErrorMessage = streamErr instanceof Error ? streamErr.message : String(streamErr);

    if (streamStarted) {
      console.error(`[Proxy] Mid-stream error from ${route.displayName}:`, streamErrorMessage);
      const payloadErr = { error: { message: `Provider error (${route.displayName}): stream interrupted`, type: 'stream_error' } };
      try { await c.body.write(`data: ${JSON.stringify(payloadErr)}\n\n`); } catch {}
      try { await c.body.write('data: [DONE]\n\n'); await c.body.close(); } catch {}
      console.log(route.platform, route.modelId, 'error', payload.estimatedInputTokens, totalOutputTokens, Date.now() - payload.start, streamErrorMessage);
      return;
    }
    // Bubble up to outer retry block if it failed before starting
    throw streamErr;
  }
}

// Handle Standard Unary JSON response
export async function handleStandardCompletion(
  c: Context,
  route: RouteResult,
  payload: { messages: ChatMessage[]; options: any; attempt: number }
) {
  const result = await route.provider.chatCompletion(route.apiKey, payload.messages, route.modelId, payload.options);

  const totalTokens = result.usage?.total_tokens ?? 0;
  recordTokens(route.platform, route.modelId, route.keyId, totalTokens);
  recordSuccess(route.modelDbId);
  setStickyModel(payload.messages, route.modelDbId);

  c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
  if (payload.attempt > 0) c.header('X-Fallback-Attempts', String(payload.attempt));
  return c.json(result);
}
