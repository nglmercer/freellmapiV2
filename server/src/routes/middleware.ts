import { type Context, type Next } from 'hono';
import type { ChatMessage, ContentPart } from '@freellmapi/shared/types.js';
import { z } from 'zod';

import { getUnifiedApiKey } from '../db/index.js';
import crypto from 'crypto';

export const toolCallSchema = z.object({
  id: z.string().min(1),
  type: z.literal('function'),
  function: z.object({
    name: z.string().min(1),
    arguments: z.string(),
  }),
  thought_signature: z.string().optional(),
});

const contentPartSchema = z.object({
  type: z.string(),
  text: z.string().optional(),
  image_url: z.object({ url: z.string(), detail: z.string().optional() }).optional(),
});

const contentSchema = z.union([z.string(), z.array(contentPartSchema)]);

export const systemMessageSchema = z.object({
  role: z.literal('system'),
  content: contentSchema,
  name: z.string().optional(),
});

export const userMessageSchema = z.object({
  role: z.literal('user'),
  content: contentSchema,
  name: z.string().optional(),
});

export const assistantMessageSchema = z.object({
  role: z.literal('assistant'),
  content: contentSchema.nullable().optional(),
  name: z.string().optional(),
  tool_calls: z.array(toolCallSchema).optional(),
}).refine((msg) => {
  const hasContent = typeof msg.content === 'string' && msg.content.length > 0;
  const hasArrayContent = Array.isArray(msg.content) && msg.content.length > 0;
  const hasToolCalls = (msg.tool_calls?.length ?? 0) > 0;
  return hasContent || hasArrayContent || hasToolCalls;
}, {
  message: 'assistant messages must include non-empty content or tool_calls',
});

export const toolMessageSchema = z.object({
  role: z.literal('tool'),
  content: contentSchema,
  tool_call_id: z.string().min(1),
  name: z.string().optional(),
});

export const toolDefinitionSchema = z.object({
  type: z.literal('function'),
  function: z.object({
    name: z.string().min(1),
    description: z.string().optional(),
    parameters: z.record(z.string(), z.unknown()).optional(),
    strict: z.boolean().optional(),
  }),
});

export const toolChoiceSchema = z.union([
  z.enum(['none', 'auto', 'required']),
  z.object({
    type: z.literal('function'),
    function: z.object({
      name: z.string().min(1),
    }),
  }),
]);
export const streamOptionsSchema = z.object({
  include_usage: z.boolean().optional(),
});

export const chatCompletionSchema = z.object({
  messages: z.array(z.union([
    systemMessageSchema,
    userMessageSchema,
    assistantMessageSchema,
    toolMessageSchema,
  ])).min(1),
  model: z.string().optional(),
  temperature: z.number().min(0).max(2).optional(),
  max_tokens: z.number().int().positive().optional(),
  n: z.number().int().min(1).max(10).optional().default(1),
  top_p: z.number().min(0).max(1).optional(),
  stream: z.boolean().optional(),
  stream_options: streamOptionsSchema.optional(),
  tools: z.array(toolDefinitionSchema).optional(),
  tool_choice: toolChoiceSchema.optional(),
  parallel_tool_calls: z.boolean().optional(),
});

export const completionSchema = z.object({
  model: z.string().min(1),
  prompt: z.union([z.string(), z.array(z.string())]).transform(v => Array.isArray(v) ? v : [v]),
  suffix: z.string().optional(),
  max_tokens: z.number().int().min(1).max(4000).optional().default(16),
  temperature: z.number().min(0).max(2).optional(),
  top_p: z.number().min(0).max(1).optional(),
  n: z.number().int().min(1).max(10).optional().default(1),
  stream: z.boolean().optional(),
  stream_options: streamOptionsSchema.optional(),
  stop: z.union([z.string(), z.array(z.string())]).optional().transform(v => v ? (Array.isArray(v) ? v : [v]) : undefined),
  echo: z.boolean().optional().default(false),
  best_of: z.number().int().positive().optional(),
});

