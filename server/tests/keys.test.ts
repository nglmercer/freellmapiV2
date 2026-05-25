import { describe, test, expect, beforeAll, afterAll } from 'bun:test';
import { Database } from 'bun:sqlite';
import { initDb, getUnifiedApiKey } from '../src/db/index.js';
import * as schema from '../src/db/schema.js';
import { eq } from 'drizzle-orm';

describe('API Key Authentication', () => {
  let db: Database;

  beforeAll(() => {
    db = initDb(':memory:');
  });

  afterAll(() => {
    db.delete(schema.settings).run();
  });

  describe('getUnifiedApiKey', () => {
    test('default key', () => {
      const key = getUnifiedApiKey();
      expect(key).toMatch(/^[a-f0-9]{48}$/);
    });

    test('existing key', () => {
      const key = getUnifiedApiKey();
      const secondKey = getUnifiedApiKey();
      expect(key).toBe(secondKey);
    });

    test('env key', () => {
      const key = getUnifiedApiKey();
      expect(key).toMatch(/^[0-9a-fA-F]{64}$/);
    });
  });

  describe('middleware apiKeyAuth', () => {
    test('unauthorized', async () => {
      const c = {
        req: {
          header: () => null,
        },
        status: (s: number) => { c._status = s; },
        json: (d: any) => { c._data = d; },
      } as any;
      const next = () => {};

      const { apiKeyAuth } = await import('../src/routes/middleware.js');
      await apiKeyAuth(c, next);

      expect(c._status).toBe(401);
    });

    test('unauthorized token', async () => {
      const c = {
        req: {
          header: () => 'Bearer wrong-key',
        },
        status: (s: number) => { c._status = s; },
        json: (d: any) => { c._data = d; },
      } as any;
      const next = () => {};

      const { apiKeyAuth } = await import('../src/routes/middleware.js');
      await apiKeyAuth(c, next);

      expect(c._status).toBe(401);
    });

    test('authorized token', async () => {
      const key = getUnifiedApiKey();
      const c = {
        req: {
          header: () => `Bearer ${key}`,
        },
        status: (s: number) => { c._status = s; },
        json: (d: any) => { c._data = d; },
      } as any;
      const next = () => {};

      const { apiKeyAuth } = await import('../src/routes/middleware.js');
      await apiKeyAuth(c, next);

      expect(c._status).toBe(200);
    });
  });
});
