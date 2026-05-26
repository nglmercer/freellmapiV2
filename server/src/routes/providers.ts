import { Hono } from 'hono';
import { z } from 'zod';
import { getDb } from '../db/index.js';
import * as schema from '../db/schema.js';
import { eq, and, desc, max } from 'drizzle-orm';
import { hasCustomProvider, providerIdToPlatform, platformToProviderId } from '../providers/custom.js';

export const providersRouter = new Hono();

function toProviderJson(row: typeof schema.customProviders.$inferSelect) {
  let extraHeaders: Record<string, string> | null = null;
  if (row.extraHeaders) {
    try { extraHeaders = JSON.parse(row.extraHeaders); } catch { extraHeaders = null; }
  }
  return {
    id: row.id,
    name: row.name,
    baseUrl: row.baseUrl,
    timeoutMs: row.timeoutMs,
    extraHeaders,
    enabled: row.enabled === 1,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  };
}

providersRouter.get('/', async (c) => {
  const db = getDb();
  const rows = db.select().from(schema.customProviders).orderBy(desc(schema.customProviders.id)).all();
  return c.json(rows.map(toProviderJson));
});

providersRouter.get('/:id', async (c) => {
  const id = parseInt(c.req.param('id'));
  if (isNaN(id)) { c.status(400); return c.json({ error: { message: 'Invalid ID' } }); }
  const db = getDb();
  const row = db.select().from(schema.customProviders).where(eq(schema.customProviders.id, id)).get();
  if (!row) { c.status(404); return c.json({ error: { message: 'Provider not found' } }); }
  return c.json(toProviderJson(row));
});

const createSchema = z.object({
  name: z.string().min(1).max(100),
  baseUrl: z.string().url().min(1),
  timeoutMs: z.number().int().min(1000).max(300000).optional().default(15000),
  extraHeaders: z.record(z.string(), z.string()).optional(),
  enabled: z.boolean().optional().default(true),
});

providersRouter.post('/', async (c) => {
  let body: unknown;
  try { body = await c.req.json(); } catch { c.status(400); return c.json({ error: { message: 'Malformed JSON body' } }); }
  const parsed = createSchema.safeParse(body);
  if (!parsed.success) { c.status(400); return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } }); }

  const { name, baseUrl, timeoutMs, extraHeaders, enabled } = parsed.data;

  // check for name conflict
  const db = getDb();
  const existing = db.select({ id: schema.customProviders.id })
    .from(schema.customProviders).where(eq(schema.customProviders.name, name)).get();
  if (existing) { c.status(409); return c.json({ error: { message: 'A provider with this name already exists' } }); }

  const result = db.insert(schema.customProviders).values({
    name,
    baseUrl,
    timeoutMs,
    extraHeaders: extraHeaders ? JSON.stringify(extraHeaders) : null,
    enabled: enabled ? 1 : 0,
  }).run();

  const lastRowId = (result as any).lastInsertRowid as number | undefined;
  const inserted = lastRowId ? db.select().from(schema.customProviders).where(eq(schema.customProviders.id, lastRowId)).get() : null;

  c.status(201);
  return c.json({ success: true, provider: inserted ? toProviderJson(inserted) : null });
});

const updateSchema = z.object({
  name: z.string().min(1).max(100).optional(),
  baseUrl: z.string().url().min(1).optional(),
  timeoutMs: z.number().int().min(1000).max(300000).optional(),
  extraHeaders: z.record(z.string(), z.string()).optional().nullable(),
  enabled: z.boolean().optional(),
});

providersRouter.patch('/:id', async (c) => {
  const id = parseInt(c.req.param('id'));
  if (isNaN(id)) { c.status(400); return c.json({ error: { message: 'Invalid ID' } }); }

  let body: unknown;
  try { body = await c.req.json(); } catch { c.status(400); return c.json({ error: { message: 'Malformed JSON body' } }); }
  const parsed = updateSchema.safeParse(body);
  if (!parsed.success) { c.status(400); return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } }); }

  const db = getDb();
  const existing = db.select().from(schema.customProviders).where(eq(schema.customProviders.id, id)).get();
  if (!existing) { c.status(404); return c.json({ error: { message: 'Provider not found' } }); }

  const sets: Record<string, any> = {};
  if (parsed.data.name !== undefined) sets.name = parsed.data.name;
  if (parsed.data.baseUrl !== undefined) sets.baseUrl = parsed.data.baseUrl;
  if (parsed.data.timeoutMs !== undefined) sets.timeoutMs = parsed.data.timeoutMs;
  if (parsed.data.extraHeaders !== undefined) {
    sets.extraHeaders = parsed.data.extraHeaders ? JSON.stringify(parsed.data.extraHeaders) : null;
  }
  if (parsed.data.enabled !== undefined) sets.enabled = parsed.data.enabled ? 1 : 0;
  sets.updatedAt = new Date().toISOString();

  if (Object.keys(sets).length === 0) { c.status(400); return c.json({ error: { message: 'No fields to update' } }); }

  db.update(schema.customProviders).set(sets as any).where(eq(schema.customProviders.id, id)).run();
  const updated = db.select().from(schema.customProviders).where(eq(schema.customProviders.id, id)).get();
  return c.json({ success: true, provider: updated ? toProviderJson(updated) : null });
});

