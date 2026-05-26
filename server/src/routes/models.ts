import { Hono } from 'hono';
import { z } from 'zod';
import { getDb } from '../db/index.js';
import { hasProvider } from '../providers/index.js';
import type { Platform } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { eq, sql, asc, and, or, like, desc } from 'drizzle-orm';
import { calculateCost, getAllPricing } from '../services/pricing.js';
import { syncModels } from '../services/model-sync/sync.js';

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

// SEARCH: /api/models/search?provider=...&gateway=...&free=true&search=...&limit=...&offset=...
modelsRouter.get('/search', async (c) => {
  const db = getDb();
  const provider = c.req.query('provider');
  const gateway = c.req.query('gateway');
  const free = c.req.query('free');
  const search = c.req.query('search');
  const limit = Math.min(parseInt(c.req.query('limit') ?? '50'), 200);
  const offset = parseInt(c.req.query('offset') ?? '0');

  const conditions = [eq(schema.models.enabled, 1)];
  if (provider) conditions.push(eq(schema.models.platform, provider));
  if (gateway) conditions.push(eq(schema.models.gateway, gateway));
  if (free === 'true') conditions.push(eq(schema.models.freeTier, 1));
  if (search) {
    const q = `%${search}%`;
    conditions.push(or(like(schema.models.displayName, q), like(schema.models.modelId, q), like(schema.models.description, q))!);
  }

  const models = db.select({
    id: schema.models.id,
    platform: schema.models.platform,
    modelId: schema.models.modelId,
    displayName: schema.models.displayName,
    contextWindow: schema.models.contextWindow,
    freeTier: schema.models.freeTier,
    gateway: schema.models.gateway,
    supportedFeatures: schema.models.supportedFeatures,
    pricingPrompt: schema.models.pricingPrompt,
    pricingCompletion: schema.models.pricingCompletion,
    externalUrl: schema.models.externalUrl,
    description: schema.models.description,
    intelligenceRank: schema.models.intelligenceRank,
    speedRank: schema.models.speedRank,
    sizeLabel: schema.models.sizeLabel,
    lastSyncedAt: schema.models.lastSyncedAt,
    source: schema.models.source,
  })
    .from(schema.models)
    .where(and(...conditions))
    .orderBy(asc(schema.models.intelligenceRank))
    .limit(limit)
    .offset(offset)
    .all();

  const total = db.select({ count: sql<number>`count(*)` })
    .from(schema.models)
    .where(and(...conditions))
    .get();

  return c.json({
    data: models.map(m => ({ ...m, freeTier: m.freeTier === 1, supportedFeatures: m.supportedFeatures ? JSON.parse(m.supportedFeatures) : [] })),
    meta: { total: total?.count ?? 0, limit, offset },
  });
});

// PRICING: /api/models/pricing
modelsRouter.get('/pricing', async (c) => {
  return c.json(getAllPricing());
});

// PRICING: POST /api/models/pricing/calculate
const calcSchema = z.object({
  platform: z.string(),
  modelId: z.string(),
  promptTokens: z.number().int().min(0),
  completionTokens: z.number().int().min(0),
});

modelsRouter.post('/pricing/calculate', async (c) => {
  const parsed = calcSchema.safeParse(await c.req.json());
  if (!parsed.success) { c.status(400); return c.json({ error: parsed.error.errors }); }
  return c.json(calculateCost(parsed.data.platform, parsed.data.modelId, parsed.data.promptTokens, parsed.data.completionTokens));
});

// POST /api/models/sync
modelsRouter.post('/sync', async (c) => {
  try {
    const result = await syncModels();
    return c.json({ success: true, ...result });
  } catch (err) {
    c.status(500);
    return c.json({ success: false, error: err instanceof Error ? err.message : String(err) });
  }
});

// GET /api/models/sync/status
modelsRouter.get('/sync/status', async (c) => {
  const db = getDb();
  const lastSync = db.select().from(schema.syncLog).orderBy(desc(schema.syncLog.id)).limit(1).get();
  const recentChanges = lastSync ? db.select().from(schema.syncChanges).where(eq(schema.syncChanges.syncLogId, lastSync.id)).all() : [];
  return c.json({ lastSync, recentChanges });
});

// GET /api/models/sync/history
modelsRouter.get('/sync/history', async (c) => {
  const db = getDb();
  const limit = Math.min(parseInt(c.req.query('limit') ?? '10'), 50);
  return c.json(db.select().from(schema.syncLog).orderBy(desc(schema.syncLog.id)).limit(limit).all());
});

// GET /api/models/sync/changes/:logId
modelsRouter.get('/sync/changes/:logId', async (c) => {
  const db = getDb();
  const logId = parseInt(c.req.param('logId'));
  if (isNaN(logId)) { c.status(400); return c.json({ error: 'Invalid logId' }); }
  return c.json(db.select().from(schema.syncChanges).where(eq(schema.syncChanges.syncLogId, logId)).all());
});
