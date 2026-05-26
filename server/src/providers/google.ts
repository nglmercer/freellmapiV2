import type {
  ChatMessage,
  ChatCompletionResponse,
  ChatCompletionChunk,
  ChatToolCall,
  ChatToolChoice,
  ChatToolDefinition,
  TokenUsage,
  ContentPart,
} from '@freellmapi/shared/types.js';
import { BaseProvider, type CompletionOptions } from './base.js';

const API_BASE = 'https://generativelanguage.googleapis.com/v1beta';

interface GeminiPart {
  text?: string;
  inlineData?: {
    mimeType: string;
    data: string;
  };
  fileData?: {
    mimeType: string;
    fileUri: string;
  };
  thoughtSignature?: string;
  functionCall?: {
    id?: string;
    name?: string;
    args?: unknown;
  };
  functionResponse?: {
    id?: string;
    name?: string;
    response?: unknown;
  };
}

interface GeminiCandidate {
  content?: { parts?: GeminiPart[] };
  finishReason?: string;
}

interface GeminiResponse {
  candidates?: GeminiCandidate[];
  usageMetadata?: {
    promptTokenCount?: number;
    candidatesTokenCount?: number;
    totalTokenCount?: number;
  };
}

function safeParseObject(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
    return { value: parsed };
  } catch {
    return { value: raw };
  }
}

function normalizeGeminiArgs(args: unknown): string {
  if (typeof args === 'string') return args;
  return JSON.stringify(args ?? {});
}

function toGeminiFinishReason(finishReason?: string): string {
  const r = (finishReason ?? '').toUpperCase();
  if (!r) return 'stop';
  if (r === 'MAX_TOKENS') return 'length';
  if (r === 'SAFETY' || r === 'RECITATION' || r === 'BLOCKLIST' || r === 'PROHIBITED_CONTENT' || r === 'SPII') {
    return 'content_filter';
  }
  return 'stop';
}

function toGeminiTools(tools?: ChatToolDefinition[]): Array<{ functionDeclarations: Array<Record<string, unknown>> }> | undefined {
  if (!tools || tools.length === 0) return undefined;

  return [{
    functionDeclarations: tools.map(t => ({
      name: t.function.name,
      description: t.function.description,
      parameters: t.function.parameters,
    })),
  }];
}

function toGeminiToolConfig(toolChoice?: ChatToolChoice): { functionCallingConfig: Record<string, unknown> } | undefined {
  if (!toolChoice) return undefined;

  if (typeof toolChoice === 'string') {
    const mode =
      toolChoice === 'none'
        ? 'NONE'
        : toolChoice === 'required'
          ? 'ANY'
          : 'AUTO';
    return { functionCallingConfig: { mode } };
  }

  return {
    functionCallingConfig: {
      mode: 'ANY',
      allowedFunctionNames: [toolChoice.function.name],
    },
  };
}

// Translate OpenAI messages to Gemini format
function toGeminiContents(messages: ChatMessage[]) {
  const systemMessages = messages
    .filter(m => m.role === 'system' && typeof m.content === 'string' && m.content.length > 0)
    .map(m => m.content as string);

  const toolNameByCallId = new Map<string, string>();
  for (const m of messages) {
    for (const tc of m.tool_calls ?? []) {
      toolNameByCallId.set(tc.id, tc.function.name);
    }
  }

  const contents = messages
    .filter(m => m.role !== 'system')
    .map((m): { role: 'user' | 'model'; parts: GeminiPart[] } | null => {
      if (m.role === 'assistant') {
        const parts: GeminiPart[] = [];

        if (typeof m.content === 'string' && m.content.length > 0) {
          parts.push({ text: m.content });
        }

        for (const call of m.tool_calls ?? []) {
          parts.push({
            thoughtSignature: call.thought_signature,
            functionCall: {
              id: call.id,
              name: call.function.name,
              args: safeParseObject(call.function.arguments),
            },
          });
        }

        if (parts.length === 0) return null;
        return {
          role: 'model',
          parts,
        };
      }

      if (m.role === 'tool') {
        const toolCallId = m.tool_call_id;
        if (!toolCallId) return null;

        const toolName = m.name ?? toolNameByCallId.get(toolCallId) ?? 'tool';
        const response = safeParseObject(typeof m.content === 'string' ? m.content : '');

        return {
          role: 'user',
          parts: [{
            functionResponse: {
              id: toolCallId,
              name: toolName,
              response,
            },
          }],
        };
      }

      return {
        role: 'user',
        parts: contentToGeminiParts(m.content),
      };
    })
    .filter((entry): entry is { role: 'user' | 'model'; parts: GeminiPart[] } => entry !== null);

  return {
    contents,
    systemInstruction: systemMessages.length > 0
      ? { parts: [{ text: systemMessages.join('\n\n') }] }
      : undefined,
  };
}

