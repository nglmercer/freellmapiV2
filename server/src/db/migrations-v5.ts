import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export function migrateModelsV5(tx: Transaction): void {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-pro'))).run();
  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}

export function migrateModelsV6(tx: Transaction): void {
  const removals = [
    ['openrouter', 'arcee-ai/trinity-large-preview:free'],
  ] as const;
  for (const [p, m] of removals) {
    const model = tx.select({ id: schema.models.id }).from(schema.models).where(and(eq(schema.models.platform, p), eq(schema.models.modelId, m))).get();
    if (model) {
      tx.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, model.id)).run();
      tx.delete(schema.models).where(eq(schema.models.id, model.id)).run();
    }
  }

  tx.update(schema.models).set({ rpdLimit: 20, monthlyTokenBudget: '~3M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-flash'))).run();
  tx.update(schema.models).set({ rpdLimit: 20, monthlyTokenBudget: '~3M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-flash-lite'))).run();

  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}

export function migrateModelsV7(tx: Transaction): void {
  const removals = [
    ['openrouter', 'inclusionai/ling-2.6-flash:free'],
  ] as const;
  for (const [p, m] of removals) {
    const model = tx.select({ id: schema.models.id }).from(schema.models).where(and(eq(schema.models.platform, p), eq(schema.models.modelId, m))).get();
    if (model) {
      tx.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, model.id)).run();
      tx.delete(schema.models).where(eq(schema.models.id, model.id)).run();
    }
  }

  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}

export function migrateModelsV8(tx: Transaction): void {
  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}

export function migrateModelsV9(tx: Transaction): void {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'zai-glm-4.7'))).run();
}

export function migrateModelsV10(tx: Transaction): void {
  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}

export function migrateModelsV11(tx: Transaction): void {
  tx.update(schema.models).set({ modelId: 'qwen-3-235b-a22b-instruct-2507' }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'qwen3-235b'))).run();
  tx.update(schema.models).set({ enabled: 1, monthlyTokenBudget: '~3M (1k credits)' }).where(and(eq(schema.models.platform, 'nvidia'), eq(schema.models.modelId, 'meta/llama-3.1-70b-instruct'))).run();

  // Hardcoded model additions removed — see server/src/db/seed.ts.
  ensureFallbackEntries(tx);
}
