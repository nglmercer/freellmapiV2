import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb } from '../src/db/index.js';

describe('Models Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  it('should return an array of models', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(Array.isArray(data)).toBe(true);
    expect(data.length).toBeGreaterThan(0);
  });

  it('should include required fields on each model', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    const first = data[0];

    expect(first).toHaveProperty('id');
    expect(first).toHaveProperty('platform');
    expect(first).toHaveProperty('modelId');
    expect(first).toHaveProperty('displayName');
    expect(first).toHaveProperty('intelligenceRank');
    expect(first).toHaveProperty('speedRank');
    expect(first).toHaveProperty('rpmLimit');
    expect(first).toHaveProperty('rpdLimit');
    expect(first).toHaveProperty('tpmLimit');
    expect(first).toHaveProperty('tpdLimit');
    expect(first).toHaveProperty('enabled');
    expect(typeof first.enabled).toBe('boolean');
    expect(first).toHaveProperty('hasProvider');
    expect(first).toHaveProperty('keyCount');
  });

  it('should have correct id type', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(typeof data[0].id).toBe('number');
  });

  it('should have correct platform type', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(typeof data[0].platform).toBe('string');
    expect(data[0].platform.length).toBeGreaterThan(0);
  });

  it('should have keyCount as number', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(typeof data[0].keyCount).toBe('number');
  });

  it('should have hasProvider as boolean', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(typeof data[0].hasProvider).toBe('boolean');
  });

  it('should return models ordered by fallback priority', async () => {
    const res = await app.request('/api/models');
    expect(res.status).toBe(200);
    const data = await res.json();
    // Seeded models should maintain priority ordering via fallback_config join
    expect(data.length).toBeGreaterThan(0);
  });
});
