import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction, runMigrations, getUnifiedApiKey } from '../src/db/index.js';
import { chatCompletionSchema, completionSchema } from '../src/routes/middleware.js';

describe('/v1/completions', () => {
  let app: ReturnType<typeof createApp>;
  let apiKey: string;

  beforeEach(async () => {
    resetDb();
    initDb(':memory:');
    runInTransaction(runMigrations);
    app = createApp();
    apiKey = getUnifiedApiKey();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/completions — authentication
  // ─────────────────────────────────────────────────────────────

  describe('authentication', () => {
    it('should return 401 when no Authorization header is provided', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello world',
        }),
      });
      expect(res.status).toBe(401);
      const data = await res.json() as any;
      expect(data.error).toBeDefined();
      expect(data.error.message).toBe('Invalid API key');
      expect(data.error.type).toBe('authentication_error');
    });

    it('should return 401 with a wrong API key', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer wrong-key',
        },
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello world',
        }),
      });
      expect(res.status).toBe(401);
      const data = await res.json() as any;
      expect(data.error.message).toBe('Invalid API key');
      expect(data.error.type).toBe('authentication_error');
    });

    it('should allow access with the correct unified API key', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello world',
          max_tokens: 10,
        }),
      });
      expect(res.status).not.toBe(401);
    });
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/completions — request validation
  // ─────────────────────────────────────────────────────────────

  describe('request validation', () => {
    const authHeaders = () => ({
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    });

    it('should return 400 for an empty body', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: '',
      });
      expect(res.status).toBe(400);
      const data = await res.json() as any;
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should return 400 for malformed JSON', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: 'not json at all',
      });
      expect(res.status).toBe(400);
      const data = await res.json() as any;
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should return 400 when model is missing', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ prompt: 'Hello' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json() as any;
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should return 400 when prompt is missing', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ model: 'auto' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json() as any;
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should accept a string prompt', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Say hello',
          max_tokens: 10,
        }),
      });
      expect(res.status).not.toBe(400);
    });

    it('should accept an array of prompts', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: ['Hello', 'World'],
          max_tokens: 10,
        }),
      });
      expect(res.status).not.toBe(400);
    });

    it('should return 400 for negative temperature', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          temperature: -1,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 for temperature > 2', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          temperature: 3,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when max_tokens is not a positive integer', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          max_tokens: 0,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when max_tokens exceeds 4000', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          max_tokens: 5000,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when top_p is outside [0, 1]', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          top_p: 1.5,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when n exceeds 10', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          n: 11,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should accept a valid request with optional fields', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Continue: Once upon a time',
          suffix: 'The end.',
          max_tokens: 50,
          temperature: 0.7,
          top_p: 0.9,
          n: 2,
          echo: true,
          best_of: 2,
          stop: ['\n'],
        }),
      });
      expect(res.status).not.toBe(400);
      expect(res.status).not.toBe(401);
    });

    it('should accept stream_options with include_usage', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello',
          max_tokens: 10,
          stream: true,
          stream_options: { include_usage: true },
        }),
      });
      expect(res.status).not.toBe(400);
    });
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/completions — handler (no API keys)
  // ─────────────────────────────────────────────────────────────

  describe('handler results (no API keys configured)', () => {
    const authHeaders = () => ({
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    });

    it('should return an error response when no external provider keys configured', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: 'Hello world',
          max_tokens: 10,
        }),
      });
      expect([429, 502, 503, 500]).toContain(res.status);
      const data = await res.json() as any;
      expect(data).toHaveProperty('error');
    });

    it('should return a structured error object', async () => {
      const res = await app.request('/v1/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          prompt: ['Hello', 'World'],
          max_tokens: 10,
          n: 2,
        }),
      });
      const data = await res.json() as any;
      expect(data.error).toBeDefined();
      expect(typeof data.error.message).toBe('string');
      expect(typeof data.error.type).toBe('string');
    });
  });
});

