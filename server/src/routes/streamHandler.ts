import type { Context } from 'hono';
import { stream } from 'hono/streaming';
import type { ChatMessage, ChatCompletionChunk } from '@freellmapi/shared/types.js';
import type { RouteResult } from '../services/router.js';
import type { CompletionOptions } from '../providers/base.js';
import { recordTokens, setStickyModel } from '../services/ratelimit.js';
import { recordSuccess } from '../services/router.js';

// Handle SSE Stream Generation
export async function handleStreamingCompletion(
  c: Context,
  route: RouteResult,
  payload: { messages: ChatMessage[]; options?: CompletionOptions; estimatedInputTokens: number; start: number; attempt: number }
): Promise<Response> {
  const gen = route.provider.streamChatCompletion(
    route.apiKey,
    payload.messages,
    route.modelId,
    payload.options
  );

  const iterator = gen[Symbol.asyncIterator]();
  const includeUsage = payload.options?.stream_options?.include_usage ?? false;

  // Pre-stream handshake: try to get the first chunk synchronously before the
  // Hono stream starts. If this throws, the error propagates to the retry loop
  // in proxy.ts so it can fail over to the next model.
  let firstChunk: ChatCompletionChunk | undefined;
  let streamId: string | undefined;
  try {
    const result = await iterator.next();
    if (!result.done) {
      firstChunk = result.value;
      streamId = result.value.id;
    }
  } catch (err) {
    // Pre-stream error — bubble up for retry
    throw err;
  }

  c.header('Content-Type', 'text/event-stream');
  c.header('Cache-Control', 'no-cache');
  c.header('Connection', 'keep-alive');
  c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
  if (payload.attempt > 0) c.header('X-Fallback-Attempts', String(payload.attempt));

  let totalOutputTokens = 0;

  // Fixed: Call stream(c, async callback) instead of c.stream()
  return stream(c, async (streamInstance) => {
    let streamStarted = false;

    try {
      // Write the pre-fetched first chunk (if any)
      if (firstChunk) {
        streamStarted = true;
        const text = firstChunk.choices[0]?.delta?.content ?? '';
        totalOutputTokens += Math.ceil(text.length / 4);
        await streamInstance.write(`data: ${JSON.stringify(firstChunk)}\n\n`);
      }

      // Write remaining chunks
      for await (const chunk of iterator) {
        const text = chunk.choices[0]?.delta?.content ?? '';
        totalOutputTokens += Math.ceil(text.length / 4);
        await streamInstance.write(`data: ${JSON.stringify(chunk)}\n\n`);
      }

      // Emit usage in final chunk if requested (OpenAI compatibility)
      const finalChunk: ChatCompletionChunk = {
        id: streamId ?? firstChunk?.id ?? `chatcmpl-${Date.now()}`,
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: route.modelId,
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
        ...(includeUsage ? {
          usage: {
            prompt_tokens: payload.estimatedInputTokens,
            completion_tokens: totalOutputTokens,
            total_tokens: payload.estimatedInputTokens + totalOutputTokens,
          }
        } : {})
      };
      await streamInstance.write(`data: ${JSON.stringify(finalChunk)}\n\n`);
      await streamInstance.write('data: [DONE]\n\n');

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
        try { await streamInstance.write(`data: ${JSON.stringify(payloadErr)}\n\n`); } catch {}
        try { await streamInstance.write('data: [DONE]\n\n'); } catch {}
        console.log(route.platform, route.modelId, 'error', payload.estimatedInputTokens, totalOutputTokens, Date.now() - payload.start, streamErrorMessage);
        return;
      }
      // Pre-stream errors are caught by the outer handshake; this should not be reached
      throw streamErr;
    }
  });
}

// Handle Standard Unary JSON response
export async function handleStandardCompletion(
  c: Context,
  route: RouteResult,
  payload: { messages: ChatMessage[]; options?: CompletionOptions; attempt: number }
): Promise<Response> {
  const result = await route.provider.chatCompletion(route.apiKey, payload.messages, route.modelId, payload.options);

  const totalTokens = result.usage?.total_tokens ?? 0;
  recordTokens(route.platform, route.modelId, route.keyId, totalTokens);
  recordSuccess(route.modelDbId);
  setStickyModel(payload.messages, route.modelDbId);

  c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
  if (payload.attempt > 0) c.header('X-Fallback-Attempts', String(payload.attempt));
  return c.json(result);
}