providersRouter.delete('/:id', async (c) => {
  const id = parseInt(c.req.param('id'));
  if (isNaN(id)) { c.status(400); return c.json({ error: { message: 'Invalid ID' } }); }

  const db = getDb();
  const existing = db.select().from(schema.customProviders).where(eq(schema.customProviders.id, id)).get();
  if (!existing) { c.status(404); return c.json({ error: { message: 'Provider not found' } }); }

  const platform = providerIdToPlatform(id);

  // delete fallback_config entries for this provider's models
  const providerModels = db.select({ id: schema.models.id })
    .from(schema.models).where(eq(schema.models.platform, platform)).all();
  for (const m of providerModels) {
    db.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, m.id)).run();
  }

  // delete models
  db.delete(schema.models).where(eq(schema.models.platform, platform)).run();

  // delete api keys for this provider
  db.delete(schema.apiKeys).where(eq(schema.apiKeys.platform, platform)).run();

  // delete the provider
  db.delete(schema.customProviders).where(eq(schema.customProviders.id, id)).run();

  return c.json({ success: true });
});

// CUSTOM MODELS for a custom provider

const createModelSchema = z.object({
  modelId: z.string().min(1),
  displayName: z.string().min(1),
  intelligenceRank: z.number().int().min(1).max(999).optional().default(99),
  speedRank: z.number().int().min(1).max(999).optional().default(10),
  sizeLabel: z.string().optional().default(''),
  rpmLimit: z.number().int().nullable().optional().default(null),
  rpdLimit: z.number().int().nullable().optional().default(null),
  tpmLimit: z.number().int().nullable().optional().default(null),
  tpdLimit: z.number().int().nullable().optional().default(null),
  monthlyTokenBudget: z.string().optional().default(''),
  contextWindow: z.number().int().nullable().optional().default(null),
  enabled: z.boolean().optional().default(true),
});

providersRouter.get('/:id/models', async (c) => {
  const id = parseInt(c.req.param('id'));
  if (isNaN(id)) { c.status(400); return c.json({ error: { message: 'Invalid ID' } }); }

  const db = getDb();
  const platform = providerIdToPlatform(id);

  const models = db.select().from(schema.models)
    .where(eq(schema.models.platform, platform))
    .orderBy(schema.models.intelligenceRank)
    .all();

  return c.json(models.map(m => ({ ...m, enabled: m.enabled === 1 })));
});

providersRouter.post('/:id/models', async (c) => {
  const id = parseInt(c.req.param('id'));
  if (isNaN(id)) { c.status(400); return c.json({ error: { message: 'Invalid ID' } }); }

  let body: unknown;
  try { body = await c.req.json(); } catch { c.status(400); return c.json({ error: { message: 'Malformed JSON body' } }); }
  const parsed = createModelSchema.safeParse(body);
  if (!parsed.success) { c.status(400); return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } }); }

  const db = getDb();
  const platform = providerIdToPlatform(id);
  const provider = db.select().from(schema.customProviders).where(eq(schema.customProviders.id, id)).get();
  if (!provider) { c.status(404); return c.json({ error: { message: 'Provider not found' } }); }

  const existing = db.select({ id: schema.models.id })
    .from(schema.models)
    .where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, parsed.data.modelId)))
    .get();
  if (existing) { c.status(409); return c.json({ error: { message: 'Model already exists' } }); }

  const result = db.insert(schema.models).values({
    platform,
    modelId: parsed.data.modelId,
    displayName: parsed.data.displayName,
    intelligenceRank: parsed.data.intelligenceRank,
    speedRank: parsed.data.speedRank,
    sizeLabel: parsed.data.sizeLabel,
    rpmLimit: parsed.data.rpmLimit,
    rpdLimit: parsed.data.rpdLimit,
    tpmLimit: parsed.data.tpmLimit,
    tpdLimit: parsed.data.tpdLimit,
    monthlyTokenBudget: parsed.data.monthlyTokenBudget,
    contextWindow: parsed.data.contextWindow,
    enabled: parsed.data.enabled ? 1 : 0,
    source: 'custom',
    freeTier: 0,
  }).run();

  const lastRowId = (result as any).lastInsertRowid as number | undefined;
  if (lastRowId && parsed.data.enabled) {
    const mx = db.select({ mx: max(schema.fallbackConfig.priority) }).from(schema.fallbackConfig).get();
    db.insert(schema.fallbackConfig).values({
      modelDbId: lastRowId,
      priority: (mx?.mx ?? 0) + 1,
      enabled: 1,
    }).run();
  }

  c.status(201);
  return c.json({ success: true });
});

