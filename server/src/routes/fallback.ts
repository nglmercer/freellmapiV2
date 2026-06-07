import { Hono } from 'hono';
import { z } from 'zod';
import { getDb, runInTransaction } from '../db/index.js';
import { getAllPenalties } from '../services/router.js';
import * as schema from '../db/schema.js';
import { eq, sql, asc, and, inArray } from 'drizzle-orm';

export const fallbackRouter = new Hono();
// Get fallback chain (with dynamic penalties)
fallbackRouter.get('/', async (c) => {
  const db = getDb();

  const rows = db.select({
    modelDbId: schema.fallbackConfig.modelDbId,
    priority: schema.fallbackConfig.priority,
    enabled: schema.fallbackConfig.enabled,
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
    monthlyTokenBudget: schema.models.monthlyTokenBudget,
    freeTier: schema.models.freeTier,
  })
  .from(schema.fallbackConfig)
  .innerJoin(schema.models, eq(schema.models.id, schema.fallbackConfig.modelDbId))
  .orderBy(asc(schema.fallbackConfig.priority))
  .all();

  // Count enabled keys per platform
  const keyCounts = db.select({
    platform: schema.apiKeys.platform,
    count: sql<number>`COUNT(*)`
  })
  .from(schema.apiKeys)
  .where(eq(schema.apiKeys.enabled, 1))
  .groupBy(schema.apiKeys.platform)
  .all();
  const keyCountMap = new Map(keyCounts.map(k => [k.platform, k.count]));

  // Get current dynamic penalties
  const penalties = getAllPenalties();
  const penaltyMap = new Map(penalties.map(p => [p.modelDbId, p]));

  return c.json(rows.map(r => {
    const penalty = penaltyMap.get(r.modelDbId);
    return {
      modelDbId: r.modelDbId,
      priority: r.priority,
      effectivePriority: r.priority + (penalty?.penalty ?? 0),
      penalty: penalty?.penalty ?? 0,
      rateLimitHits: penalty?.count ?? 0,
      enabled: r.enabled === 1,
      platform: r.platform,
      modelId: r.modelId,
      displayName: r.displayName,
      intelligenceRank: r.intelligenceRank,
      speedRank: r.speedRank,
      intelligenceScore: r.intelligenceScore,
      speedTokensPerSec: r.speedTokensPerSec,
      rankingSource: r.rankingSource,
      lastRankedAt: r.lastRankedAt,
      sizeLabel: r.sizeLabel,
      rpmLimit: r.rpmLimit,
      rpdLimit: r.rpdLimit,
      monthlyTokenBudget: r.monthlyTokenBudget,
      freeTier: r.freeTier === 1,
      keyCount: keyCountMap.get(r.platform) ?? 0,
    };
  }));
});

const updateSchema = z.array(z.object({
  modelDbId: z.number(),
  priority: z.number(),
  enabled: z.boolean(),
}));

// Update fallback chain (full replace)
fallbackRouter.put('/', async (c) => {
  const parsed = updateSchema.safeParse(await c.req.json());
  if (!parsed.success) {
    c.status(400)
    return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } });
  }

  const db = getDb();

  runInTransaction(() => {
    for (const entry of parsed.data) {
      db.update(schema.fallbackConfig)
        .set({ priority: entry.priority, enabled: entry.enabled ? 1 : 0 })
        .where(eq(schema.fallbackConfig.modelDbId, entry.modelDbId))
        .run();
    }
  });

  return c.json({ success: true });
});

function parseBudgetValue(s: string): number {
  if (!s) return 0;
  const m = s.match(/~?([\d.]+)(?:-([\d.]+))?([MK])?/);
  if (!m || !m[1]) return 0;
  const value = m[2] ?? m[1];
  const high = parseFloat(value);
  const unit = m[3] === 'M' ? 1_000_000 : m[3] === 'K' ? 1_000 : 1;
  return high * unit;
}

