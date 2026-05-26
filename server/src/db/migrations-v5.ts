import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export function migrateModelsV5(tx: Transaction): void {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-pro'))).run();
  tx.insert(schema.models).values({ platform: 'cerebras', modelId: 'zai-glm-4.7', displayName: 'GLM-4.7 (Cerebras)', intelligenceRank: 7, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 10, rpdLimit: 100, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 8192 }).onConflictDoNothing().run();
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

  const additions = [
    { platform: 'cloudflare', modelId: '@cf/moonshotai/kimi-k2.5', displayName: 'Kimi K2.5 (CF)', intelligenceRank: 3, speedRank: 11, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 262144 },
    { platform: 'cloudflare', modelId: '@cf/qwen/qwen3-30b-a3b-fp8', displayName: 'Qwen3 30B-A3B fp8 (CF)', intelligenceRank: 7, speedRank: 11, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/deepseek-ai/deepseek-r1-distill-qwen-32b', displayName: 'DeepSeek R1 Distill Qwen 32B (CF)', intelligenceRank: 9, speedRank: 11, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3-5M', contextWindow: 131072 },
    { platform: 'google', modelId: 'gemini-3.1-flash-lite-preview', displayName: 'Gemini 3.1 Flash-Lite Preview', intelligenceRank: 18, speedRank: 3, sizeLabel: 'Medium', rpmLimit: 15, rpdLimit: 20, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 1048576 },
    { platform: 'google', modelId: 'gemini-3-flash-preview', displayName: 'Gemini 3 Flash Preview', intelligenceRank: 11, speedRank: 5, sizeLabel: 'Large', rpmLimit: 10, rpdLimit: 20, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 1048576 },
    { platform: 'google', modelId: 'gemini-3.1-pro-preview', displayName: 'Gemini 3.1 Pro Preview', intelligenceRank: 1, speedRank: 8, sizeLabel: 'Frontier', rpmLimit: 5, rpdLimit: 20, tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 1048576 },
    { platform: 'openrouter', modelId: 'google/gemma-4-31b-it:free', displayName: 'Gemma 4 31B (free)', intelligenceRank: 19, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'liquid/lfm-2.5-1.2b-instruct:free', displayName: 'Liquid LFM 2.5 1.2B (free)', intelligenceRank: 30, speedRank: 10, sizeLabel: 'Small', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 32768 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();
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

  const additions = [
    { platform: 'openrouter', modelId: 'inclusionai/ling-2.6-1t:free', displayName: 'Ling 2.6 1T (free)', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'tencent/hy3-preview:free', displayName: 'Tencent HY3 Preview (free)', intelligenceRank: 7, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'poolside/laguna-m.1:free', displayName: 'Poolside Laguna M.1 (free)', intelligenceRank: 13, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'google/gemma-4-26b-a4b-it:free', displayName: 'Gemma 4 26B-A4B (free)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free', displayName: 'Nemotron 3 Nano 30B Reasoning (free)', intelligenceRank: 23, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'poolside/laguna-xs.2:free', displayName: 'Poolside Laguna XS.2 (free)', intelligenceRank: 26, speedRank: 10, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-nano-9b-v2:free', displayName: 'Nemotron Nano 9B v2 (free)', intelligenceRank: 28, speedRank: 10, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 128000 },
    { platform: 'openrouter', modelId: 'liquid/lfm-2.5-1.2b-thinking:free', displayName: 'Liquid LFM 2.5 1.2B Thinking (free)', intelligenceRank: 30, speedRank: 10, sizeLabel: 'Small', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 32768 },
    { platform: 'zhipu', modelId: 'glm-4.7-flash', displayName: 'GLM-4.7 Flash', intelligenceRank: 18, speedRank: 4, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();
  ensureFallbackEntries(tx);
}

export function migrateModelsV8(tx: Transaction): void {
  const additions = [
    { platform: 'sambanova', modelId: 'DeepSeek-V3.1-cb', displayName: 'DeepSeek V3.1 (CB)', intelligenceRank: 5, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'sambanova', modelId: 'gemma-3-12b-it', displayName: 'Gemma 3 12B (SambaNova)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/moonshotai/kimi-k2.6', displayName: 'Kimi K2.6 (CF)', intelligenceRank: 2, speedRank: 11, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 262144 },
    { platform: 'cloudflare', modelId: '@cf/ibm-granite/granite-4.0-h-micro', displayName: 'Granite 4.0 H Micro (CF)', intelligenceRank: 29, speedRank: 11, sizeLabel: 'Small', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();
  ensureFallbackEntries(tx);
}

export function migrateModelsV9(tx: Transaction): void {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'zai-glm-4.7'))).run();
}

export function migrateModelsV10(tx: Transaction): void {
  const additions = [
    { platform: 'ollama', modelId: 'qwen3-coder:480b', displayName: 'Qwen3-Coder 480B (Ollama)', intelligenceRank: 2, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 262144 },
    { platform: 'ollama', modelId: 'mistral-large-3:675b', displayName: 'Mistral Large 3 675B (Ollama)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'deepseek-v3.2', displayName: 'DeepSeek V3.2 (Ollama)', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'cogito-2.1:671b', displayName: 'Cogito 2.1 671B (Ollama)', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'kimi-k2-thinking', displayName: 'Kimi K2 Thinking (Ollama)', intelligenceRank: 5, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'glm-4.7', displayName: 'GLM-4.7 (Ollama)', intelligenceRank: 6, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gpt-oss:120b', displayName: 'GPT-OSS 120B (Ollama)', intelligenceRank: 6, speedRank: 9, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'devstral-2:123b', displayName: 'Devstral 2 123B (Ollama)', intelligenceRank: 8, speedRank: 10, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gpt-oss:20b', displayName: 'GPT-OSS 20B (Ollama)', intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~20-30M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gemma4:31b', displayName: 'Gemma 4 31B (Ollama)', intelligenceRank: 22, speedRank: 10, sizeLabel: 'Medium', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~20-30M', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();
  ensureFallbackEntries(tx);
}

export function migrateModelsV11(tx: Transaction): void {
  tx.update(schema.models).set({ modelId: 'qwen-3-235b-a22b-instruct-2507' }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'qwen3-235b'))).run();
  tx.update(schema.models).set({ enabled: 1, monthlyTokenBudget: '~3M (1k credits)' }).where(and(eq(schema.models.platform, 'nvidia'), eq(schema.models.modelId, 'meta/llama-3.1-70b-instruct'))).run();

  const additions = [
    { platform: 'nvidia', modelId: 'meta/llama-3.3-70b-instruct', displayName: 'Llama 3.3 70B (NV)', intelligenceRank: 17, speedRank: 6, sizeLabel: 'Large', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 131072 },
    { platform: 'nvidia', modelId: 'meta/llama-4-maverick-17b-128e-instruct', displayName: 'Llama 4 Maverick (NV)', intelligenceRank: 11, speedRank: 6, sizeLabel: 'Large', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 131072 },
    { platform: 'nvidia', modelId: 'deepseek-ai/deepseek-v4-pro', displayName: 'DeepSeek V4 Pro (NV)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'nvidia', modelId: 'mistralai/mistral-large-3-675b-instruct-2512', displayName: 'Mistral Large 3 675B (NV)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'nvidia', modelId: 'minimaxai/minimax-m2.7', displayName: 'MiniMax M2.7 (NV)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 196608 },
    { platform: 'nvidia', modelId: 'nvidia/nemotron-3-super-120b-a12b', displayName: 'Nemotron 3 Super 120B (NV)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 262144 },
    { platform: 'nvidia', modelId: 'nvidia/nemotron-3-nano-30b-a3b', displayName: 'Nemotron 3 Nano 30B (NV)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 262144 },
    { platform: 'nvidia', modelId: 'google/gemma-4-31b-it', displayName: 'Gemma 4 31B (NV)', intelligenceRank: 19, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 262144 },
    { platform: 'nvidia', modelId: 'moonshotai/kimi-k2.6', displayName: 'Kimi K2.6 (NV)', intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'gpt-oss-120b', displayName: 'GPT-OSS 120B (Cerebras)', intelligenceRank: 6, speedRank: 1, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras', modelId: 'llama3.1-8b', displayName: 'Llama 3.1 8B (Cerebras)', intelligenceRank: 28, speedRank: 1, sizeLabel: 'Small', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'groq/compound', displayName: 'Compound (Groq)', intelligenceRank: 6, speedRank: 2, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'groq/compound-mini', displayName: 'Compound Mini (Groq)', intelligenceRank: 18, speedRank: 2, sizeLabel: 'Medium', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'kilo', modelId: 'nvidia/nemotron-3-super-120b-a12b:free', displayName: 'Nemotron 3 Super 120B (Kilo)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (200/hr)', contextWindow: 262144 },
    { platform: 'pollinations', modelId: 'openai-fast', displayName: 'GPT-OSS 20B (Pollinations)', intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~? (anon)', contextWindow: 131072 },
    { platform: 'llm7', modelId: 'gpt-oss-20b', displayName: 'GPT-OSS 20B (LLM7)', intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium', rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7', modelId: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo', displayName: 'Llama 3.1 8B Turbo (LLM7)', intelligenceRank: 28, speedRank: 10, sizeLabel: 'Small', rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7', modelId: 'codestral-latest', displayName: 'Codestral (LLM7)', intelligenceRank: 16, speedRank: 8, sizeLabel: 'Medium', rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 32000 },
    { platform: 'llm7', modelId: 'ministral-8b-2512', displayName: 'Ministral 8B (LLM7)', intelligenceRank: 28, speedRank: 10, sizeLabel: 'Small', rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7', modelId: 'GLM-4.6V-Flash', displayName: 'GLM-4.6V Flash (LLM7)', intelligenceRank: 15, speedRank: 9, sizeLabel: 'Large', rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();
  ensureFallbackEntries(tx);
}