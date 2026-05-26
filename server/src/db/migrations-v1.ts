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

  const newModels = [
    { platform: 'cerebras', modelId: 'qwen-3-coder-480b', displayName: 'Qwen3-Coder 480B', intelligenceRank: 2, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'llama-4-maverick-17b-128e-instruct', displayName: 'Llama 4 Maverick', intelligenceRank: 3, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'gpt-oss-120b', displayName: 'GPT-OSS 120B', intelligenceRank: 3, speedRank: 1, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'deepseek/deepseek-v3.1:free', displayName: 'DeepSeek V3.1 (free)', intelligenceRank: 2, speedRank: 10, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'moonshotai/kimi-k2:free', displayName: 'Kimi K2 (free)', intelligenceRank: 2, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'qwen/qwen3-coder:free', displayName: 'Qwen3 Coder (free)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'z-ai/glm-4.5-air:free', displayName: 'GLM-4.5 Air (free)', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'mistral', modelId: 'magistral-medium-latest', displayName: 'Magistral Medium', intelligenceRank: 4, speedRank: 8, sizeLabel: 'Large', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 40000 },
    { platform: 'mistral', modelId: 'codestral-latest', displayName: 'Codestral', intelligenceRank: 6, speedRank: 6, sizeLabel: 'Medium', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 32000 },
    { platform: 'zhipu', modelId: 'glm-4.5-flash', displayName: 'GLM-4.5 Flash', intelligenceRank: 5, speedRank: 4, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'moonshot', modelId: 'kimi-latest', displayName: 'Kimi Latest', intelligenceRank: 4, speedRank: 8, sizeLabel: 'Large', rpmLimit: 60, rpdLimit: null, tpmLimit: null, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 200000 },
    { platform: 'minimax', modelId: 'MiniMax-M1', displayName: 'MiniMax M1', intelligenceRank: 5, speedRank: 8, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: null, tpmLimit: 1000000, tpdLimit: null, monthlyTokenBudget: '~30M', contextWindow: 200000 },
  ];

  for (const m of newModels) {
    tx.insert(schema.models).values(m).onConflictDoNothing().run();
  }

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

  const additions = [
    { platform: 'openrouter', modelId: 'nvidia/nemotron-3-super-120b-a12b:free', displayName: 'Nemotron 3 Super 120B (free)', intelligenceRank: 2, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'qwen/qwen3-next-80b-a3b-instruct:free', displayName: 'Qwen3-Next 80B (free)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'minimax/minimax-m2.5:free', displayName: 'MiniMax M2.5 (free)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 196608 },
    { platform: 'openrouter', modelId: 'google/gemma-4-31b-it:free', displayName: 'Gemma 4 31B (free)', intelligenceRank: 5, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
  ];

  for (const a of additions) {
    tx.insert(schema.models).values(a).onConflictDoNothing().run();
  }

  ensureFallbackEntries(tx);
}