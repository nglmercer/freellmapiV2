import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { Hono } from 'hono';
import { createApp } from '../src/app.js';
import { initDb, getDb, getUnifiedApiKey } from '../src/db/index.js';
import { encrypt } from '../src/lib/crypto.js';
import * as schema from '../src/db/schema.js';

describe('Health Endpoint', () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(async () => {
    initDb(':memory:');
    app = createApp();
  });

  afterEach(() => {
    // in-memory DB is discarded naturally between tests
  });

it('should return ok status for /api/ping', async () => {
     const res = await app.request('/api/ping');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(data.status).toBe('ok');
     expect(data.timestamp).toBeDefined();
   });

it('GET /api/health should return platform summary and keys array', async () => {
     const res = await app.request('/api/health');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(data).toHaveProperty('platforms');
     expect(data).toHaveProperty('keys');
     expect(Array.isArray(data.platforms)).toBe(true);
     expect(Array.isArray(data.keys)).toBe(true);
   });

it('should have correct platform shape', async () => {
     const db = getDb();
     const { encrypted, iv, authTag } = encrypt('shape-test-key');
     db.insert(schema.apiKeys).values({
       platform: 'google',
       encryptedKey: encrypted,
       iv,
       authTag,
       status: 'healthy',
       enabled: 1
     }).run();

     const res = await app.request('/api/health');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     expect(data.platforms.length).toBeGreaterThan(0);
     const p = data.platforms[0];
     expect(p).toHaveProperty('platform');
     expect(p).toHaveProperty('hasProvider');
     expect(p).toHaveProperty('totalKeys');
     expect(p).toHaveProperty('healthyKeys');
     expect(p).toHaveProperty('rateLimitedKeys');
     expect(p).toHaveProperty('invalidKeys');
     expect(p).toHaveProperty('errorKeys');
     expect(p).toHaveProperty('unknownKeys');
     expect(p).toHaveProperty('enabledKeys');
     expect(typeof p.hasProvider).toBe('boolean');
     expect(typeof p.totalKeys).toBe('number');
     expect(typeof p.healthyKeys).toBe('number');
   });

it('should count keys by status correctly', async () => {
     const db = getDb();
     const { encrypted, iv, authTag } = encrypt('test-health-key');

     // Insert keys with various statuses
     db.insert(schema.apiKeys).values([
       { platform: 'google', encryptedKey: encrypted, iv, authTag, status: 'healthy', enabled: 1 },
       { platform: 'google', encryptedKey: encrypted, iv, authTag, status: 'healthy', enabled: 1 },
       { platform: 'google', encryptedKey: encrypted, iv, authTag, status: 'invalid', enabled: 1 },
       { platform: 'groq', encryptedKey: encrypted, iv, authTag, status: 'rate_limited', enabled: 1 },
       { platform: 'groq', encryptedKey: encrypted, iv, authTag, status: 'error', enabled: 0 }
     ]).run();

     const res = await app.request('/api/health');
     expect(res.status).toBe(200);
     const data = await res.json() as any;

     const googlePlatform = data.platforms.find((p: { platform: string }) => p.platform === 'google');
     expect(googlePlatform).toBeDefined();
     expect(googlePlatform.totalKeys).toBe(3);
     expect(googlePlatform.healthyKeys).toBe(2);
     expect(googlePlatform.invalidKeys).toBe(1);
     expect(googlePlatform.rateLimitedKeys).toBe(0);
     expect(googlePlatform.errorKeys).toBe(0);
     expect(googlePlatform.enabledKeys).toBe(3);

     const groqPlatform = data.platforms.find((p: { platform: string }) => p.platform === 'groq');
     expect(groqPlatform).toBeDefined();
     expect(groqPlatform.rateLimitedKeys).toBe(1);
     expect(groqPlatform.errorKeys).toBe(1);
     expect(groqPlatform.enabledKeys).toBe(1); // third key has enabled = 0
   });

it('should list all keys in the keys array', async () => {
     const db = getDb();
     const { encrypted, iv, authTag } = encrypt('health-key-1');

     db.insert(schema.apiKeys).values({
       platform: 'google',
       encryptedKey: encrypted,
       iv,
       authTag,
       status: 'healthy',
       enabled: 1,
       label: 'Health Test Key'
     }).run();

     const res = await app.request('/api/health');
     expect(res.status).toBe(200);
     const data = await res.json() as any;
     const googleKey = data.keys.find((k: any) => k.platform === 'google');
     expect(googleKey).toBeDefined();
     expect(googleKey.label).toBe('Health Test Key');
     expect(googleKey.status).toBe('healthy');
     expect(googleKey.enabled).toBe(true);
   });

  describe('POST /api/health/check/:keyId', () => {
it('should return 400 for an invalid key id', async () => {
       const res = await app.request('/api/health/check/notanumber', {
         method: 'POST',
       });
       expect(res.status).toBe(400);
       const data = await res.json() as any;
       expect(data.error.message).toBe('Invalid key ID');
     });

it('should accept a negative key id as valid numeric id', async () => {
       // parseInt('-1') = -1 (not NaN), so the route proceeds to checkKeyHealth
       const res = await app.request('/api/health/check/-1', {
         method: 'POST',
       });
       // No key with id -1 exists, so returns 200 with error status
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       expect(data.status).toBe('error');
     });

it('should return error for a non-existent key', async () => {
       const res = await app.request('/api/health/check/99999', {
         method: 'POST',
       });
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       // checkKeyHealth returns 'error' when key not found
       expect(data.keyId).toBe(99999);
       expect(data.status).toBe('error');
     });
  });

  describe('POST /api/health/check-all', () => {
it('should return 200 with success when there are no keys to check', async () => {
       const res = await app.request('/api/health/check-all', {
         method: 'POST',
       });
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       expect(data.success).toBe(true);
     });

it('should return 200 when keys exist', async () => {
       const db = getDb();
       const { encrypted, iv, authTag } = encrypt('check-all-key');
       db.insert(schema.apiKeys).values({
         platform: 'google',
         encryptedKey: encrypted,
         iv,
         authTag,
         status: 'unknown',
         enabled: 1
       }).run();

       const res = await app.request('/api/health/check-all', {
         method: 'POST',
       });
       expect(res.status).toBe(200);
       const data = await res.json() as any;
       expect(data.success).toBe(true);
     });
  });
});