function contentToGeminiParts(content: string | ContentPart[] | null): GeminiPart[] {
  if (typeof content === 'string') return [{ text: content }];
  if (Array.isArray(content)) {
    const parts: GeminiPart[] = [];
    for (const part of content) {
      if (part.type === 'text' && typeof part.text === 'string') {
        parts.push({ text: part.text });
      } else if (part.type === 'image_url' && part.image_url) {
        const url = part.image_url.url;
        if (url.startsWith('data:')) {
          const comma = url.indexOf(',');
          const header = url.slice(0, comma);
          const mimeMatch = header.match(/data:(image\/\w+);base64/);
          const mimeType = mimeMatch?.[1] ?? 'image/jpeg';
          const data = url.slice(comma + 1);
          parts.push({ inlineData: { mimeType, data } });
        } else {
          const mimeType = guessMimeType(url);
          parts.push({ fileData: { mimeType, fileUri: url } });
        }
      }
    }
    return parts.length > 0 ? parts : [{ text: '' }];
  }
  return [{ text: '' }];
}

function guessMimeType(url: string): string {
  const ext = url.split('.').pop()?.toLowerCase() ?? '';
  const mimeMap: Record<string, string> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
    bmp: 'image/bmp',
    svg: 'image/svg+xml',
  };
  return mimeMap[ext] ?? 'image/png';
}

function extractToolCalls(parts: GeminiPart[] | undefined): ChatToolCall[] {
  const calls: ChatToolCall[] = [];
  if (!parts) return calls;

  let fallbackIndex = 0;
  for (const part of parts) {
    if (!part.functionCall?.name) continue;

    const id = part.functionCall.id ?? `call_${Date.now()}_${fallbackIndex++}`;
    calls.push({
      id,
      type: 'function',
      function: {
        name: part.functionCall.name,
        arguments: normalizeGeminiArgs(part.functionCall.args),
      },
      thought_signature: part.thoughtSignature,
    });
  }

  return calls;
}

function extractText(parts: GeminiPart[] | undefined): string | null {
  if (!parts) return null;
  const text = parts
    .map(p => p.text ?? '')
    .join('');
  return text.length > 0 ? text : null;
}

export class GoogleProvider extends BaseProvider {
  readonly platform = 'google' as const;
  readonly name = 'Google AI Studio';

  async chatCompletion(
    apiKey: string,
    messages: ChatMessage[],
    modelId: string,
    options?: CompletionOptions,
  ): Promise<ChatCompletionResponse> {
    const { contents, systemInstruction } = toGeminiContents(messages);

    const generationConfig: Record<string, unknown> = {
      temperature: options?.temperature,
      maxOutputTokens: options?.max_tokens,
      topP: options?.top_p,
    };
    if (options?.seed !== undefined) generationConfig.seed = options.seed;
    if (options?.frequency_penalty !== undefined) generationConfig.frequencyPenalty = options.frequency_penalty;
    if (options?.presence_penalty !== undefined) generationConfig.presencePenalty = options.presence_penalty;

    const body: Record<string, unknown> = {
      contents,
      generationConfig,
      tools: toGeminiTools(options?.tools),
      toolConfig: toGeminiToolConfig(options?.tool_choice),
    };
    if (systemInstruction) body.systemInstruction = systemInstruction;
    if (options?.response_format?.type === 'json_object' || options?.response_format?.type === 'json_schema') {
      body.responseSchema = options.response_format.json_schema ?? { type: 'OBJECT' };
    }

    const url = `${API_BASE}/models/${modelId}:generateContent`;
    const res = await this.fetchWithTimeout(url, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(`Google API error ${res.status}: ${(err as any).error?.message ?? res.statusText}`);
    }

    const data = await res.json() as GeminiResponse;
    const candidate = data.candidates?.[0];
    const parts = candidate?.content?.parts;
    const toolCalls = extractToolCalls(parts);
    const text = extractText(parts);

    const usage: TokenUsage = {
      prompt_tokens: data.usageMetadata?.promptTokenCount ?? 0,
      completion_tokens: data.usageMetadata?.candidatesTokenCount ?? 0,
      total_tokens: data.usageMetadata?.totalTokenCount ?? 0,
    };

    return {
      id: this.makeId(),
      object: 'chat.completion',
      created: Math.floor(Date.now() / 1000),
      model: modelId,
      choices: [{
        index: 0,
        message: {
          role: 'assistant',
          content: text,
          ...(toolCalls.length > 0 ? { tool_calls: toolCalls } : {}),
        },
        finish_reason: toolCalls.length > 0 ? 'tool_calls' : toGeminiFinishReason(candidate?.finishReason),
      }],
      usage,
      _routed_via: { platform: 'google', model: modelId },
    };
  }

