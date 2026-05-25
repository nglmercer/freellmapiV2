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
    test('debe devolver una key por defecto si no existe en DB', () => {
      const key = getUnifiedApiKey();
      expect(key).toMatch(/^freellmapi-[a-f0-9]{48}$/);
    });

    test('debe devolver la key existente si ya está en DB', () => {
      const key = getUnifiedApiKey();
      const secondKey = getUnifiedApiKey();
      expect(key).toBe(secondKey);
    });

    test('debe devolver key de .env si existe', () => {
      const key = getUnifiedApiKey();
      expect(key).toMatch(/^[0-9a-fA-F]{64}$/);
    });
  });

  describe('middleware apiKeyAuth', () => {
    test('debe rechautenticar requests sin token', async () => {
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

    test('debe rechautenticar token incorrecto', async () => {
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

    test('debe autenticar token correcto', async () => {
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
