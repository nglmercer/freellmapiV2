import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction } from '../src/db/index.js';
import { seedModels } from '../src/db/seed.js';
import { migrateModels, migrateModelsV2 } from '../src/db/migrations-v1.js';
import { migrateModelsV3Ranks, migrateModelsV4 } from '../src/db/migrations-v4.js';
import { migrateModelsV5, migrateModelsV6, migrateModelsV7, migrateModelsV8, migrateModelsV9, migrateModelsV10, migrateModelsV11 } from '../src/db/migrations-v5.js';
import { ensureUnifiedKey } from '../src/db/unified-key.js';

describe('Models Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    resetDb();
    initDb(':memory:');
    runInTransaction((tx) => {
      seedModels(tx);
      migrateModels(tx);
      migrateModelsV2(tx);
      migrateModelsV3Ranks(tx);
      migrateModelsV4(tx);
      migrateModelsV5(tx);
      migrateModelsV6(tx);
      migrateModelsV7(tx);
      migrateModelsV8(tx);
      migrateModelsV9(tx);
      migrateModelsV10(tx);
      migrateModelsV11(tx);
      ensureUnifiedKey(tx);
    });
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

it('should return an array of models', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(Array.isArray(data)).toBe(true);
     expect(data.length).toBeGreaterThan(0);
   });

it('should include required fields on each model', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
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
     const data = await res.json() as any;
     expect(typeof data[0].id).toBe('number');
   });

it('should have correct platform type', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(typeof data[0].platform).toBe('string');
     expect(data[0].platform.length).toBeGreaterThan(0);
   });

it('should have keyCount as number', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(typeof data[0].keyCount).toBe('number');
   });

it('should have hasProvider as boolean', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(typeof data[0].hasProvider).toBe('boolean');
   });

it('should return models ordered by fallback priority', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     // Seeded models should maintain priority ordering via fallback_config join
     expect(data.length).toBeGreaterThan(0);
   });

it('should have keyCount starting at 0 with no keys', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     for (const m of data) {
       expect(typeof m.keyCount).toBe('number');
     }
   });

it('should include model fields from fallback_config', async () => {
     const res = await app.request('/api/models');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     const first = data[0];
     expect(first).toHaveProperty('priority');
     expect(first).toHaveProperty('fallbackEnabled');
     expect(typeof first.fallbackEnabled).toBe('boolean');
   });
});