fallbackRouter.post('/sort/:preset', async (c) => {
  const preset = String(c.req.param('preset'));
  if (!['intelligence', 'speed', 'budget'].includes(preset)) {
    c.status(400)
    return c.json({ error: { message: `Unknown preset: ${preset}. Use: intelligence, speed, budget` } });
  }

  const db = getDb();
  const rows = db.select({
    id: schema.fallbackConfig.modelDbId,
    intelligenceRank: schema.models.intelligenceRank,
    speedRank: schema.models.speedRank,
    intelligenceScore: schema.models.intelligenceScore,
    speedTokensPerSec: schema.models.speedTokensPerSec,
    monthlyTokenBudget: schema.models.monthlyTokenBudget,
  })
    .from(schema.fallbackConfig)
    .innerJoin(schema.models, eq(schema.models.id, schema.fallbackConfig.modelDbId))
    .all();

  const sorted = [...rows].sort((a, b) => {
    let cmp = 0;
    if (preset === 'intelligence') {
      // Prefer real benchmark score when available; fall back to the ordinal
      // rank assigned by the enrichment service.
      const aHas = a.intelligenceScore != null;
      const bHas = b.intelligenceScore != null;
      if (aHas && bHas) cmp = (b.intelligenceScore as number) - (a.intelligenceScore as number);
      else if (aHas) return -1;
      else if (bHas) return 1;
      else cmp = a.intelligenceRank - b.intelligenceRank;
    }
    else if (preset === 'speed') {
      const aHas = a.speedTokensPerSec != null;
      const bHas = b.speedTokensPerSec != null;
      if (aHas && bHas) cmp = (b.speedTokensPerSec as number) - (a.speedTokensPerSec as number);
      else if (aHas) return -1;
      else if (bHas) return 1;
      else cmp = a.speedRank - b.speedRank;
    }
    else cmp = parseBudgetValue(b.monthlyTokenBudget) - parseBudgetValue(a.monthlyTokenBudget);
    return cmp !== 0 ? cmp : a.id - b.id;
  });

  runInTransaction((tx) => {
    for (let i = 0; i < sorted.length; i++) {
      const row = sorted[i];
      if (!row) continue;
      tx.update(schema.fallbackConfig)
        .set({ priority: i + 1 })
        .where(eq(schema.fallbackConfig.modelDbId, row.id))
        .run();
    }
  });

  return c.json({ success: true, preset });
});

// Token usage per model for the stacked bar
fallbackRouter.get('/token-usage', async (c) => {
  const db = getDb();

  // Get platforms that have enabled keys
  const platforms = db.selectDistinct({ platform: schema.apiKeys.platform })
    .from(schema.apiKeys)
    .where(eq(schema.apiKeys.enabled, 1))
    .all();
  const platformSet = new Set(platforms.map(p => p.platform));

  // Get monthly budget per model, ordered by fallback priority
  const models = db.select({
    platform: schema.models.platform,
    modelId: schema.models.modelId,
    displayName: schema.models.displayName,
    monthlyTokenBudget: schema.models.monthlyTokenBudget,
    priority: schema.fallbackConfig.priority
  })
  .from(schema.models)
  .innerJoin(schema.fallbackConfig, eq(schema.fallbackConfig.modelDbId, schema.models.id))
  .where(eq(schema.models.enabled, 1))
  .orderBy(asc(schema.fallbackConfig.priority))
  .all();

  function parseBudget(s: string | undefined): number {
    if (!s) return 0;
    const m = s.match(/~?([\d.]+)(?:-([\d.]+))?([MK])?/);
    if (!m || !m[1]) return 0;
    const value = m[2] ?? m[1];
    const high = parseFloat(value);
    const unit = m[3] === 'M' ? 1_000_000 : m[3] === 'K' ? 1_000 : 1;
    return high * unit;
  }

  // Build per-model breakdown (only platforms with keys)
  const modelBudgets = models
    .filter(m => platformSet.has(m.platform))
    .map(m => ({
      displayName: m.displayName,
      platform: m.platform,
      budget: parseBudget(m.monthlyTokenBudget),
    }));

  const totalBudget = modelBudgets.reduce((s, m) => s + m.budget, 0);

  // Tokens used this month
  const usage = db.select({
    total_used: sql<number>`COALESCE(SUM(${schema.requests.inputTokens} + ${schema.requests.outputTokens}), 0)`
  })
  .from(schema.requests)
  .where(sql`${schema.requests.createdAt} >= datetime('now', 'start of month')`)
  .get();

  return c.json({
    totalBudget,
    totalUsed: usage?.total_used ?? 0,
    models: modelBudgets,
  });
});
