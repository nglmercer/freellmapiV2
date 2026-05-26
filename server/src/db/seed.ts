import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and, max, asc, isNull, count } from 'drizzle-orm';

export function ensureFallbackEntries(tx: Transaction): void {
  const missing = tx.select({ id: schema.models.id })
    .from(schema.models)
    .leftJoin(schema.fallbackConfig, eq(schema.models.id, schema.fallbackConfig.modelDbId))
    .where(isNull(schema.fallbackConfig.id))
    .orderBy(asc(schema.models.intelligenceRank))
    .all();

  if (missing.length > 0) {
    const maxPriorityResult = tx.select({ mx: max(schema.fallbackConfig.priority) }).from(schema.fallbackConfig).get();
    const maxPriority = maxPriorityResult?.mx ?? 0;

for (let i = 0; i < missing.length; i++) {
       tx.insert(schema.fallbackConfig).values({
         modelDbId: missing[i]!.id,
         priority: maxPriority + i + 1,
         enabled: 1
       }).run();
     }
  }
}

export function seedModels(tx: Transaction): void {
  const result = tx.select({ value: count() }).from(schema.models).get();
  if (result && result.value > 0) return;

  const modelsData = [
    { platform: 'google', modelId: 'gemini-2.5-pro', displayName: 'Gemini 2.5 Pro', intelligenceRank: 1, speedRank: 8, sizeLabel: 'Frontier', rpmLimit: 5, rpdLimit: 100, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~12M', contextWindow: 1048576 },
    { platform: 'google', modelId: 'gemini-2.5-flash', displayName: 'Gemini 2.5 Flash', intelligenceRank: 4, speedRank: 5, sizeLabel: 'Large', rpmLimit: 10, rpdLimit: 20, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 1048576 },
    { platform: 'google', modelId: 'gemini-2.5-flash-lite', displayName: 'Gemini 2.5 Flash-Lite', intelligenceRank: 8, speedRank: 3, sizeLabel: 'Medium', rpmLimit: 15, rpdLimit: 1000, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~120M', contextWindow: 1048576 },
    { platform: 'openrouter', modelId: 'deepseek/deepseek-v3.1:free', displayName: 'DeepSeek V3.1 (free)', intelligenceRank: 2, speedRank: 10, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'moonshotai/kimi-k2:free', displayName: 'Kimi K2 (free)', intelligenceRank: 2, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'qwen/qwen3-coder:free', displayName: 'Qwen3 Coder (free)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'z-ai/glm-4.5-air:free', displayName: 'GLM-4.5 Air (free)', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'qwen-3-coder-480b', displayName: 'Qwen3-Coder 480B', intelligenceRank: 2, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'llama-4-maverick-17b-128e-instruct', displayName: 'Llama 4 Maverick', intelligenceRank: 3, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'qwen3-235b', displayName: 'Qwen3 235B', intelligenceRank: 3, speedRank: 1, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 8192 },
    { platform: 'cerebras', modelId: 'gpt-oss-120b', displayName: 'GPT-OSS 120B', intelligenceRank: 3, speedRank: 1, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: null, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'github', modelId: 'openai/gpt-5', displayName: 'GPT-5 (GitHub)', intelligenceRank: 1, speedRank: 7, sizeLabel: 'Frontier', rpmLimit: 10, rpdLimit: 50, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18M', contextWindow: 128000 },
    { platform: 'sambanova', modelId: 'Meta-Llama-3.3-70B-Instruct', displayName: 'Llama 3.3 70B', intelligenceRank: 6, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: null, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 8192 },
    { platform: 'mistral', modelId: 'mistral-large-latest', displayName: 'Mistral Large 3', intelligenceRank: 7, speedRank: 8, sizeLabel: 'Large', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 131072 },
    { platform: 'mistral', modelId: 'magistral-medium-latest', displayName: 'Magistral Medium', intelligenceRank: 4, speedRank: 8, sizeLabel: 'Large', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 40000 },
    { platform: 'mistral', modelId: 'codestral-latest', displayName: 'Codestral', intelligenceRank: 6, speedRank: 6, sizeLabel: 'Medium', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 32000 },
    { platform: 'groq', modelId: 'llama-3.3-70b-versatile', displayName: 'Llama 3.3 70B', intelligenceRank: 9, speedRank: 2, sizeLabel: 'Medium', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 6000, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'llama-4-scout-17b-16e-instruct', displayName: 'Llama 4 Scout', intelligenceRank: 10, speedRank: 2, sizeLabel: 'Medium', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 6000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'nvidia', modelId: 'meta/llama-3.1-70b-instruct', displayName: 'Llama 3.1 70B (NV)', intelligenceRank: 11, speedRank: 6, sizeLabel: 'Large', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: 'credits-based', contextWindow: 131072 },
    { platform: 'cohere', modelId: 'command-r-plus-08-2024', displayName: 'Command R+ (08-2024)', intelligenceRank: 12, speedRank: 11, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 33, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~1-2M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/meta/llama-3.1-70b-instruct', displayName: 'Llama 3.1 70B (CF)', intelligenceRank: 13, speedRank: 11, sizeLabel: 'Medium', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'huggingface', modelId: 'accounts/fireworks/models/llama-v3p3-70b-instruct', displayName: 'Llama 3.3 70B (HF)', intelligenceRank: 14, speedRank: 11, sizeLabel: 'Medium', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~1-3M', contextWindow: 131072 },
    { platform: 'zhipu', modelId: 'glm-4.5-flash', displayName: 'GLM-4.5 Flash', intelligenceRank: 5, speedRank: 4, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'moonshot', modelId: 'kimi-latest', displayName: 'Kimi Latest', intelligenceRank: 4, speedRank: 8, sizeLabel: 'Large', rpmLimit: 60, rpdLimit: null, tpmLimit: null, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 200000 },
    { platform: 'minimax', modelId: 'MiniMax-M1', displayName: 'MiniMax M1', intelligenceRank: 5, speedRank: 8, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: null, tpmLimit: 1000000, tpdLimit: null, monthlyTokenBudget: '~30M', contextWindow: 200000 },
  ];

  tx.insert(schema.models).values(modelsData).run();

  const allModels = tx.select({ id: schema.models.id, intelligenceRank: schema.models.intelligenceRank })
    .from(schema.models)
    .orderBy(asc(schema.models.intelligenceRank))
    .all();

  const fallbackData = allModels.map((m, i) => ({
    modelDbId: m.id,
    priority: i + 1,
    enabled: 1
  }));

  tx.insert(schema.fallbackConfig).values(fallbackData).run();
}