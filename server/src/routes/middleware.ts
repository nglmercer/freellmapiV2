import { type Context, type Next } from 'hono';
import type { ChatMessage } from '@freellmapi/shared/types.js';
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

export const systemMessageSchema = z.object({
  role: z.literal('system'),
  content: z.string(),
  name: z.string().optional(),
});

export const userMessageSchema = z.object({
  role: z.literal('user'),
  content: z.string(),
  name: z.string().optional(),
});

export const assistantMessageSchema = z.object({
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

export const toolMessageSchema = z.object({
  role: z.literal('tool'),
  content: z.string(),
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
  top_p: z.number().min(0).max(1).optional(),
  stream: z.boolean().optional(),
  tools: z.array(toolDefinitionSchema).optional(),
  tool_choice: toolChoiceSchema.optional(),
  parallel_tool_calls: z.boolean().optional(),
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

// Middleware: Validate Request Payload with Zod
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

// Helper: Normalize Messages to Provider Expectations
export function normalizeMessages(messages: any[]): ChatMessage[] {
  return messages.map((m): ChatMessage => {
    const base: ChatMessage = {
      role: m.role,
      content: m.content ?? null,
      ...(m.name ? { name: m.name } : {}),
    };

    if (m.role === 'assistant' && m.tool_calls) {
      base.tool_calls = m.tool_calls.map((tc: any) => ({
        id: tc.id,
        type: tc.type,
        function: tc.function,
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
    if (typeof m.content !== 'string') return sum;
    return sum + Math.ceil(m.content.length / 4);
  }, 0);
}
