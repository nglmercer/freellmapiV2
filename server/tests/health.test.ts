import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { createApp } from '../src/app.js';
import { initDb } from '../src/db/index.js';

describe('Health Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    // Initialize test database
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // Cleanup if needed
  });

  it('should return ok status for /api/ping', async () => {
    const res = await app.request('/api/ping');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.status).toBe('ok');
    expect(data.timestamp).toBeDefined();
  });

  it('should return platform health data', async () => {
    const res = await app.request('/api/health');
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data).toHaveProperty('platforms');
    expect(data).toHaveProperty('keys');
    expect(Array.isArray(data.platforms)).toBe(true);
    expect(Array.isArray(data.keys)).toBe(true);
  });
});