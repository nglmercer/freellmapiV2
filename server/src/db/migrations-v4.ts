import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export function migrateModelsV3Ranks(tx: Transaction): void {
  const ranks = [
    [1,  'openrouter',  'minimax/minimax-m2.5:free'],
    [2,  'openrouter',  'qwen/qwen3-coder:free'],
    [3,  'openrouter',  'qwen/qwen3-next-80b-a3b-instruct:free'],
    [4,  'moonshot',    'kimi-latest'],
    [5,  'cerebras',    'qwen-3-235b-a22b-instruct-2507'],
    [6,  'google',      'gemini-2.5-pro'],
    [7,  'openrouter',  'z-ai/glm-4.5-air:free'],
    [8,  'openrouter',  'openai/gpt-oss-120b:free'],
    [9,  'openrouter',  'nvidia/nemotron-3-super-120b-a12b:free'],
    [10, 'minimax',     'MiniMax-M1'],
    [11, 'mistral',     'codestral-latest'],
    [12, 'mistral',     'mistral-large-latest'],
    [13, 'mistral',     'magistral-medium-latest'],
    [14, 'google',      'gemini-2.5-flash'],
    [15, 'zhipu',       'glm-4.5-flash'],
    [16, 'groq',        'llama-3.3-70b-versatile'],
    [16, 'sambanova',   'Meta-Llama-3.3-70B-Instruct'],
    [16, 'openrouter',  'meta-llama/llama-3.3-70b-instruct:free'],
    [16, 'huggingface', 'accounts/fireworks/models/llama-v3p3-70b-instruct'],
    [17, 'openrouter',  'nousresearch/hermes-3-llama-3.1-405b:free'],
    [18, 'groq',        'meta-llama/llama-4-scout-17b-16e-instruct'],
    [19, 'openrouter',  'google/gemma-4-31b-it:free'],
    [20, 'google',      'gemini-2.5-flash-lite'],
    [21, 'github',      'gpt-4o'],
    [22, 'nvidia',      'meta/llama-3.1-70b-instruct'],
    [22, 'cloudflare',  '@cf/meta/llama-3.1-70b-instruct'],
    [23, 'cohere',      'command-r-plus-08-2024'],
  ] as const;

  for (const [rank, platform, modelId] of ranks) {
    tx.update(schema.models).set({ intelligenceRank: rank }).where(and(eq(schema.models.platform, platform), eq(schema.models.modelId, modelId))).run();
  }
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

  const additions = [
    { platform: 'openrouter', modelId: 'inclusionai/ling-2.6-flash:free', displayName: 'Ling 2.6 Flash (free)', intelligenceRank: 7, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'arcee-ai/trinity-large-preview:free', displayName: 'Trinity Large Preview (free)', intelligenceRank: 13, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-3-nano-30b-a3b:free', displayName: 'Nemotron 3 Nano 30B (free)', intelligenceRank: 22, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'openai/gpt-oss-120b:free', displayName: 'GPT-OSS 120B (free)', intelligenceRank: 6, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'openai/gpt-oss-20b:free', displayName: 'GPT-OSS 20B (free)', intelligenceRank: 18, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'meta-llama/llama-3.3-70b-instruct:free', displayName: 'Llama 3.3 70B (free)', intelligenceRank: 17, speedRank: 9, sizeLabel: 'Medium', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'sambanova', modelId: 'DeepSeek-V3.1', displayName: 'DeepSeek V3.1', intelligenceRank: 5, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'sambanova', modelId: 'DeepSeek-V3.2', displayName: 'DeepSeek V3.2', intelligenceRank: 4, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'sambanova', modelId: 'Llama-4-Maverick-17B-128E-Instruct', displayName: 'Llama 4 Maverick', intelligenceRank: 11, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 8192 },
    { platform: 'sambanova', modelId: 'gpt-oss-120b', displayName: 'GPT-OSS 120B (SambaNova)', intelligenceRank: 6, speedRank: 9, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'openai/gpt-oss-120b', displayName: 'GPT-OSS 120B (Groq)', intelligenceRank: 6, speedRank: 2, sizeLabel: 'Large', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'openai/gpt-oss-20b', displayName: 'GPT-OSS 20B (Groq)', intelligenceRank: 18, speedRank: 2, sizeLabel: 'Medium', rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'qwen/qwen3-32b', displayName: 'Qwen3 32B (Groq)', intelligenceRank: 19, speedRank: 2, sizeLabel: 'Medium', rpmLimit: 60, rpdLimit: 1000, tpmLimit: 6000, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 131072 },
    { platform: 'groq', modelId: 'llama-3.1-8b-instant', displayName: 'Llama 3.1 8B Instant', intelligenceRank: 28, speedRank: 2, sizeLabel: 'Small', rpmLimit: 30, rpdLimit: 14400, tpmLimit: 6000, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 131072 },
    { platform: 'mistral', modelId: 'devstral-latest', displayName: 'Devstral', intelligenceRank: 16, speedRank: 8, sizeLabel: 'Medium', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 131072 },
    { platform: 'mistral', modelId: 'mistral-medium-latest', displayName: 'Mistral Medium 3.5', intelligenceRank: 14, speedRank: 8, sizeLabel: 'Large', rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 131072 },
    { platform: 'github', modelId: 'openai/gpt-4.1', displayName: 'GPT-4.1 (GitHub)', intelligenceRank: 20, speedRank: 7, sizeLabel: 'Large', rpmLimit: 10, rpdLimit: 50, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~9M', contextWindow: 128000 },
    { platform: 'cohere', modelId: 'command-a-03-2025', displayName: 'Command-A (03-2025)', intelligenceRank: 27, speedRank: 11, sizeLabel: 'Large', rpmLimit: 20, rpdLimit: 33, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~1-2M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/openai/gpt-oss-120b', displayName: 'GPT-OSS 120B (CF)', intelligenceRank: 6, speedRank: 11, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/zai-org/glm-4.7-flash', displayName: 'GLM-4.7 Flash (CF)', intelligenceRank: 10, speedRank: 11, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/meta/llama-4-scout-17b-16e-instruct', displayName: 'Llama 4 Scout (CF)', intelligenceRank: 12, speedRank: 11, sizeLabel: 'Large', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
  ];

  for (const a of additions) {
    tx.insert(schema.models).values(a).onConflictDoNothing().run();
  }

  ensureFallbackEntries(tx);

  const ranks = [
    [1,  'openrouter',  'minimax/minimax-m2.5:free'],
    [2,  'openrouter',  'qwen/qwen3-coder:free'],
    [3,  'openrouter',  'qwen/qwen3-next-80b-a3b-instruct:free'],
    [4,  'sambanova',   'DeepSeek-V3.2'],
    [5,  'sambanova',   'DeepSeek-V3.1'],
    [6,  'cerebras',    'qwen-3-235b-a22b-instruct-2507'],
    [6,  'openrouter',  'openai/gpt-oss-120b:free'],
    [6,  'groq',        'openai/gpt-oss-120b'],
    [6,  'sambanova',   'gpt-oss-120b'],
    [6,  'cloudflare',  '@cf/openai/gpt-oss-120b'],
    [7,  'openrouter',  'inclusionai/ling-2.6-flash:free'],
    [8,  'openrouter',  'z-ai/glm-4.5-air:free'],
    [10, 'cloudflare',  '@cf/zai-org/glm-4.7-flash'],
    [11, 'sambanova',   'Llama-4-Maverick-17B-128E-Instruct'],
    [12, 'groq',        'meta-llama/llama-4-scout-17b-16e-instruct'],
    [12, 'cloudflare',  '@cf/meta/llama-4-scout-17b-16e-instruct'],
    [13, 'openrouter',  'arcee-ai/trinity-large-preview:free'],
    [14, 'google',      'gemini-2.5-pro'],
    [14, 'mistral',     'mistral-large-latest'],
    [14, 'mistral',     'mistral-medium-latest'],
    [16, 'mistral',     'devstral-latest'],
    [16, 'mistral',     'codestral-latest'],
    [17, 'groq',        'llama-3.3-70b-versatile'],
    [17, 'sambanova',   'Meta-Llama-3.3-70B-Instruct'],
    [17, 'cloudflare',  '@cf/meta/llama-3.3-70b-instruct-fp8-fast'],
    [17, 'openrouter',  'meta-llama/llama-3.3-70b-instruct:free'],
    [17, 'nvidia',      'meta/llama-3.1-70b-instruct'],
    [18, 'openrouter',  'openai/gpt-oss-20b:free'],
    [18, 'groq',        'openai/gpt-oss-20b'],
    [19, 'groq',        'qwen/qwen3-32b'],
    [20, 'google',      'gemini-2.5-flash'],
    [20, 'github',      'openai/gpt-4.1'],
    [21, 'mistral',     'magistral-medium-latest'],
    [22, 'openrouter',  'nvidia/nemotron-3-super-120b-a12b:free'],
    [23, 'openrouter',  'nvidia/nemotron-3-nano-30b-a3b:free'],
    [24, 'zhipu',       'glm-4.5-flash'],
    [25, 'github',      'gpt-4o'],
    [26, 'google',      'gemini-2.5-flash-lite'],
    [27, 'cohere',      'command-a-03-2025'],
    [27, 'cohere',      'command-r-plus-08-2024'],
    [28, 'groq',        'llama-3.1-8b-instant'],
  ] as const;

  for (const [r, p, m] of ranks) {
    tx.update(schema.models).set({ intelligenceRank: r }).where(and(eq(schema.models.platform, p), eq(schema.models.modelId, m))).run();
  }
}