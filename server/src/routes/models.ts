import { Hono } from 'hono';
import { getDb } from '../db/index.js';
import { hasProvider } from '../providers/index.js';
import type { Platform } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { eq, sql, asc } from 'drizzle-orm';

export const modelsRouter = new Hono();

// List all models with availability info
modelsRouter.get('/', async (c) => {
  const db = getDb();

  const modelsWithFallback = db.select({
    id: schema.models.id,
    platform: schema.models.platform,
    modelId: schema.models.modelId,
    displayName: schema.models.displayName,
    intelligenceRank: schema.models.intelligenceRank,
    speedRank: schema.models.speedRank,
    sizeLabel: schema.models.sizeLabel,
    rpmLimit: schema.models.rpmLimit,
    rpdLimit: schema.models.rpdLimit,
    tpmLimit: schema.models.tpmLimit,
    tpdLimit: schema.models.tpdLimit,
    monthlyTokenBudget: schema.models.monthlyTokenBudget,
    contextWindow: schema.models.contextWindow,
    enabled: schema.models.enabled,
    priority: schema.fallbackConfig.priority,
    fallbackEnabled: schema.fallbackConfig.enabled,
  })
  .from(schema.models)
  .leftJoin(schema.fallbackConfig, eq(schema.fallbackConfig.modelDbId, schema.models.id))
  .orderBy(sql`COALESCE(${schema.fallbackConfig.priority}, ${schema.models.intelligenceRank}) ASC`)
  .all();

  // Count keys per platform
  const keyCounts = db.select({
    platform: schema.apiKeys.platform,
    count: sql<number>`COUNT(*)`
  })
  .from(schema.apiKeys)
  .where(eq(schema.apiKeys.enabled, 1))
  .groupBy(schema.apiKeys.platform)
  .all();

  const keyCountMap = new Map(keyCounts.map(k => [k.platform, k.count]));

   const result = modelsWithFallback.map(m => ({
     id: m.id,
     platform: m.platform,
     modelId: m.modelId,
     displayName: m.displayName,
     intelligenceRank: m.intelligenceRank,
     speedRank: m.speedRank,
     sizeLabel: m.sizeLabel,
     rpmLimit: m.rpmLimit,
     rpdLimit: m.rpdLimit,
     tpmLimit: m.tpmLimit,
     tpdLimit: m.tpdLimit,
     monthlyTokenBudget: m.monthlyTokenBudget,
     contextWindow: m.contextWindow,
     enabled: m.enabled === 1,
     priority: m.priority,
     fallbackEnabled: m.fallbackEnabled === 1,
     hasProvider: hasProvider(m.platform as Platform),
     keyCount: keyCountMap.get(m.platform) ?? 0,
   }));

  return c.json(result);
});
