import { Hono } from 'hono';
import { z } from 'zod';
import { getDb, runInTransaction } from '../db/index.js';
import { getAllPenalties } from '../services/router.js';

export const fallbackRouter = new Hono();
// Get fallback chain (with dynamic penalties)
fallbackRouter.get('/', async (c) => {
  const db = getDb();
  const rows = db.query(`
    SELECT fc.model_db_id, fc.priority, fc.enabled,
           m.platform, m.model_id, m.display_name, m.intelligence_rank,
           m.speed_rank, m.size_label, m.rpm_limit, m.rpd_limit,
           m.monthly_token_budget
    FROM fallback_config fc
    JOIN models m ON m.id = fc.model_db_id
    ORDER BY fc.priority ASC
  `).all() as any[];

  // Count enabled keys per platform
  const keyCounts = db.query(`
    SELECT platform, COUNT(*) as count
    FROM api_keys WHERE enabled = 1
    GROUP BY platform
  `).all() as { platform: string; count: number }[];
  const keyCountMap = new Map(keyCounts.map(k => [k.platform, k.count]));

  // Get current dynamic penalties
  const penalties = getAllPenalties();
  const penaltyMap = new Map(penalties.map(p => [p.modelDbId, p]));

  return c.json(rows.map(r => {
    const penalty = penaltyMap.get(r.model_db_id);
    return {
      modelDbId: r.model_db_id,
      priority: r.priority,
      effectivePriority: r.priority + (penalty?.penalty ?? 0),
      penalty: penalty?.penalty ?? 0,
      rateLimitHits: penalty?.count ?? 0,
      enabled: r.enabled === 1,
      platform: r.platform,
      modelId: r.model_id,
      displayName: r.display_name,
      intelligenceRank: r.intelligence_rank,
      speedRank: r.speed_rank,
      sizeLabel: r.size_label,
      rpmLimit: r.rpm_limit,
      rpdLimit: r.rpd_limit,
      monthlyTokenBudget: r.monthly_token_budget,
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
  const update = db.query(`
    UPDATE fallback_config SET priority = ?, enabled = ? WHERE model_db_id = ?
  `);

  runInTransaction(db, () => {
    for (const entry of parsed.data) {
      update.run([entry.priority, entry.enabled ? 1 : 0, entry.modelDbId]);
    }
  });

  return c.json({ success: true });
});

// Sort presets — `orderBy` is selected from a fixed whitelist, never from
// user input directly, so the interpolation below is safe.
const SORT_PRESETS: Record<string, string> = {
  intelligence: 'm.intelligence_rank ASC',
  speed: 'm.speed_rank ASC',
  budget: "CASE m.monthly_token_budget WHEN '~120M' THEN 1 WHEN '~50-100M' THEN 2 WHEN '~30M' THEN 3 WHEN '~18-45M' THEN 4 WHEN '~18M' THEN 5 WHEN '~15M' THEN 6 WHEN '~12M' THEN 7 WHEN '~6M' THEN 8 WHEN '~5-10M' THEN 9 WHEN '~4M' THEN 10 ELSE 11 END ASC",
};

fallbackRouter.post('/sort/:preset', async (c) => {
  const preset = String(c.req.param('preset'));
  const orderBy = SORT_PRESETS[preset];
  if (!orderBy) {
    c.status(400)
    return c.json({ error: { message: `Unknown preset: ${preset}. Use: intelligence, speed, budget` } });
  }

  const db = getDb();
  const models = db.query(`SELECT m.id FROM models m ORDER BY ${orderBy}`).all() as { id: number }[];

  const update = db.query('UPDATE fallback_config SET priority = ? WHERE model_db_id = ?');
  runInTransaction(db, () => {
    for (let i = 0; i < models.length; i++) {
      update.run([i + 1, models[i].id]);
    }
  });

  return c.json({ success: true, preset });
});

// Token usage per model for the stacked bar
fallbackRouter.get('/token-usage', async (c) => {
  const db = getDb();

  // Get platforms that have enabled keys
  const platforms = db.query(`
    SELECT DISTINCT ak.platform
    FROM api_keys ak
    WHERE ak.enabled = 1
  `).all() as { platform: string }[];
  const platformSet = new Set(platforms.map(p => p.platform));

  // Get monthly budget per model, ordered by fallback priority
  const models = db.query(`
    SELECT m.platform, m.model_id, m.display_name, m.monthly_token_budget,
           fc.priority
    FROM models m
    JOIN fallback_config fc ON fc.model_db_id = m.id
    WHERE m.enabled = 1
    ORDER BY fc.priority ASC
  `).all() as { platform: string; model_id: string; display_name: string; monthly_token_budget: string; priority: number }[];

  function parseBudget(s: string): number {
    const m = s.match(/~?([\d.]+)(?:-([\d.]+))?([MK])?/);
    if (!m) return 0;
    const high = parseFloat(m[2] ?? m[1]);
    const unit = m[3] === 'M' ? 1_000_000 : m[3] === 'K' ? 1_000 : 1;
    return high * unit;
  }

  // Build per-model breakdown (only platforms with keys)
  const modelBudgets = models
    .filter(m => platformSet.has(m.platform))
    .map(m => ({
      displayName: m.display_name,
      platform: m.platform,
      budget: parseBudget(m.monthly_token_budget),
    }));

  const totalBudget = modelBudgets.reduce((s, m) => s + m.budget, 0);

  // Tokens used this month
  const usage = db.query(`
    SELECT
      COALESCE(SUM(input_tokens + output_tokens), 0) as total_used
    FROM requests
    WHERE created_at >= datetime('now', 'start of month')
  `).get() as { total_used: number };

  return c.json({
    totalBudget,
    totalUsed: usage.total_used,
    models: modelBudgets,
  });
});
