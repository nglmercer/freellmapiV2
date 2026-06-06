import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export function migrateModelsV3Ranks(_tx: Transaction): void {
  // Hardcoded rank UPDATEs removed. Intelligence ranks are populated by the
  // enrichment service (server/src/services/rankings/enrich.ts) from real
  // benchmark sources. A static allowlist can't keep up with model releases
  // and produces the wrong rank for similar but distinct model names.
  return;
}

export function migrateModelsV4(tx: Transaction): void {
  const removals = [
    ['moonshot', 'kimi-latest'],
    ['minimax', 'MiniMax-M1'],
    ['openrouter', 'google/gemma-4-31b-it:free'],
    ['huggingface', 'accounts/fireworks/models/llama-v3p3-70b-instruct'],
  ] as const;

  for (const [platform, modelId] of removals) {
    const model = tx.select({ id: schema.models.id }).from(schema.models).where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelId))).get();
    if (model) {
      tx.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, model.id)).run();
      tx.delete(schema.models).where(eq(schema.models.id, model.id)).run();
    }
  }

  tx.update(schema.models)
    .set({
      modelId: '@cf/meta/llama-3.3-70b-instruct-fp8-fast',
      displayName: 'Llama 3.3 70B fp8-fast (CF)',
      contextWindow: 131072
    })
    .where(and(eq(schema.models.platform, 'cloudflare'), eq(schema.models.modelId, '@cf/meta/llama-3.1-70b-instruct')))
    .run();

  tx.update(schema.models).set({ tpmLimit: 12000 }).where(and(eq(schema.models.platform, 'groq'), eq(schema.models.modelId, 'llama-3.3-70b-versatile'))).run();
  tx.update(schema.models).set({ rpdLimit: 20 }).where(and(eq(schema.models.platform, 'sambanova'), eq(schema.models.modelId, 'Meta-Llama-3.3-70B-Instruct'))).run();
  tx.update(schema.models).set({ rpdLimit: 14400 }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'qwen-3-235b-a22b-instruct-2507'))).run();
  tx.update(schema.models).set({ rpdLimit: 250, monthlyTokenBudget: '~25M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-flash'))).run();
  tx.update(schema.models).set({ rpdLimit: 50, monthlyTokenBudget: '~6M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-pro'))).run();

  // Hardcoded model additions removed — see server/src/db/seed.ts.
  // Models are populated exclusively by the sync service + rankings enrich.
  ensureFallbackEntries(tx);

  // Hardcoded rank UPDATEs removed. Intelligence ranks are populated by the
  // enrichment service from real benchmark sources (Artificial Analysis,
  // LMArena) and never from a static list.
}