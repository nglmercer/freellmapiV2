import type { Context } from 'hono';
import { stream } from 'hono/streaming';
import type {
  ChatMessage,
  CompletionRequest,
  CompletionResponse,
  CompletionChunk,
  Platform,
} from '@freellmapi/shared/types.js';
import type { RouteResult } from '../services/router.js';
import { routeRequest, recordRateLimitHit, recordSuccess } from '../services/router.js';
import { recordRequest, recordTokens, setCooldown, setStickyModel } from '../services/ratelimit.js';
import { estimateInputTokens } from './middleware.js';

const MAX_RETRIES = 30;

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

function buildChatMessages(prompt: string, suffix?: string): ChatMessage[] {
  const content = suffix ? `${prompt} ${suffix}` : prompt;
  return [{ role: 'user', content }];
}

async function runSingleCompletion(
  route: RouteResult,
  prompt: string,
  suffix: string | undefined,
  options: Omit<CompletionRequest, 'prompt' | 'model'>,
): Promise<{ text: string; usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number } }> {
  const messages = buildChatMessages(prompt, suffix);
  const result = await route.provider.chatCompletion(
    route.apiKey,
    messages,
    route.modelId,
    {
      temperature: options.temperature,
      max_tokens: options.max_tokens,
      top_p: options.top_p,
      seed: options.seed,
      frequency_penalty: options.frequency_penalty,
      presence_penalty: options.presence_penalty,
      user: options.user,
      logprobs: options.logprobs != null ? true : undefined,
      top_logprobs: options.logprobs ?? undefined,
    },
  );

  const rawContent = result.choices[0]?.message.content ?? '';
  const text = typeof rawContent === 'string' ? rawContent : '';
  const usage = result.usage ?? { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
  return { text, usage };
}

export async function handleCompletion(
  c: Context,
  data: CompletionRequest,
): Promise<Response> {
  const start = Date.now();
  const { model: requestedModel, prompt: prompts, suffix, stream: doStream, ...rest } = data;

  const estimatedInputTokens = (prompts as string[]).reduce((s: number, p: string) => s + estimateInputTokens([{ role: 'user', content: p }]), 0);
  const estimatedTotal = estimatedInputTokens + (rest.max_tokens ?? 16);
  const n = Math.min(rest.n ?? 1, (prompts as string[]).length);
  const echo = rest.echo ?? false;

  if (doStream) {
    return handleCompletionStream(c, data, prompts as string[], suffix, echo, n, estimatedInputTokens, estimatedTotal, start);
  }

  return handleCompletionStandard(c, data, prompts as string[], suffix, echo, n, estimatedInputTokens, estimatedTotal, start);
}

async function handleCompletionStandard(
  c: Context,
  data: CompletionRequest,
  prompts: string[],
  suffix: string | undefined,
  echo: boolean,
  n: number,
  estimatedInputTokens: number,
  estimatedTotal: number,
  start: number,
): Promise<Response> {
  const { model: requestedModel, ...rest } = data;
  const skipKeys = new Set<string>();
  let lastError: string | null = null;

  for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
    let route: RouteResult;
    try {
      route = routeRequest(estimatedTotal, skipKeys.size > 0 ? skipKeys : undefined, undefined);
    } catch (err) {
      c.status(429);
      return c.json({ error: { message: `All models rate-limited. Last error: ${lastError}`, type: 'rate_limit_error' } });
    }

    try {
      const results = await Promise.all(
        prompts.slice(0, n).map((prompt) =>
          runSingleCompletion(route, prompt, suffix, rest),
        ),
      );

      const finalTexts = results.map(r => echo ? prompts[results.indexOf(r)] + r.text : r.text);

      const totalInputTokens = results.reduce((s, r) => s + r.usage.prompt_tokens, 0);
      const totalOutputTokens = results.reduce((s, r) => s + r.usage.completion_tokens, 0);
      const totalTokens = results.reduce((s, r) => s + r.usage.total_tokens, 0);

      recordTokens(route.platform, route.modelId, route.keyId, totalTokens);
      recordSuccess(route.modelDbId);
      recordRequest(route.platform, route.modelId, route.keyId);

      const id = `cmpl-${Date.now()}`;
      const response: CompletionResponse = {
        id,
        object: 'text_completion',
        created: Math.floor(Date.now() / 1000),
        model: route.modelId,
        choices: finalTexts.map((text, i) => ({
          text,
          index: i,
          logprobs: null,
          finish_reason: 'stop',
        })),
        usage: {
          prompt_tokens: totalInputTokens,
          completion_tokens: totalOutputTokens,
          total_tokens: totalTokens,
        },
        _routed_via: { platform: route.platform as Platform, model: route.modelId },
      };

      c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
      if (attempt > 0) c.header('X-Fallback-Attempts', String(attempt));
      return c.json(response);
    } catch (err) {
      lastError = err instanceof Error ? err.message : String(err);

      if (isRetryableError(err)) {
        skipKeys.add(`${route.platform}:${route.modelId}:${route.keyId}`);
        setCooldown(route.platform, route.modelId, route.keyId, 600_000);
        recordRateLimitHit(route.modelDbId);
        console.log(`[Completions] ${lastError.slice(0, 60)} from ${route.displayName}, falling back (${attempt + 1}/${MAX_RETRIES})`);
        continue;
      }

      c.status(502);
      return c.json({ error: { message: `Provider error (${route.displayName}): ${lastError}`, type: 'provider_error' } });
    }
  }

  c.status(429);
  return c.json({ error: { message: `All models rate-limited after ${MAX_RETRIES} attempts. Last: ${lastError}`, type: 'rate_limit_error' } });
}