const updateModelSchema = z.object({
  displayName: z.string().min(1).optional(),
  intelligenceRank: z.number().int().min(1).max(999).optional(),
  speedRank: z.number().int().min(1).max(999).optional(),
  sizeLabel: z.string().optional(),
  rpmLimit: z.number().int().nullable().optional(),
  rpdLimit: z.number().int().nullable().optional(),
  tpmLimit: z.number().int().nullable().optional(),
  tpdLimit: z.number().int().nullable().optional(),
  monthlyTokenBudget: z.string().optional(),
  contextWindow: z.number().int().nullable().optional(),
  enabled: z.boolean().optional(),
});

providersRouter.patch('/:id/models/:modelId', async (c) => {
  const providerId = parseInt(c.req.param('id'));
  const modelIdParam = c.req.param('modelId');
  if (isNaN(providerId)) { c.status(400); return c.json({ error: { message: 'Invalid provider ID' } }); }

  let body: unknown;
  try { body = await c.req.json(); } catch { c.status(400); return c.json({ error: { message: 'Malformed JSON body' } }); }
  const parsed = updateModelSchema.safeParse(body);
  if (!parsed.success) { c.status(400); return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } }); }

  const db = getDb();
  const platform = providerIdToPlatform(providerId);

  const model = db.select().from(schema.models)
    .where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelIdParam)))
    .get();

  if (!model) { c.status(404); return c.json({ error: { message: 'Model not found' } }); }

  const sets: Record<string, any> = {};
  if (parsed.data.displayName !== undefined) sets.displayName = parsed.data.displayName;
  if (parsed.data.intelligenceRank !== undefined) sets.intelligenceRank = parsed.data.intelligenceRank;
  if (parsed.data.speedRank !== undefined) sets.speedRank = parsed.data.speedRank;
  if (parsed.data.sizeLabel !== undefined) sets.sizeLabel = parsed.data.sizeLabel;
  if (parsed.data.rpmLimit !== undefined) sets.rpmLimit = parsed.data.rpmLimit;
  if (parsed.data.rpdLimit !== undefined) sets.rpdLimit = parsed.data.rpdLimit;
  if (parsed.data.tpmLimit !== undefined) sets.tpmLimit = parsed.data.tpmLimit;
  if (parsed.data.tpdLimit !== undefined) sets.tpdLimit = parsed.data.tpdLimit;
  if (parsed.data.monthlyTokenBudget !== undefined) sets.monthlyTokenBudget = parsed.data.monthlyTokenBudget;
  if (parsed.data.contextWindow !== undefined) sets.contextWindow = parsed.data.contextWindow;
  if (parsed.data.enabled !== undefined) sets.enabled = parsed.data.enabled ? 1 : 0;

  if (Object.keys(sets).length === 0) { c.status(400); return c.json({ error: { message: 'No fields to update' } }); }

  db.update(schema.models).set(sets as any)
    .where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelIdParam)))
    .run();

  return c.json({ success: true });
});

providersRouter.delete('/:id/models/:modelId', async (c) => {
  const providerId = parseInt(c.req.param('id'));
  const modelIdParam = c.req.param('modelId');
  if (isNaN(providerId)) { c.status(400); return c.json({ error: { message: 'Invalid provider ID' } }); }

  const db = getDb();
  const platform = providerIdToPlatform(providerId);

  const model = db.select().from(schema.models)
    .where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelIdParam)))
    .get();

  if (!model) { c.status(404); return c.json({ error: { message: 'Model not found' } }); }

  db.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, model.id)).run();
  db.delete(schema.models).where(eq(schema.models.id, model.id)).run();

  return c.json({ success: true });
});
