import { Hono } from 'hono';
import { z } from 'zod';
import { getDb } from '../db/index.js';
import { hasProvider } from '../providers/index.js';
import type { Platform } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { eq, sql, asc, and, or, like, desc, ne, isNotNull, isNull, inArray, notInArray } from 'drizzle-orm';
import { calculateCost, getAllPricing } from '../services/pricing.js';
import { syncModels } from '../services/model-sync/sync.js';
import { enrichRankings, resetRankings } from '../services/rankings/enrich.js';

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
    intelligenceScore: schema.models.intelligenceScore,
    speedTokensPerSec: schema.models.speedTokensPerSec,
    rankingSource: schema.models.rankingSource,
    lastRankedAt: schema.models.lastRankedAt,
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
     intelligenceScore: m.intelligenceScore,
     speedTokensPerSec: m.speedTokensPerSec,
     rankingSource: m.rankingSource,
     lastRankedAt: m.lastRankedAt,
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
    intelligenceScore: schema.models.intelligenceScore,
    speedTokensPerSec: schema.models.speedTokensPerSec,
    rankingSource: schema.models.rankingSource,
    lastRankedAt: schema.models.lastRankedAt,
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

// POST /api/models/enrich-rankings
// Re-fetch intelligence/speed rankings from external benchmark sources and
// update only rows whose last_ranked_at is older than 24h (or null). Safe
// to call repeatedly.
modelsRouter.post('/enrich-rankings', async (c) => {
  try {
    const result = await enrichRankings();
    return c.json({ success: true, ...result });
  } catch (err) {
    c.status(500);
    return c.json({ success: false, error: err instanceof Error ? err.message : String(err) });
  }
});

// POST /api/models/reset-rankings
// Force a full re-enrichment by clearing last_ranked_at on every row. The
// next /enrich-rankings call will treat all models as stale.
modelsRouter.post('/reset-rankings', async (c) => {
  const r = resetRankings();
  return c.json({ success: true, ...r });
});