async function handleCompletionStream(
  c: Context,
  data: CompletionRequest,
  prompts: string[],
  suffix: string | undefined,
  echo: boolean,
  n: number,
  estimatedInputTokens: number,
  estimatedTotal: number,
  start: number,
): Promise<Response> {
  const { model: requestedModel, ...rest } = data;
  const skipKeys = new Set<string>();
  let lastError: string | null = null;

  for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
    let route: RouteResult;
    try {
      route = routeRequest(estimatedTotal, skipKeys.size > 0 ? skipKeys : undefined, undefined);
    } catch (err) {
      c.status(429);
      return c.json({ error: { message: `All models rate-limited. Last error: ${lastError}`, type: 'rate_limit_error' } });
    }

    const prompt = prompts[0]!;
    const messages = buildChatMessages(prompt, suffix ?? undefined);
    const gen = route.provider.streamChatCompletion(route.apiKey, messages, route.modelId, {
      temperature: rest.temperature,
      max_tokens: rest.max_tokens,
      top_p: rest.top_p,
      seed: rest.seed,
      frequency_penalty: rest.frequency_penalty,
      presence_penalty: rest.presence_penalty,
      user: rest.user,
      logprobs: rest.logprobs != null ? true : undefined,
      top_logprobs: rest.logprobs ?? undefined,
      stream_options: rest.stream_options,
    });

    const iterator = gen[Symbol.asyncIterator]();

    let firstChunk: { id: string; content: string; finish_reason: string | null } | undefined;
    try {
      const result = await iterator.next();
      if (!result.done) {
        const chunk = result.value;
        firstChunk = {
          id: chunk.id,
          content: chunk.choices[0]?.delta?.content ?? '',
          finish_reason: chunk.choices[0]?.finish_reason ?? null,
        };
      }
    } catch (err) {
      lastError = err instanceof Error ? err.message : String(err);
      if (isRetryableError(err)) {
        skipKeys.add(`${route.platform}:${route.modelId}:${route.keyId}`);
        setCooldown(route.platform, route.modelId, route.keyId, 600_000);
        recordRateLimitHit(route.modelDbId);
        continue;
      }
      throw err;
    }

    c.header('Content-Type', 'text/event-stream');
    c.header('Cache-Control', 'no-cache');
    c.header('Connection', 'keep-alive');
    c.header('X-Routed-Via', `${route.platform}/${route.modelId}`);
    if (attempt > 0) c.header('X-Fallback-Attempts', String(attempt));

    let totalOutputTokens = 0;
    const includeUsage = rest.stream_options?.include_usage ?? false;

    return stream(c, async (streamInstance) => {
      let streamStarted = false;

      try {
        if (firstChunk) {
          streamStarted = true;
          const text = echo ? prompt + firstChunk.content : firstChunk.content;
          totalOutputTokens += Math.ceil(firstChunk.content.length / 4);
          const cc: CompletionChunk = {
            id: firstChunk.id,
            object: 'text_completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: route.modelId,
            choices: [{
              text,
              index: 0,
              logprobs: null,
              finish_reason: firstChunk.finish_reason,
            }],
          };
          await streamInstance.write(`data: ${JSON.stringify(cc)}\n\n`);
        }

        for await (const chunk of iterator) {
          const content = chunk.choices[0]?.delta?.content ?? '';
          totalOutputTokens += Math.ceil(content.length / 4);
          const cc: CompletionChunk = {
            id: chunk.id,
            object: 'text_completion.chunk',
            created: chunk.created,
            model: route.modelId,
            choices: [{
              text: content,
              index: 0,
              logprobs: null,
              finish_reason: chunk.choices[0]?.finish_reason ?? null,
            }],
          };
          await streamInstance.write(`data: ${JSON.stringify(cc)}\n\n`);
        }

        const finalChunk: CompletionChunk = {
          id: firstChunk?.id ?? `cmpl-${Date.now()}`,
          object: 'text_completion.chunk',
          created: Math.floor(Date.now() / 1000),
          model: route.modelId,
          choices: [{ text: '', index: 0, logprobs: null, finish_reason: 'stop' }],
          ...(includeUsage ? {
            usage: {
              prompt_tokens: estimatedInputTokens,
              completion_tokens: totalOutputTokens,
              total_tokens: estimatedInputTokens + totalOutputTokens,
            },
          } : {}),
        };
        await streamInstance.write(`data: ${JSON.stringify(finalChunk)}\n\n`);
        await streamInstance.write('data: [DONE]\n\n');

        recordTokens(route.platform, route.modelId, route.keyId, estimatedInputTokens + totalOutputTokens);
        recordSuccess(route.modelDbId);
        recordRequest(route.platform, route.modelId, route.keyId);
      } catch (streamErr) {
        const msg = streamErr instanceof Error ? streamErr.message : String(streamErr);
        if (streamStarted) {
          console.error(`[Completions] Mid-stream error from ${route.displayName}:`, msg);
          try { await streamInstance.write(`data: ${JSON.stringify({ error: { message: `stream interrupted`, type: 'stream_error' } })}\n\n`); } catch {}
          try { await streamInstance.write('data: [DONE]\n\n'); } catch {}
          return;
        }
        throw streamErr;
      }
    });
  }

  c.status(429);
  return c.json({ error: { message: `All models rate-limited after ${MAX_RETRIES} attempts. Last: ${lastError}`, type: 'rate_limit_error' } });
}