// ─────────────────────────────────────────────────────────────
// Chat completions — n > 1 handling
// ─────────────────────────────────────────────────────────────

describe('POST /v1/chat/completions — n > 1', () => {
  let app: ReturnType<typeof createApp>;
  let apiKey: string;

  beforeEach(async () => {
    resetDb();
    initDb(':memory:');
    runInTransaction(runMigrations);
    app = createApp();
    apiKey = getUnifiedApiKey();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  const authHeaders = () => ({
    'Content-Type': 'application/json',
    Authorization: `Bearer ${apiKey}`,
  });

  describe('request validation', () => {
    it('should accept n = 1 (default)', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
          n: 1,
        }),
      });
      expect(res.status).not.toBe(400);
    });

    it('should accept n between 2 and 10', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
          n: 5,
        }),
      });
      expect(res.status).not.toBe(400);
    });

    it('should return 400 when n is 0', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
          n: 0,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when n exceeds 10', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
          n: 11,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when n is negative', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
          n: -1,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return an error when no keys configured with n > 1', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hello' }],
          n: 3,
        }),
      });
      expect([429, 502, 503, 500]).toContain(res.status);
      const data = await res.json() as any;
      expect(data).toHaveProperty('error');
    });
  });
});

// ─────────────────────────────────────────────────────────────
// Schema validation unit tests
// ─────────────────────────────────────────────────────────────

describe('completionSchema', () => {
  it('should parse a minimal valid request', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.prompt).toEqual(['Hello']);
      expect(result.data.max_tokens).toBe(16);
      expect(result.data.n).toBe(1);
      expect(result.data.echo).toBe(false);
    }
  });

  it('should transform a single string prompt into an array', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Single prompt',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(Array.isArray(result.data.prompt)).toBe(true);
      expect(result.data.prompt).toEqual(['Single prompt']);
    }
  });

  it('should keep an array prompt as-is', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: ['First', 'Second', 'Third'],
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.prompt).toEqual(['First', 'Second', 'Third']);
    }
  });

  it('should transform a string stop into an array', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
      stop: '\n',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.stop).toEqual(['\n']);
    }
  });

  it('should handle missing optional fields with defaults', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.max_tokens).toBe(16);
      expect(result.data.n).toBe(1);
      expect(result.data.echo).toBe(false);
      expect(result.data.stream).toBeUndefined();
      expect(result.data.temperature).toBeUndefined();
      expect(result.data.top_p).toBeUndefined();
      expect(result.data.suffix).toBeUndefined();
      expect(result.data.stop).toBeUndefined();
    }
  });

  it('should reject request with max_tokens = 0', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
      max_tokens: 0,
    });
    expect(result.success).toBe(false);
  });

  it('should reject request with max_tokens > 4000', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
      max_tokens: 5000,
    });
    expect(result.success).toBe(false);
  });

  it('should reject request with n = 0', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
      n: 0,
    });
    expect(result.success).toBe(false);
  });

  it('should reject request with n > 10', () => {
    const result = completionSchema.safeParse({
      model: 'gpt-3.5-turbo',
      prompt: 'Hello',
      n: 11,
    });
    expect(result.success).toBe(false);
  });
});

describe('chatCompletionSchema — n field', () => {
  it('should default n to 1 when omitted', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.n).toBe(1);
    }
  });

  it('should accept n = 1 explicitly', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      n: 1,
    });
    expect(result.success).toBe(true);
  });

  it('should accept n = 10 (max)', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      n: 10,
    });
    expect(result.success).toBe(true);
  });

  it('should reject n = 0', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      n: 0,
    });
    expect(result.success).toBe(false);
  });

  it('should reject n = 11', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      n: 11,
    });
    expect(result.success).toBe(false);
  });

  it('should reject n = -1', () => {
    const result = chatCompletionSchema.safeParse({
      messages: [{ role: 'user', content: 'Hello' }],
      n: -1,
    });
    expect(result.success).toBe(false);
  });
});
