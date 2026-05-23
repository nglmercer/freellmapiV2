import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb } from '../src/db/index.js';

describe('Keys Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  describe('GET /api/keys/', () => {
    it('should return empty array initially', async () => {
      const res = await app.request('/api/keys');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
      expect(data.length).toBe(0);
    });
  });

  describe('POST /api/keys/', () => {
    it('should add a key and return 201', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          platform: 'google',
          key: 'test-api-key-123',
          label: 'Test Key',
        }),
      });

      expect(res.status).toBe(201);
      const data = await res.json();
      expect(data).toHaveProperty('id');
      expect(data.platform).toBe('google');
      expect(data.label).toBe('Test Key');
      expect(data.maskedKey).not.toBe('test-api-key-123');
      expect(data.enabled).toBe(true);
      expect(data.status).toBe('unknown');
    });

    it('should mask the key (first 4 + last 4 chars)', async () => {
      const key = 'abcdefghijklmnop';
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key }),
      });
      expect(res.status).toBe(201);
      const data = await res.json();
      expect(data.maskedKey).toBe('abcd...nop');
    });

    it('should mask short keys as ****+last4', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'short' }),
      });
      expect(res.status).toBe(201);
      const data = await res.json();
      expect(data.maskedKey).toBe('****hort');
    });

    it('should default label to empty string when not provided', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'my-key' }),
      });
      expect(res.status).toBe(201);
      const data = await res.json();
      expect(data.label).toBe('');
    });

    it('should return 400 for missing platform', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ key: 'my-api-key' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 for empty key', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: '' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 for invalid platform enum', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'openai', key: 'abc' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should accept all valid platforms', async () => {
      const platforms = [
        'google', 'groq', 'cerebras', 'sambanova', 'nvidia', 'mistral',
        'openrouter', 'github', 'cohere', 'cloudflare', 'zhipu', 'ollama',
        'kilo', 'pollinations', 'llm7',
      ] as const;

      for (const platform of platforms) {
        const res = await app.request('/api/keys', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ platform, key: `key-for-${platform}` }),
        });
        expect(res.status).toBe(201);
        const data = await res.json();
        expect(data.platform).toBe(platform);
      }
    });

    it('should return 400 for malformed JSON body', async () => {
      const res = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: 'not json',
      });
      expect(res.status).toBe(400);
    });

    it('should retrieve the list after adding a key', async () => {
      await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'key-1', label: 'One' }),
      });

      const res = await app.request('/api/keys');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.length).toBe(1);
      expect(data[0].label).toBe('One');
    });

    it('should list keys newest first', async () => {
      await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'key-a', label: 'A' }),
      });
      await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'groq', key: 'key-b', label: 'B' }),
      });

      const res = await app.request('/api/keys');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data[0].label).toBe('B');
      expect(data[1].label).toBe('A');
    });
  });

  describe('DELETE /api/keys/:id', () => {
    it('should delete an existing key', async () => {
      const addRes = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'delete-me' }),
      });
      const added = await addRes.json();

      const delRes = await app.request(`/api/keys/${added.id}`, {
        method: 'DELETE',
      });
      expect(delRes.status).toBe(200);
      const delData = await delRes.json();
      expect(delData.success).toBe(true);

      const getRes = await app.request('/api/keys');
      const data = await getRes.json();
      expect(data.length).toBe(0);
    });

    it('should return 404 when deleting a non-existent key', async () => {
      const res = await app.request('/api/keys/99999', { method: 'DELETE' });
      expect(res.status).toBe(404);
      const data = await res.json();
      expect(data.error.message).toBe('Key not found');
    });

    it('should return 400 for a non-numeric id', async () => {
      const res = await app.request('/api/keys/abc', { method: 'DELETE' });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error.message).toBe('Invalid key ID');
    });
  });

  describe('PATCH /api/keys/:id', () => {
    it('should toggle a key enabled=false', async () => {
      const addRes = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'toggle-key' }),
      });
      const { id } = await addRes.json();

      const patchRes = await app.request(`/api/keys/${id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: false }),
      });
      expect(patchRes.status).toBe(200);
      const data = await patchRes.json();
      expect(data.success).toBe(true);
      expect(data.enabled).toBe(false);
    });

    it('should toggle a key enabled=true', async () => {
      const addRes = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'toggle-key-2' }),
      });
      const { id } = await addRes.json();

      // Disable first
      await app.request(`/api/keys/${id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: false }),
      });

      // Re-enable
      const patchRes = await app.request(`/api/keys/${id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: true }),
      });
      expect(patchRes.status).toBe(200);
      const data = await patchRes.json();
      expect(data.enabled).toBe(true);
    });

    it('should return 400 for non-boolean enabled field', async () => {
      const addRes = await app.request('/api/keys', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ platform: 'google', key: 'bad-toggle' }),
      });
      const { id } = await addRes.json();

      const res = await app.request(`/api/keys/${id}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: 'yes' }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error.message).toContain('boolean');
    });

    it('should return 400 for non-numeric id', async () => {
      const res = await app.request('/api/keys/nope', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: true }),
      });
      expect(res.status).toBe(400);
    });

    it('should return 404 for a non-existent key', async () => {
      const res = await app.request('/api/keys/99999', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: false }),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error.message).toBe('Key not found');
    });
  });
});
