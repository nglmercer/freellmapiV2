import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction, runMigrations, getUnifiedApiKey } from '../src/db/index.js';

describe('Settings Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    resetDb();
    initDb(':memory:');
    runInTransaction(runMigrations);
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

  describe('GET /api/settings/api-key', () => {
it('should return the unified API key', async () => {
       const res = await app.request('/api/settings/api-key');
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       expect(data).toHaveProperty('apiKey');
       expect(typeof data.apiKey).toBe('string');
       expect(data.apiKey.length).toBe(48);
     });

it('should match getUnifiedApiKey() value', async () => {
       const apiKey = getUnifiedApiKey();
       const res = await app.request('/api/settings/api-key');
       const data = await res.json() as any;
       expect(data.apiKey).toBe(apiKey);
     });
  });

  describe('POST /api/settings/api-key/regenerate', () => {
it('should regenerate and return a new API key', async () => {
       const oldKey = getUnifiedApiKey();

       const res = await app.request('/api/settings/api-key/regenerate', {
         method: 'POST',
       });
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       expect(data).toHaveProperty('apiKey');
       expect(typeof data.apiKey).toBe('string');
       expect(data.apiKey.length).toBe(48);
       expect(data.apiKey).not.toBe(oldKey);
     });

it('should persist the regenerated key', async () => {
       const res = await app.request('/api/settings/api-key/regenerate', {
         method: 'POST',
       });
       const { apiKey: newKey } = await res.json() as any;

       // Verify by reading it back
       const getRes = await app.request('/api/settings/api-key');
       const data = await getRes.json() as any;
       expect(data.apiKey).toBe(newKey);
     });
  });
});
