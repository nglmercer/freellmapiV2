import { Hono } from 'hono';
import { getDb } from '../db/index.js';
import { checkKeyHealth, checkAllKeys } from '../services/health.js';
import { hasProvider } from '../providers/index.js';
import type { Platform } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { sql, eq, desc, asc } from 'drizzle-orm';

export const healthRouter = new Hono();

// Get health status for all platforms
healthRouter.get('/', async (c) => {
  const db = getDb();

  const platforms = db.select({
    platform: schema.apiKeys.platform,
    total_keys: sql<number>`COUNT(*)`,
    healthy_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.status} = 'healthy' THEN 1 ELSE 0 END)`,
    rate_limited_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.status} = 'rate_limited' THEN 1 ELSE 0 END)`,
    invalid_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.status} = 'invalid' THEN 1 ELSE 0 END)`,
    error_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.status} = 'error' THEN 1 ELSE 0 END)`,
    unknown_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.status} = 'unknown' THEN 1 ELSE 0 END)`,
    enabled_keys: sql<number>`SUM(CASE WHEN ${schema.apiKeys.enabled} = 1 THEN 1 ELSE 0 END)`
  })
  .from(schema.apiKeys)
  .groupBy(schema.apiKeys.platform)
  .all();

  const keys = db.select({
    id: schema.apiKeys.id,
    platform: schema.apiKeys.platform,
    label: schema.apiKeys.label,
    status: schema.apiKeys.status,
    enabled: schema.apiKeys.enabled,
    createdAt: schema.apiKeys.createdAt,
    lastCheckedAt: schema.apiKeys.lastCheckedAt
  })
  .from(schema.apiKeys)
  .orderBy(schema.apiKeys.platform, desc(schema.apiKeys.createdAt))
  .all();

   return c.json({
     platforms: platforms.map(p => ({
       platform: p.platform as Platform,
       hasProvider: hasProvider(p.platform as Platform),
       totalKeys: p.total_keys,
       healthyKeys: p.healthy_keys ?? 0,
       rateLimitedKeys: p.rate_limited_keys ?? 0,
       invalidKeys: p.invalid_keys ?? 0,
       errorKeys: p.error_keys ?? 0,
       unknownKeys: p.unknown_keys ?? 0,
       enabledKeys: p.enabled_keys ?? 0,
     })),
     keys: keys.map(k => ({
       id: k.id,
       platform: k.platform as Platform,
       label: k.label,
       status: k.status,
       enabled: k.enabled === 1,
       createdAt: k.createdAt,
       lastCheckedAt: k.lastCheckedAt,
     })),
   });
});

// Check a specific key
healthRouter.post('/check/:keyId', async (c) => {
  const keyId = parseInt(c.req.param('keyId'), 10);
  if (isNaN(keyId)) {
    c.status(400)
    return c.json({ error: { message: 'Invalid key ID' } });
  }

  const status = await checkKeyHealth(keyId);
  return c.json({ keyId, status });
});

// Check all keys
healthRouter.post('/check-all', async (c) => {
  await checkAllKeys();
  c.status(200)
  return c.json({ success: true });
});