// GET /api/models/enrich-rankings/status
// Reports the current ranking freshness across the models table.
modelsRouter.get('/enrich-rankings/status', async (c) => {
  const db = getDb();
  const total = db.select({ c: sql<number>`count(*)` }).from(schema.models).get();
  const ranked = db
    .select({ c: sql<number>`count(*)`, oldest: sql<string | null>`min(last_ranked_at)`, newest: sql<string | null>`max(last_ranked_at)` })
    .from(schema.models)
    .where(sql`${schema.models.lastRankedAt} IS NOT NULL`)
    .get();
  const sources = db
    .select({ source: schema.models.rankingSource, c: sql<number>`count(*)` })
    .from(schema.models)
    .where(sql`${schema.models.rankingSource} IS NOT NULL`)
    .groupBy(schema.models.rankingSource)
    .all();
  return c.json({
    total: total?.c ?? 0,
    ranked: ranked?.c ?? 0,
    unranked: (total?.c ?? 0) - (ranked?.c ?? 0),
    oldestRanking: ranked?.oldest ?? null,
    newestRanking: ranked?.newest ?? null,
    bySource: sources.map((s) => ({ source: s.source, count: s.c })),
  });
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

// ─────────────────────────────────────────────────────────────────────
// Bulk enable / disable — destructive operations, require `confirm: true`.
// These affect the per-model `enabled` flag (catalog visibility) and the
// `fallback_config.enabled` flag (chain membership) atomically, so the
// router and the UI never see a half-applied state.
// ─────────────────────────────────────────────────────────────────────

const bulkSchema = z.object({
  confirm: z.literal(true),
});

/** Parse and validate the { confirm: true } body. Returns a sentinel on failure. */
async function parseBulkConfirm(c: any): Promise<{ ok: true } | { ok: false; status: number; body: unknown }> {
  let body: unknown = {};
  try {
    body = await c.req.json();
  } catch {
    body = {};
  }
  if (typeof body !== 'object' || body === null) {
    return { ok: false, status: 400, body: { error: { message: 'Invalid body. Expected { "confirm": true }.' } } };
  }
  const parsed = bulkSchema.safeParse(body);
  if (!parsed.success) {
    return { ok: false, status: 400, body: { error: { message: 'Missing confirm: true. Bulk actions are destructive and require explicit confirmation.' } } };
  }
  return { ok: true };
}

// POST /api/models/disable-all
// Disable every model in the catalog and remove every row from
// fallback_config.enabled = 0. The router will then have no models to try.
modelsRouter.post('/disable-all', async (c) => {
  const confirm = await parseBulkConfirm(c);
  if (!confirm.ok) {
    c.status(confirm.status);
    return c.json(confirm.body);
  }
  const db = getDb();
  const r1 = db.update(schema.models).set({ enabled: 0 }).run() as unknown as { changes: number };
  const r2 = db.update(schema.fallbackConfig).set({ enabled: 0 }).run() as unknown as { changes: number };
  return c.json({ success: true, modelsUpdated: r1.changes, fallbackEntriesUpdated: r2.changes });
});

// POST /api/models/enable-all
// Enable every model in the catalog and re-enable every fallback entry.
// Useful as a "reset" after bulk-disable or after seeding.
modelsRouter.post('/enable-all', async (c) => {
  const confirm = await parseBulkConfirm(c);
  if (!confirm.ok) {
    c.status(confirm.status);
    return c.json(confirm.body);
  }
  const db = getDb();
  const r1 = db.update(schema.models).set({ enabled: 1 }).run() as unknown as { changes: number };
  const r2 = db.update(schema.fallbackConfig).set({ enabled: 1 }).run() as unknown as { changes: number };
  return c.json({ success: true, modelsUpdated: r1.changes, fallbackEntriesUpdated: r2.changes });
});

// POST /api/models/enable-free
// Enable only rows whose `free_tier = 1`; disable every other row.
// This is the "free-only" preset users want when they don't want to pay
// for inference. The `free_tier` column is populated by the sync service
// from getmodelsapi (which sets it from `:free` / `free` markers in the
// model id) — never inferred locally.
modelsRouter.post('/enable-free', async (c) => {
  const confirm = await parseBulkConfirm(c);
  if (!confirm.ok) {
    c.status(confirm.status);
    return c.json(confirm.body);
  }
  const db = getDb();
  const r1 = db
    .update(schema.models)
    .set({ enabled: 1 })
    .where(eq(schema.models.freeTier, 1))
    .run() as unknown as { changes: number };
  const r2 = db
    .update(schema.models)
    .set({ enabled: 0 })
    .where(ne(schema.models.freeTier, 1))
    .run() as unknown as { changes: number };
  // Mirror the same split on the fallback chain.
  const freeIds = db
    .select({ id: schema.models.id })
    .from(schema.models)
    .where(eq(schema.models.freeTier, 1))
    .all()
    .map((r) => r.id);
  const r3 = freeIds.length > 0
    ? (db
        .update(schema.fallbackConfig)
        .set({ enabled: 1 })
        .where(inArray(schema.fallbackConfig.modelDbId, freeIds))
        .run() as unknown as { changes: number })
    : { changes: 0 };
  const r4 = freeIds.length > 0
    ? (db
        .update(schema.fallbackConfig)
        .set({ enabled: 0 })
        .where(notInArray(schema.fallbackConfig.modelDbId, freeIds))
        .run() as unknown as { changes: number })
    : (db
        .update(schema.fallbackConfig)
        .set({ enabled: 0 })
        .run() as unknown as { changes: number });
  return c.json({
    success: true,
    freeEnabled: r1.changes,
    paidDisabled: r2.changes,
    fallbackFreeEnabled: r3.changes,
    fallbackPaidDisabled: r4.changes,
  });
});