export function timingSafeStringEqual(a: string, b: string): boolean {
  const bufA = Buffer.from(a, 'utf8');
  const bufB = Buffer.from(b, 'utf8');

  // Constant-time comparisons require identical lengths. If they don't match,
  // we compare bufA against itself to waste the exact same amount of CPU time
  // before returning false. This prevents timing attacks on key length.
  const match = bufA.length === bufB.length;
  const compareTo = match ? bufB : bufA;

  return crypto.timingSafeEqual(bufA, compareTo) && match;
}
// Middleware: Authenticate Request
export async function apiKeyAuth(c: Context, next: Next) {
  const token = c.req.header('authorization')?.replace(/^Bearer\s+/i, '');
  const unifiedKey = getUnifiedApiKey();

  if (!token || !timingSafeStringEqual(token, unifiedKey)) {
    c.status(401);
    return c.json({
      error: { message: 'Invalid API key', type: 'authentication_error' },
    });
  }
  await next();
}

// Middleware: Validate Chat Completion Request Payload
export async function validateChatBody(c: Context, next: Next) {
  try {
    const body = await c.req.json();
    const parsed = chatCompletionSchema.safeParse(body);

    if (!parsed.success) {
      c.status(400);
      return c.json({
        error: {
          message: `Invalid request: ${parsed.error.errors.map(e => e.message).join(', ')}`,
          type: 'invalid_request_error',
        },
      });
    }

    // Pass valid data forward via Hono context variables
    c.set('parsedData', parsed.data);
    await next();
  } catch (err) {
    c.status(400);
    return c.json({ error: { message: 'Malformed JSON payload', type: 'invalid_request_error' } });
  }
}

// Middleware: Validate Completions Request Payload
export async function validateCompletionBody(c: Context, next: Next) {
  try {
    const body = await c.req.json();
    const parsed = completionSchema.safeParse(body);

    if (!parsed.success) {
      c.status(400);
      return c.json({
        error: {
          message: `Invalid request: ${parsed.error.errors.map(e => e.message).join(', ')}`,
          type: 'invalid_request_error',
        },
      });
    }

    c.set('parsedData', parsed.data);
    await next();
  } catch (err) {
    c.status(400);
    return c.json({ error: { message: 'Malformed JSON payload', type: 'invalid_request_error' } });
  }
}

interface ContentPartRaw {
  type: string;
  text?: string;
  image_url?: { url: string; detail?: string };
}

function normalizeContent(content: unknown): string | ContentPart[] | null {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    const parts = content as ContentPartRaw[];
    const hasImages = parts.some(p => p.type === 'image_url' || p.type === 'image');
    if (hasImages) return parts as ContentPart[];
    const text = parts
      .filter((p): p is ContentPartRaw & { text: string } => p.type === 'text' && typeof p.text === 'string')
      .map(p => p.text)
      .join(' ') || null;
    return text;
  }
  return null;
}

interface RawMessage {
  role: string;
  content?: unknown;
  name?: string;
  tool_calls?: Array<{ id: string; type: string; function: unknown; thought_signature?: string }>;
  tool_call_id?: string;
}

export function normalizeMessages(messages: RawMessage[]): ChatMessage[] {
  return messages.map((m): ChatMessage => {
    const base: ChatMessage = {
      role: m.role as ChatMessage['role'],
      content: normalizeContent(m.content),
      ...(m.name ? { name: m.name } : {}),
    };

    if (m.role === 'assistant' && m.tool_calls) {
      base.tool_calls = m.tool_calls.map(tc => ({
        id: tc.id,
        type: 'function' as const,
        function: tc.function as { name: string; arguments: string },
        thought_signature: tc.thought_signature,
      }));
    }

    if (m.role === 'tool') {
      base.tool_call_id = m.tool_call_id;
    }

    return base;
  });
}

// Helper: Quick Token Length Heuristic
export function estimateInputTokens(messages: ChatMessage[]): number {
  return messages.reduce((sum, m) => {
    const content = m.content;
    if (typeof content === 'string') return sum + Math.ceil(content.length / 4);
    if (Array.isArray(content)) {
      const text = content
        .filter((p): p is ContentPart & { text: string } => p.type === 'text' && typeof p.text === 'string')
        .map(p => p.text)
        .join(' ');
      return sum + Math.ceil(text.length / 4);
    }
    return sum;
  }, 0);
}
