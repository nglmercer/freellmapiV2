import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export function migrateModels(tx: Transaction): void {
  tx.update(schema.models)
    .set({
      modelId: 'deepseek/deepseek-v3.1:free',
      displayName: 'DeepSeek V3.1 (free)',
      intelligenceRank: 2,
      monthlyTokenBudget: '~6M',
      rpdLimit: 200,
      contextWindow: 131072,
      sizeLabel: 'Frontier'
    })
    .where(and(eq(schema.models.platform, 'openrouter'), eq(schema.models.modelId, 'deepseek/deepseek-r1:free')))
    .run();

  tx.update(schema.models)
    .set({
      modelId: 'openai/gpt-5',
      displayName: 'GPT-5 (GitHub)',
      intelligenceRank: 1,
      monthlyTokenBudget: '~18M',
      contextWindow: 128000,
      sizeLabel: 'Frontier'
    })
    .where(and(eq(schema.models.platform, 'github'), eq(schema.models.modelId, 'gpt-4o')))
    .run();

  tx.update(schema.models).set({ rpdLimit: 20, monthlyTokenBudget: '~3M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-flash'))).run();
  tx.update(schema.models).set({ rpmLimit: 20 }).where(and(eq(schema.models.platform, 'sambanova'), eq(schema.models.modelId, 'Meta-Llama-3.3-70B-Instruct'))).run();
  tx.update(schema.models).set({ tpmLimit: 6000 }).where(and(eq(schema.models.platform, 'groq'), eq(schema.models.modelId, 'llama-4-scout-17b-16e-instruct'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: '~1-2M' }).where(and(eq(schema.models.platform, 'cohere'), eq(schema.models.modelId, 'command-r-plus-08-2024'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: '~1-3M' }).where(and(eq(schema.models.platform, 'huggingface'), eq(schema.models.modelId, 'accounts/fireworks/models/llama-v3p3-70b-instruct'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: 'credits-based', enabled: 0 }).where(and(eq(schema.models.platform, 'nvidia'), eq(schema.models.modelId, 'meta/llama-3.1-70b-instruct'))).run();

  // Model additions removed — see server/src/db/seed.ts. New rows are
  // populated exclusively by the sync service and the rankings enrichment
  // service. Historical UPDATEs above are preserved because they correct
  // rows that the sync service may also have written.
  ensureFallbackEntries(tx);
}

export function migrateModelsV2(tx: Transaction): void {
  const removals = [
    { platform: 'cerebras', modelId: 'qwen-3-coder-480b' },
    { platform: 'cerebras', modelId: 'llama-4-maverick-17b-128e-instruct' },
    { platform: 'cerebras', modelId: 'gpt-oss-120b' },
    { platform: 'openrouter', modelId: 'deepseek/deepseek-v3.1:free' },
    { platform: 'openrouter', modelId: 'moonshotai/kimi-k2:free' },
  ];

  for (const { platform, modelId } of removals) {
    const model = tx.select({ id: schema.models.id }).from(schema.models).where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelId))).get();
    if (model) {
      tx.delete(schema.fallbackConfig).where(eq(schema.fallbackConfig.modelDbId, model.id)).run();
      tx.delete(schema.models).where(eq(schema.models.id, model.id)).run();
    }
  }

  tx.update(schema.models)
    .set({
      modelId: 'gpt-4o',
      displayName: 'GPT-4o',
      intelligenceRank: 5,
      sizeLabel: 'Large',
      contextWindow: 8000,
      monthlyTokenBudget: '~18M'
    })
    .where(and(eq(schema.models.platform, 'github'), eq(schema.models.modelId, 'openai/gpt-5')))
    .run();

  tx.update(schema.models)
    .set({ modelId: 'meta-llama/llama-4-scout-17b-16e-instruct' })
    .where(and(eq(schema.models.platform, 'groq'), eq(schema.models.modelId, 'llama-4-scout-17b-16e-instruct')))
    .run();

  // Hardcoded model additions removed — see server/src/db/seed.ts.
  // Models are populated exclusively by the sync service + rankings enrich.
  ensureFallbackEntries(tx);
}