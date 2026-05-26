import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction, runMigrations, getUnifiedApiKey } from '../src/db/index.js';

describe('Fallback Endpoint', () => {
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

  it('GET /api/fallback should return fallback chain', async () => {
    const res = await app.request('/api/fallback');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(Array.isArray(data)).toBe(true);
    expect(data.length).toBeGreaterThan(0);
  });

  it('GET /api/fallback should include required fields on each entry', async () => {
    const res = await app.request('/api/fallback');
    expect(res.status).toBe(200);
    const data = await res.json();
    const entry = data[0];

    expect(entry).toHaveProperty('modelDbId');
    expect(typeof entry.modelDbId).toBe('number');
    expect(entry).toHaveProperty('priority');
    expect(typeof entry.priority).toBe('number');
    expect(entry).toHaveProperty('enabled');
    expect(typeof entry.enabled).toBe('boolean');
    expect(entry).toHaveProperty('effectivePriority');
    expect(entry).toHaveProperty('platform');
    expect(entry).toHaveProperty('modelId');
    expect(entry).toHaveProperty('displayName');
    expect(entry).toHaveProperty('keyCount');
    expect(entry).toHaveProperty('intelligenceRank');
    expect(entry).toHaveProperty('speedRank');
    expect(entry).toHaveProperty('penalty');
    expect(entry).toHaveProperty('rateLimitHits');
  });

  it('GET /api/fallback should have zero penalties initially', async () => {
    const res = await app.request('/api/fallback');
    expect(res.status).toBe(200);
    const data = await res.json();
    // All penalties should be 0 on a fresh DB
    for (const entry of data) {
      expect(entry.penalty).toBe(0);
      expect(entry.rateLimitHits).toBe(0);
    }
  });

  it('GET /api/fallback should have effectivePriority equal to priority initially', async () => {
    const res = await app.request('/api/fallback');
    expect(res.status).toBe(200);
    const data = await res.json();
    for (const entry of data) {
      expect(entry.effectivePriority).toBe(entry.priority);
    }
  });

  describe('PUT /api/fallback', () => {
    const authHeaders = () => ({
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${apiKey}`,
    });

    it('should update fallback chain successfully', async () => {
      const res = await app.request('/api/fallback', {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify([
          { modelDbId: 1, priority: 2, enabled: true },
          { modelDbId: 2, priority: 1, enabled: true },
        ]),
      });
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.success).toBe(true);
    });

    it('should persist updated priorities in subsequent GET', async () => {
      await app.request('/api/fallback', {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify([
          { modelDbId: 1, priority: 10, enabled: true },
          { modelDbId: 2, priority: 5, enabled: true },
        ]),
      });

      const res = await app.request('/api/fallback');
      expect(res.status).toBe(200);
      const data = await res.json();
      const model1 = data.find((e: any) => e.modelDbId === 1);
      expect(model1).toBeDefined();
      expect(model1.priority).toBe(10);
    });

    it('should return 400 for invalid body', async () => {
      const res = await app.request('/api/fallback', {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify('not an array'),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 when modelDbId is not a number', async () => {
      const res = await app.request('/api/fallback', {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify([{ modelDbId: 'abc', priority: 1, enabled: true }]),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should return 400 when enabled is not a boolean', async () => {
      const res = await app.request('/api/fallback', {
        method: 'PUT',
        headers: authHeaders(),
        body: JSON.stringify([{ modelDbId: 1, priority: 1, enabled: 'yes' }]),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
    });

    it('should work without auth header (route is not protected)', async () => {
      const res = await app.request('/api/fallback', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify([{ modelDbId: 1, priority: 1, enabled: true }]),
      });
      expect(res.status).toBe(200);
    });
  });

  describe('POST /api/fallback/sort/:preset', () => {
    it('should sort by intelligence preset', async () => {
      const res = await app.request('/api/fallback/sort/intelligence', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${apiKey}`,
        },
      });
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.success).toBe(true);
      expect(data.preset).toBe('intelligence');
    });

    it('should sort by speed preset', async () => {
      const res = await app.request('/api/fallback/sort/speed', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${apiKey}`,
        },
      });
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.success).toBe(true);
      expect(data.preset).toBe('speed');
    });

    it('should sort by budget preset', async () => {
      const res = await app.request('/api/fallback/sort/budget', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${apiKey}`,
        },
      });
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.success).toBe(true);
      expect(data.preset).toBe('budget');
    });

    it('should return 400 for unknown preset', async () => {
      const res = await app.request('/api/fallback/sort/doesnotexist', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${apiKey}`,
        },
        body: JSON.stringify({}),
      });
      expect(res.status).toBe(400);
      const data = await res.json();
      expect(data.error).toBeDefined();
      expect(data.error.message).toContain('Unknown preset');
    });

    it('should work without auth header on sort route (not protected)', async () => {
      const res = await app.request('/api/fallback/sort/intelligence', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      });
      expect(res.status).toBe(200);
    });
  });

  describe('GET /api/fallback/token-usage', () => {
    it('should return token usage data', async () => {
      const res = await app.request('/api/fallback/token-usage');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('totalBudget');
      expect(typeof data.totalBudget).toBe('number');
      expect(data).toHaveProperty('totalUsed');
      expect(typeof data.totalUsed).toBe('number');
      expect(data).toHaveProperty('models');
      expect(Array.isArray(data.models)).toBe(true);
    });

    it('should return totalBudget and totalUsed as numbers', async () => {
      const res = await app.request('/api/fallback/token-usage');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Number.isFinite(data.totalBudget)).toBe(true);
      expect(Number.isFinite(data.totalUsed)).toBe(true);
    });

    it('should return model breakdown with required fields', async () => {
      const res = await app.request('/api/fallback/token-usage');
      expect(res.status).toBe(200);
      const data = await res.json();
      if (data.models.length > 0) {
        const model = data.models[0];
        expect(model).toHaveProperty('displayName');
        expect(model).toHaveProperty('platform');
        expect(model).toHaveProperty('budget');
      }
    });
  });
});