  async *streamChatCompletion(
    apiKey: string,
    messages: ChatMessage[],
    modelId: string,
    options?: CompletionOptions,
  ): AsyncGenerator<ChatCompletionChunk> {
    const { contents, systemInstruction } = toGeminiContents(messages);

    const generationConfig: Record<string, unknown> = {
      temperature: options?.temperature,
      maxOutputTokens: options?.max_tokens,
      topP: options?.top_p,
    };
    if (options?.seed !== undefined) generationConfig.seed = options.seed;
    if (options?.frequency_penalty !== undefined) generationConfig.frequencyPenalty = options.frequency_penalty;
    if (options?.presence_penalty !== undefined) generationConfig.presencePenalty = options.presence_penalty;

    const body: Record<string, unknown> = {
      contents,
      generationConfig,
      tools: toGeminiTools(options?.tools),
      toolConfig: toGeminiToolConfig(options?.tool_choice),
    };
    if (systemInstruction) body.systemInstruction = systemInstruction;
    if (options?.response_format?.type === 'json_object' || options?.response_format?.type === 'json_schema') {
      body.responseSchema = options.response_format.json_schema ?? { type: 'OBJECT' };
    }

    const url = `${API_BASE}/models/${modelId}:streamGenerateContent?alt=sse`;
    const res = await this.fetchWithTimeout(url, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      throw new Error(`Google API error ${res.status}: ${(err as any).error?.message ?? res.statusText}`);
    }

    const reader = res.body?.getReader();
    if (!reader) throw new Error('No response body');

    const decoder = new TextDecoder();
    const id = this.makeId();
    let buffer = '';
    let emittedFinish = false;
    let sawToolCalls = false;

    const seenToolCallKeys = new Set<string>();

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() ?? '';

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed || !trimmed.startsWith('data: ')) continue;
        const raw = trimmed.slice(6);
        if (raw === '[DONE]') {
          if (!emittedFinish) {
            emittedFinish = true;
            yield {
              id,
              object: 'chat.completion.chunk',
              created: Math.floor(Date.now() / 1000),
              model: modelId,
              choices: [{
                index: 0,
                delta: {},
                finish_reason: sawToolCalls ? 'tool_calls' : 'stop',
              }],
            };
          }
          return;
        }

        // Skip malformed SSE frames instead of aborting the whole stream.
        // Matches the defensive parse in openai-compat / cohere / cloudflare:
        // a single corrupt chunk shouldn't take down the rest of the response.
        let chunk: GeminiResponse;
        try {
          chunk = JSON.parse(raw) as GeminiResponse;
        } catch {
          continue;
        }
        const candidate = chunk.candidates?.[0];
        const parts = candidate?.content?.parts ?? [];

        const text = extractText(parts);
        const toolCalls = extractToolCalls(parts).filter(call => {
          const key = `${call.id}:${call.function.name}:${call.function.arguments}`;
          if (seenToolCallKeys.has(key)) return false;
          seenToolCallKeys.add(key);
          return true;
        });

        if ((text && text.length > 0) || toolCalls.length > 0) {
          sawToolCalls = sawToolCalls || toolCalls.length > 0;
          yield {
            id,
            object: 'chat.completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: modelId,
            choices: [{
              index: 0,
              delta: {
                ...(text ? { content: text } : {}),
                ...(toolCalls.length > 0 ? { tool_calls: toolCalls } : {}),
              },
              finish_reason: null,
            }],
          };
        }

        if (candidate?.finishReason && !emittedFinish) {
          emittedFinish = true;
          yield {
            id,
            object: 'chat.completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: modelId,
            choices: [{
              index: 0,
              delta: {},
              finish_reason: sawToolCalls ? 'tool_calls' : toGeminiFinishReason(candidate.finishReason),
            }],
          };
          return;
        }
      }
    }

    if (!emittedFinish) {
      yield {
        id,
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: modelId,
        choices: [{
          index: 0,
          delta: {},
          finish_reason: sawToolCalls ? 'tool_calls' : 'stop',
        }],
      };
    }
  }

   async validateKey(apiKey: string): Promise<boolean> {
     // Transport errors propagate — health.ts marks status='error' without
     // counting toward auto-disable. Only confirmed 401/403 disables a key.
     const res = await this.fetchWithTimeout(`${API_BASE}/models`, {
       method: 'GET',
       headers: { 'Authorization': `Bearer ${apiKey}` },
     }, 10000);
     return res.status !== 401 && res.status !== 403;
   }
}
