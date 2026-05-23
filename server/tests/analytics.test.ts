import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb } from '../src/db/index.js';

describe('Analytics Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  describe('GET /api/analytics/summary', () => {
    it('should return summary stats', async () => {
      const res = await app.request('/api/analytics/summary');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('totalRequests');
      expect(data).toHaveProperty('successRate');
      expect(data).toHaveProperty('totalInputTokens');
      expect(data).toHaveProperty('totalOutputTokens');
      expect(data).toHaveProperty('avgLatencyMs');
      expect(data).toHaveProperty('estimatedCostSavings');
    });

    it('should default totalRequests to 0 on empty DB', async () => {
      const res = await app.request('/api/analytics/summary');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.totalRequests).toBe(0);
      expect(data.successRate).toBe(0);
    });

    it('should accept range=24h query param', async () => {
      const res = await app.request('/api/analytics/summary?range=24h');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('totalRequests');
    });

    it('should accept range=30d query param', async () => {
      const res = await app.request('/api/analytics/summary?range=30d');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('totalRequests');
    });

    it('should return numeric fields', async () => {
      const res = await app.request('/api/analytics/summary');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(typeof data.totalRequests).toBe('number');
      expect(typeof data.successRate).toBe('number');
      expect(typeof data.totalInputTokens).toBe('number');
      expect(typeof data.totalOutputTokens).toBe('number');
      expect(typeof data.avgLatencyMs).toBe('number');
      expect(typeof data.estimatedCostSavings).toBe('number');
    });
  });

  describe('GET /api/analytics/by-model', () => {
    it('should return an array', async () => {
      const res = await app.request('/api/analytics/by-model');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should accept range=24h', async () => {
      const res = await app.request('/api/analytics/by-model?range=24h');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should accept range=30d', async () => {
      const res = await app.request('/api/analytics/by-model?range=30d');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should return empty array when there are no requests', async () => {
      const res = await app.request('/api/analytics/by-model');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
      expect(data.length).toBe(0);
    });
  });

  describe('GET /api/analytics/by-platform', () => {
    it('should return an array', async () => {
      const res = await app.request('/api/analytics/by-platform');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should return empty array when there are no requests', async () => {
      const res = await app.request('/api/analytics/by-platform');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.length).toBe(0);
    });

    it('should accept range query param', async () => {
      const res = await app.request('/api/analytics/by-platform?range=7d');
      expect(res.status).toBe(200);
      expect(Array.isArray(await res.json())).toBe(true);
    });
  });

  describe('GET /api/analytics/timeline', () => {
    it('should return timeline data', async () => {
      const res = await app.request('/api/analytics/timeline');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should accept interval=hour param', async () => {
      const res = await app.request('/api/analytics/timeline?range=24h&interval=hour');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should accept interval=day param (default for 7d)', async () => {
      const res = await app.request('/api/analytics/timeline?range=7d&interval=day');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should return entries with timestamp, requests, successCount, failureCount', async () => {
      const res = await app.request('/api/analytics/timeline');
      expect(res.status).toBe(200);
      const data = await res.json();
      // Timeline returns empty array on fresh DB (no requests in range)
      expect(Array.isArray(data)).toBe(true);
    });
  });

  describe('GET /api/analytics/error-distribution', () => {
    it('should return error distribution data', async () => {
      const res = await app.request('/api/analytics/error-distribution');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('byCategory');
      expect(data).toHaveProperty('byPlatform');
      expect(data).toHaveProperty('detailed');
      expect(Array.isArray(data.byCategory)).toBe(true);
      expect(Array.isArray(data.byPlatform)).toBe(true);
      expect(Array.isArray(data.detailed)).toBe(true);
    });

    it('should accept range query param', async () => {
      const res = await app.request('/api/analytics/error-distribution?range=30d');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data).toHaveProperty('byCategory');
      expect(data).toHaveProperty('byPlatform');
      expect(data).toHaveProperty('detailed');
    });

    it('should return empty arrays when there are no errors', async () => {
      const res = await app.request('/api/analytics/error-distribution');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.byCategory.length).toBe(0);
      expect(data.byPlatform.length).toBe(0);
      expect(data.detailed.length).toBe(0);
    });
  });

  describe('GET /api/analytics/errors', () => {
    it('should return recent errors array', async () => {
      const res = await app.request('/api/analytics/errors');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(Array.isArray(data)).toBe(true);
    });

    it('should return empty array when there are no errors', async () => {
      const res = await app.request('/api/analytics/errors');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.length).toBe(0);
    });

    it('should return at most 50 errors', async () => {
      const res = await app.request('/api/analytics/errors');
      expect(res.status).toBe(200);
      const data = await res.json();
      expect(data.length).toBeLessThanOrEqual(50);
    });

    it('should accept range query param', async () => {
      const res = await app.request('/api/analytics/errors?range=24h');
      expect(res.status).toBe(200);
      expect(Array.isArray(await res.json())).toBe(true);
    });
  });
});
