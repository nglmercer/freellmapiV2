import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction, runMigrations, getUnifiedApiKey } from '../src/db/index.js';

/**
 * Proxy tests.
 *
 * GET  /v1/models         — open (no API key required)
 * POST /v1/chat/completions — protected by apiKeyAuth + validateChatBody
 */
describe('Proxy Endpoint', () => {
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
  // GET /v1/models
  // ─────────────────────────────────────────────────────────────

  describe('GET /v1/models', () => {
    it('should return OpenAI-compatible model list', async () => {
      const res = await app.request('/v1/models');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.object).toBe('list');
      expect(Array.isArray(data.data)).toBe(true);
      expect(data.data.length).toBeGreaterThan(0);
    });

    it('should include the virtual "auto" model as first entry', async () => {
      const res = await app.request('/v1/models');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.data[0].id).toBe('auto');
      expect(data.data[0].owned_by).toBe('freellmapi');
    });

    it('should include required OpenAI model fields', async () => {
      const res = await app.request('/v1/models');
      expect(res.status).toBe(200);
      const data = await res.json();
      const model = data.data[1]; // first real model (index 0 is "auto")
      expect(model).toHaveProperty('id');
      expect(model).toHaveProperty('object');
      expect(model).toHaveProperty('owned_by');
      expect(model).toHaveProperty('name');
      expect(model.object).toBe('model');
    });

    it('should not require Authorization header', async () => {
      // /v1/models is intentionally open — no apiKeyAuth middleware
      const res = await app.request('/v1/models');
      expect(res.status).toBe(200);
    });

    it('should contain at least one real model entry', async () => {
      const res = await app.request('/v1/models');
      expect(res.status).toBe(200);
      const data = await res.json();
      // At minimum the seeded models should be present (index 1+)
      const realModels = data.data.filter((m: { owned_by: string }) => m.owned_by !== 'freellmapi');
      expect(realModels.length).toBeGreaterThan(0);
    });
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/chat/completions — auth
  // ─────────────────────────────────────────────────────────────

  describe('POST /v1/chat/completions — authentication', () => {
    it('should return 401 when no Authorization header is provided', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hello' }],
        }),
      });
      expect(res.status).toBe(401);
      const data = await res.json();
      expect(data.error).toBeDefined();
      expect(data.error.message).toBe('Invalid API key');
      expect(data.error.type).toBe('authentication_error');
    });

    it('should return 401 with a wrong API key', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer wrong-key',
        },
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hello' }],
        }),
      });
      expect(res.status).toBe(401);
      const data = await res.json();
      expect(data.error.message).toBe('Invalid API key');
      expect(data.error.type).toBe('authentication_error');
    });

    it('should reject an empty Authorization token', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: 'Bearer ',
        },
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hi' }],
        }),
      });
      expect(res.status).toBe(401);
      const data = await res.json();
      expect(data.error.message).toBe('Invalid API key');
    });

    it('should allow access with the correct unified API key', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hello' }],
        }),
      });
      // Must NOT be 401 — auth passes, handler is reached
      expect(res.status).not.toBe(401);
    });
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/chat/completions — request validation
  // ─────────────────────────────────────────────────────────────

  describe('POST /v1/chat/completions — request validation', () => {
    const authHeaders = () => ({
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    });

    it('should return 400 for an empty body', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
      headers: authHeaders(),
      body: '',
    });
    expect(res.status).toBe(400);
    const data = await res.json();
    expect(data.error).toBeDefined();
    expect(data.error.type).toBe('invalid_request_error');
  });

  it('should return 400 for malformed JSON', async () => {
    const res = await app.request('/v1/chat/completions', {
      method: 'POST',
      headers: authHeaders(),
      body: 'not json at all',
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should return 400 when messages field is missing', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ model: 'auto' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
      expect(data.error.type).toBe('invalid_request_error');
    });

    it('should return 400 when messages array is empty', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ model: 'auto', messages: [] }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 when a message has no role', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ messages: [{ content: 'hi' }] }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 for negative temperature', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'hi' }],
          temperature: -1,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 for temperature > 2', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'hi' }],
          temperature: 3,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when max_tokens is not a positive integer', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'hi' }],
          max_tokens: 0,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 400 when top_p is outside [0, 1]', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'hi' }],
          top_p: 1.5,
        }),
      });
      expect(res.status).toBe(400);
    });

    it('should accept a valid user message', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'Hello world' }],
        }),
      });
      // Validation passes — handler is invoked
      expect(res.status).not.toBe(400);
      expect(res.status).not.toBe(401);
    });

    it('should accept a system + user message pair', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [
            { role: 'system', content: 'You are a helpful assistant.' },
            { role: 'user', content: 'Say hi' },
          ],
        }),
      });
      expect(res.status).not.toBe(400);
      expect(res.status).not.toBe(401);
    });

    it('should accept an assistant message with role and non-empty content', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [
            { role: 'user', content: 'Hi' },
            { role: 'assistant', content: 'Hello there' },
            { role: 'user', content: 'How are you?' },
          ],
        }),
      });
      expect(res.status).not.toBe(400);
      expect(res.status).not.toBe(401);
    });
  });

  // ─────────────────────────────────────────────────────────────
  // POST /v1/chat/completions — handler invocation (no API keys)
  // ─────────────────────────────────────────────────────────────

  describe('POST /v1/chat/completions — handler results (fresh in-memory DB)', () => {
    const authHeaders = () => ({
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    });

    it('should return an error response when no external provider keys are configured', async () => {
      // Fresh in-memory DB: seeded models + fallback_config, but zero key rows.
      // routeRequest() finds no usable keys → throws → handler catches.
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'test' }],
        }),
      });
      // Not 400/401 (those are auth/validation failures)
      expect([429, 502, 503, 500]).toContain(res.status);
      const data = await res.json();
      expect(data).toHaveProperty('error');
    });

    it('should return a structured error object', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: 'auto',
          messages: [{ role: 'user', content: 'test' }],
        }),
      });
      const data = await res.json();
      expect(data.error).toBeDefined();
      expect(typeof data.error.message).toBe('string');
      expect(typeof data.error.type).toBe('string');
    });
  });
});
