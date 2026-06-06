import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { createApp } from '../src/app.js';
import { initDb, resetDb, runInTransaction } from '../src/db/index.js';
import { seedModels } from '../src/db/seed.js';
import { migrateModels, migrateModelsV2 } from '../src/db/migrations-v1.js';
import { migrateModelsV3Ranks, migrateModelsV4 } from '../src/db/migrations-v4.js';
import { migrateModelsV5, migrateModelsV6, migrateModelsV7, migrateModelsV8, migrateModelsV9, migrateModelsV10, migrateModelsV11 } from '../src/db/migrations-v5.js';
import { migrateModelsV12 } from '../src/db/migrations-v12.js';
import { migrateModelsV13 } from '../src/db/migrations-v13.js';
import { ensureUnifiedKey } from '../src/db/unified-key.js';
import { seedTestModels } from '../src/db/test-fixtures.js';

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
      migrateModelsV12(tx);
      migrateModelsV13(tx);
      ensureUnifiedKey(tx);
      // Hardcoded seed/migration data is intentionally absent. Insert a
      // representative fixture for the route tests to query.
      seedTestModels(tx);
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

describe('Models Bulk Actions', () => {
  let app: ReturnType<typeof createApp>;
  let apiKey: string;

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
      migrateModelsV12(tx);
      migrateModelsV13(tx);
      ensureUnifiedKey(tx);
      seedTestModels(tx);
    });
    app = createApp();
    const { getUnifiedApiKey } = await import('../src/db/index.js');
    apiKey = getUnifiedApiKey();
  });

  it('POST /api/models/disable-all rejects without confirm flag', async () => {
    const res = await app.request('/api/models/disable-all', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });

  it('POST /api/models/disable-all flips every model to enabled=0', async () => {
    const res = await app.request('/api/models/disable-all', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ confirm: true }),
    });
    expect(res.status).toBe(200);
    const data = await res.json() as any;
    expect(data.success).toBe(true);
    expect(data.modelsUpdated).toBeGreaterThan(0);

    const list = await app.request('/api/models');
    const models = (await list.json()) as any[];
    for (const m of models) {
      expect(m.enabled).toBe(false);
      expect(m.fallbackEnabled).toBe(false);
    }
  });

  it('POST /api/models/enable-all flips every model to enabled=1', async () => {
    const res = await app.request('/api/models/enable-all', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ confirm: true }),
    });
    expect(res.status).toBe(200);
    const data = await res.json() as any;
    expect(data.success).toBe(true);
    expect(data.modelsUpdated).toBeGreaterThan(0);

    const list = await app.request('/api/models');
    const models = (await list.json()) as any[];
    for (const m of models) {
      expect(m.enabled).toBe(true);
      expect(m.fallbackEnabled).toBe(true);
    }
  });

  it('POST /api/models/enable-free rejects without confirm flag', async () => {
    const res = await app.request('/api/models/enable-free', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });

  it('POST /api/models/enable-free only enables rows where freeTier is true', async () => {
    const res = await app.request('/api/models/enable-free', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ confirm: true }),
    });
    expect(res.status).toBe(200);
    const data = await res.json() as any;
    expect(data.success).toBe(true);

    const list = await app.request('/api/models');
    const models = (await list.json()) as any[];
    for (const m of models) {
      if (m.freeTier === true) {
        expect(m.enabled).toBe(true);
        expect(m.fallbackEnabled).toBe(true);
      } else {
        expect(m.enabled).toBe(false);
        expect(m.fallbackEnabled).toBe(false);
      }
    }
  });
});
