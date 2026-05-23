import crypto from 'crypto';
import { Database } from 'bun:sqlite';
import { drizzle, BunSQLiteDatabase } from 'drizzle-orm/bun-sqlite';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { initEncryptionKey } from '../lib/crypto.js';
import * as schema from './schema.js';
import { sql, eq, and, max, asc, isNull, count } from 'drizzle-orm';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DB_PATH = path.resolve(__dirname, '../../data/freeapi.db');

let db: BunSQLiteDatabase<typeof schema>;

export type Transaction = BunSQLiteDatabase<typeof schema>;

export function runInTransaction<T>(cb: (tx: Transaction) => T): T {
  return db.transaction(cb);
}

export function getDb(): BunSQLiteDatabase<typeof schema> {
  if (!db) {
    throw new Error('Database not initialized. Call initDb() first.');
  }
  return db;
}

export function initDb(dbPath?: string): BunSQLiteDatabase<typeof schema> {
  const resolvedPath = dbPath ?? DB_PATH;
  const isMemory = resolvedPath === ':memory:';

  if (!isMemory) {
    const dataDir = path.dirname(resolvedPath);
    if (!fs.existsSync(dataDir)) {
      fs.mkdirSync(dataDir, { recursive: true });
    }
  }

  const sqlite = new Database(resolvedPath);
  if (!isMemory) sqlite.exec('PRAGMA journal_mode = WAL');
  sqlite.exec('PRAGMA foreign_keys = ON');

  db = drizzle(sqlite, { schema });

  createTables(sqlite);
  initEncryptionKey(db);

  runInTransaction((tx) => {
    seedModels(tx);
    migrateModels(tx);
    migrateModelsV2(tx);
    migrateModelsV3Ranks(tx);
    migrateModelsV4(tx);
    migrateModelsV5(tx);
    migrateModelsV6(tx);
    migrateModelsV7(tx);
    migrateModelsV8(tx);
    migrateModelsV9(tx);
    migrateModelsV10(tx);
    migrateModelsV11(tx);
    ensureUnifiedKey(tx);
  });

  console.log(`Database initialized at ${resolvedPath}`);
  return db;
}

function createTables(sqlite: Database) {
  sqlite.exec(`
    CREATE TABLE IF NOT EXISTS models (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      display_name TEXT NOT NULL,
      intelligence_rank INTEGER NOT NULL,
      speed_rank INTEGER NOT NULL,
      size_label TEXT NOT NULL DEFAULT '',
      rpm_limit INTEGER,
      rpd_limit INTEGER,
      tpm_limit INTEGER,
      tpd_limit INTEGER,
      monthly_token_budget TEXT NOT NULL DEFAULT '',
      context_window INTEGER,
      enabled INTEGER NOT NULL DEFAULT 1,
      UNIQUE(platform, model_id)
    );

    CREATE TABLE IF NOT EXISTS api_keys (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      label TEXT NOT NULL DEFAULT '',
      encrypted_key TEXT NOT NULL,
      iv TEXT NOT NULL,
      auth_tag TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'unknown',
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      last_checked_at TEXT
    );

    CREATE TABLE IF NOT EXISTS requests (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      status TEXT NOT NULL,
      input_tokens INTEGER NOT NULL DEFAULT 0,
      output_tokens INTEGER NOT NULL DEFAULT 0,
      latency_ms INTEGER NOT NULL DEFAULT 0,
      error TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS fallback_config (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      model_db_id INTEGER NOT NULL REFERENCES models(id),
      priority INTEGER NOT NULL,
      enabled INTEGER NOT NULL DEFAULT 1,
      UNIQUE(model_db_id)
    );

    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_requests_created_at ON requests(created_at);
    CREATE INDEX IF NOT EXISTS idx_requests_platform ON requests(platform);
    CREATE INDEX IF NOT EXISTS idx_api_keys_platform ON api_keys(platform);
  `);
}

/**
 * Reusable helper to ensure every model has a corresponding entry in fallback_config.
 */
function ensureFallbackEntries(tx: Transaction) {
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
        modelDbId: missing[i].id,
        priority: maxPriority + i + 1,
        enabled: 1
      }).run();
    }
  }
}

function seedModels(tx: Transaction) {
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

  console.log(`Seeded ${modelsData.length} models and fallback config`);
}

function migrateModels(tx: Transaction) {
  // 1) Replace outdated models in-place
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

  // 2) Correct stale limits / budgets
  tx.update(schema.models).set({ rpdLimit: 20, monthlyTokenBudget: '~3M' }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-flash'))).run();
  tx.update(schema.models).set({ rpmLimit: 20 }).where(and(eq(schema.models.platform, 'sambanova'), eq(schema.models.modelId, 'Meta-Llama-3.3-70B-Instruct'))).run();
  tx.update(schema.models).set({ tpmLimit: 6000 }).where(and(eq(schema.models.platform, 'groq'), eq(schema.models.modelId, 'llama-4-scout-17b-16e-instruct'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: '~1-2M' }).where(and(eq(schema.models.platform, 'cohere'), eq(schema.models.modelId, 'command-r-plus-08-2024'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: '~1-3M' }).where(and(eq(schema.models.platform, 'huggingface'), eq(schema.models.modelId, 'accounts/fireworks/models/llama-v3p3-70b-instruct'))).run();
  tx.update(schema.models).set({ monthlyTokenBudget: 'credits-based', enabled: 0 }).where(and(eq(schema.models.platform, 'nvidia'), eq(schema.models.modelId, 'meta/llama-3.1-70b-instruct'))).run();

  // 3) Insert new models
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

function migrateModelsV2(tx: Transaction) {
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

function migrateModelsV3Ranks(tx: Transaction) {
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

function migrateModelsV4(tx: Transaction) {
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
    { platform: 'openrouter', modelId: 'inclusionai/ling-2.6-flash:free',        displayName: 'Ling 2.6 Flash (free)',         intelligenceRank: 7,  speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'arcee-ai/trinity-large-preview:free',    displayName: 'Trinity Large Preview (free)',  intelligenceRank: 13, speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-3-nano-30b-a3b:free',    displayName: 'Nemotron 3 Nano 30B (free)',    intelligenceRank: 22, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'openai/gpt-oss-120b:free',               displayName: 'GPT-OSS 120B (free)',           intelligenceRank: 6,  speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'openai/gpt-oss-20b:free',                displayName: 'GPT-OSS 20B (free)',            intelligenceRank: 18, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'meta-llama/llama-3.3-70b-instruct:free', displayName: 'Llama 3.3 70B (free)',          intelligenceRank: 17, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'sambanova',  modelId: 'DeepSeek-V3.1',                          displayName: 'DeepSeek V3.1',                 intelligenceRank: 5,  speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20,  tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'sambanova',  modelId: 'DeepSeek-V3.2',                          displayName: 'DeepSeek V3.2',                 intelligenceRank: 4,  speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20,  tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'sambanova',  modelId: 'Llama-4-Maverick-17B-128E-Instruct',     displayName: 'Llama 4 Maverick',              intelligenceRank: 11, speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 20,  tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 8192 },
    { platform: 'sambanova',  modelId: 'gpt-oss-120b',                           displayName: 'GPT-OSS 120B (SambaNova)',      intelligenceRank: 6,  speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 20,  tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M', contextWindow: 131072 },
    { platform: 'groq',       modelId: 'openai/gpt-oss-120b',                    displayName: 'GPT-OSS 120B (Groq)',           intelligenceRank: 6,  speedRank: 2,  sizeLabel: 'Large',    rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000,  monthlyTokenBudget: '~6M',  contextWindow: 131072 },
    { platform: 'groq',       modelId: 'openai/gpt-oss-20b',                     displayName: 'GPT-OSS 20B (Groq)',            intelligenceRank: 18, speedRank: 2,  sizeLabel: 'Medium',   rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000,  monthlyTokenBudget: '~6M',  contextWindow: 131072 },
    { platform: 'groq',       modelId: 'qwen/qwen3-32b',                         displayName: 'Qwen3 32B (Groq)',              intelligenceRank: 19, speedRank: 2,  sizeLabel: 'Medium',   rpmLimit: 60, rpdLimit: 1000, tpmLimit: 6000, tpdLimit: 500000,  monthlyTokenBudget: '~15M', contextWindow: 131072 },
    { platform: 'groq',       modelId: 'llama-3.1-8b-instant',                   displayName: 'Llama 3.1 8B Instant',          intelligenceRank: 28, speedRank: 2,  sizeLabel: 'Small',    rpmLimit: 30, rpdLimit: 14400, tpmLimit: 6000, tpdLimit: 500000, monthlyTokenBudget: '~15M', contextWindow: 131072 },
    { platform: 'mistral',    modelId: 'devstral-latest',                        displayName: 'Devstral',                      intelligenceRank: 16, speedRank: 8,  sizeLabel: 'Medium',   rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 131072 },
    { platform: 'mistral',    modelId: 'mistral-medium-latest',                  displayName: 'Mistral Medium 3.5',            intelligenceRank: 14, speedRank: 8,  sizeLabel: 'Large',    rpmLimit: 2, rpdLimit: null, tpmLimit: 500000, tpdLimit: null, monthlyTokenBudget: '~50-100M', contextWindow: 131072 },
    { platform: 'github',     modelId: 'openai/gpt-4.1',                         displayName: 'GPT-4.1 (GitHub)',              intelligenceRank: 20, speedRank: 7,  sizeLabel: 'Large',    rpmLimit: 10, rpdLimit: 50,  tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~9M', contextWindow: 128000 },
    { platform: 'cohere',     modelId: 'command-a-03-2025',                      displayName: 'Command-A (03-2025)',           intelligenceRank: 27, speedRank: 11, sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 33,  tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~1-2M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/openai/gpt-oss-120b',                displayName: 'GPT-OSS 120B (CF)',             intelligenceRank: 6,  speedRank: 11, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/zai-org/glm-4.7-flash',              displayName: 'GLM-4.7 Flash (CF)',            intelligenceRank: 10, speedRank: 11, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/meta/llama-4-scout-17b-16e-instruct', displayName: 'Llama 4 Scout (CF)',            intelligenceRank: 12, speedRank: 11, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
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

function migrateModelsV5(tx: Transaction) {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'google'), eq(schema.models.modelId, 'gemini-2.5-pro'))).run();
  tx.insert(schema.models).values({ platform: 'cerebras', modelId: 'zai-glm-4.7', displayName: 'GLM-4.7 (Cerebras)', intelligenceRank: 7, speedRank: 1, sizeLabel: 'Frontier', rpmLimit: 10, rpdLimit: 100, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M', contextWindow: 8192 }).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function migrateModelsV6(tx: Transaction) {
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
    { platform: 'cloudflare', modelId: '@cf/moonshotai/kimi-k2.5',                    displayName: 'Kimi K2.5 (CF)',                  intelligenceRank: 3,  speedRank: 11, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 262144 },
    { platform: 'cloudflare', modelId: '@cf/qwen/qwen3-30b-a3b-fp8',                  displayName: 'Qwen3 30B-A3B fp8 (CF)',          intelligenceRank: 7,  speedRank: 11, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~18-45M', contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/deepseek-ai/deepseek-r1-distill-qwen-32b', displayName: 'DeepSeek R1 Distill Qwen 32B (CF)', intelligenceRank: 9,  speedRank: 11, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3-5M',   contextWindow: 131072 },
    { platform: 'google',     modelId: 'gemini-3.1-flash-lite-preview',               displayName: 'Gemini 3.1 Flash-Lite Preview',   intelligenceRank: 18, speedRank: 3,  sizeLabel: 'Medium',   rpmLimit: 15, rpdLimit: 20,  tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M',  contextWindow: 1048576 },
    { platform: 'google',     modelId: 'gemini-3-flash-preview',                       displayName: 'Gemini 3 Flash Preview',          intelligenceRank: 11, speedRank: 5,  sizeLabel: 'Large',    rpmLimit: 10, rpdLimit: 20,  tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M',  contextWindow: 1048576 },
    { platform: 'google',     modelId: 'gemini-3.1-pro-preview',                       displayName: 'Gemini 3.1 Pro Preview',          intelligenceRank: 1,  speedRank: 8,  sizeLabel: 'Frontier',  rpmLimit: 5, rpdLimit: 20,  tpmLimit: 250000, tpdLimit: null, monthlyTokenBudget: '~3M',  contextWindow: 1048576 },
    { platform: 'openrouter', modelId: 'google/gemma-4-31b-it:free',                   displayName: 'Gemma 4 31B (free)',             intelligenceRank: 19, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'liquid/lfm-2.5-1.2b-instruct:free',            displayName: 'Liquid LFM 2.5 1.2B (free)',     intelligenceRank: 30, speedRank: 10, sizeLabel: 'Small',    rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 32768 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function migrateModelsV7(tx: Transaction) {
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
    { platform: 'openrouter', modelId: 'inclusionai/ling-2.6-1t:free',                           displayName: 'Ling 2.6 1T (free)',                       intelligenceRank: 4,  speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'tencent/hy3-preview:free',                               displayName: 'Tencent HY3 Preview (free)',               intelligenceRank: 7,  speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'poolside/laguna-m.1:free',                               displayName: 'Poolside Laguna M.1 (free)',               intelligenceRank: 13, speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'google/gemma-4-26b-a4b-it:free',                         displayName: 'Gemma 4 26B-A4B (free)',                   intelligenceRank: 22, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free',     displayName: 'Nemotron 3 Nano 30B Reasoning (free)',     intelligenceRank: 23, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 262144 },
    { platform: 'openrouter', modelId: 'poolside/laguna-xs.2:free',                              displayName: 'Poolside Laguna XS.2 (free)',              intelligenceRank: 26, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'openrouter', modelId: 'nvidia/nemotron-nano-9b-v2:free',                        displayName: 'Nemotron Nano 9B v2 (free)',               intelligenceRank: 28, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 128000 },
    { platform: 'openrouter', modelId: 'liquid/lfm-2.5-1.2b-thinking:free',                      displayName: 'Liquid LFM 2.5 1.2B Thinking (free)',      intelligenceRank: 30, speedRank: 10, sizeLabel: 'Small',    rpmLimit: 20, rpdLimit: 200, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~6M', contextWindow: 32768 },
    { platform: 'zhipu',      modelId: 'glm-4.7-flash',                                          displayName: 'GLM-4.7 Flash',                            intelligenceRank: 18, speedRank: 4,  sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function migrateModelsV8(tx: Transaction) {
  const additions = [
    { platform: 'sambanova',  modelId: 'DeepSeek-V3.1-cb',                          displayName: 'DeepSeek V3.1 (CB)',             intelligenceRank: 5,  speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M',     contextWindow: 131072 },
    { platform: 'sambanova',  modelId: 'gemma-3-12b-it',                            displayName: 'Gemma 3 12B (SambaNova)',        intelligenceRank: 22, speedRank: 9,  sizeLabel: 'Medium',   rpmLimit: 20, rpdLimit: 20, tpmLimit: null, tpdLimit: 200000, monthlyTokenBudget: '~3M',     contextWindow: 131072 },
    { platform: 'cloudflare', modelId: '@cf/moonshotai/kimi-k2.6',                  displayName: 'Kimi K2.6 (CF)',                 intelligenceRank: 2,  speedRank: 11, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 262144 },
    { platform: 'cloudflare', modelId: '@cf/ibm-granite/granite-4.0-h-micro',       displayName: 'Granite 4.0 H Micro (CF)',       intelligenceRank: 29, speedRank: 11, sizeLabel: 'Small',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function migrateModelsV9(tx: Transaction) {
  tx.update(schema.models).set({ enabled: 0 }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'zai-glm-4.7'))).run();
}

function migrateModelsV10(tx: Transaction) {
  const additions = [
    { platform: 'ollama', modelId: 'qwen3-coder:480b',     displayName: 'Qwen3-Coder 480B (Ollama)',    intelligenceRank: 2,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 262144 },
    { platform: 'ollama', modelId: 'mistral-large-3:675b', displayName: 'Mistral Large 3 675B (Ollama)', intelligenceRank: 3,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
    { platform: 'ollama', modelId: 'deepseek-v3.2',        displayName: 'DeepSeek V3.2 (Ollama)',        intelligenceRank: 4,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
    { platform: 'ollama', modelId: 'cogito-2.1:671b',      displayName: 'Cogito 2.1 671B (Ollama)',      intelligenceRank: 4,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
    { platform: 'ollama', modelId: 'kimi-k2-thinking',     displayName: 'Kimi K2 Thinking (Ollama)',     intelligenceRank: 5,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
    { platform: 'ollama', modelId: 'glm-4.7',              displayName: 'GLM-4.7 (Ollama)',              intelligenceRank: 6,  speedRank: 9, sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~5-10M',  contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gpt-oss:120b',         displayName: 'GPT-OSS 120B (Ollama)',         intelligenceRank: 6,  speedRank: 9, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'devstral-2:123b',      displayName: 'Devstral 2 123B (Ollama)',      intelligenceRank: 8, speedRank: 10, sizeLabel: 'Large',    rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~10-20M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gpt-oss:20b',          displayName: 'GPT-OSS 20B (Ollama)',         intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~20-30M', contextWindow: 131072 },
    { platform: 'ollama', modelId: 'gemma4:31b',           displayName: 'Gemma 4 31B (Ollama)',         intelligenceRank: 22, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~20-30M', contextWindow: 131072 },
  ];
  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function migrateModelsV11(tx: Transaction) {
  tx.update(schema.models).set({ modelId: 'qwen-3-235b-a22b-instruct-2507' }).where(and(eq(schema.models.platform, 'cerebras'), eq(schema.models.modelId, 'qwen3-235b'))).run();
  tx.update(schema.models).set({ enabled: 1, monthlyTokenBudget: '~3M (1k credits)' }).where(and(eq(schema.models.platform, 'nvidia'), eq(schema.models.modelId, 'meta/llama-3.1-70b-instruct'))).run();

  const additions = [
    { platform: 'nvidia',       modelId: 'meta/llama-3.3-70b-instruct',                       displayName: 'Llama 3.3 70B (NV)',                intelligenceRank: 17, speedRank: 6, sizeLabel: 'Large',    rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 131072 },
    { platform: 'nvidia',       modelId: 'meta/llama-4-maverick-17b-128e-instruct',           displayName: 'Llama 4 Maverick (NV)',             intelligenceRank: 11, speedRank: 6, sizeLabel: 'Large',    rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 131072 },
    { platform: 'nvidia',       modelId: 'deepseek-ai/deepseek-v4-pro',                       displayName: 'DeepSeek V4 Pro (NV)',               intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'nvidia',       modelId: 'mistralai/mistral-large-3-675b-instruct-2512',      displayName: 'Mistral Large 3 675B (NV)',          intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'nvidia',       modelId: 'minimaxai/minimax-m2.7',                            displayName: 'MiniMax M2.7 (NV)',                  intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 196608 },
    { platform: 'nvidia',       modelId: 'nvidia/nemotron-3-super-120b-a12b',                 displayName: 'Nemotron 3 Super 120B (NV)',        intelligenceRank: 22, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 262144 },
    { platform: 'nvidia',       modelId: 'nvidia/nemotron-3-nano-30b-a3b',                    displayName: 'Nemotron 3 Nano 30B (NV)',          intelligenceRank: 22, speedRank: 9, sizeLabel: 'Medium',   rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 262144 },
    { platform: 'nvidia',       modelId: 'google/gemma-4-31b-it',                             displayName: 'Gemma 4 31B (NV)',                  intelligenceRank: 19, speedRank: 9, sizeLabel: 'Medium',   rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~3M (credits)', contextWindow: 262144 },
    { platform: 'nvidia',       modelId: 'moonshotai/kimi-k2.6',                              displayName: 'Kimi K2.6 (NV)',                     intelligenceRank: 3, speedRank: 9, sizeLabel: 'Frontier', rpmLimit: 40, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2M (credits)', contextWindow: 131072 },
    { platform: 'cerebras',     modelId: 'gpt-oss-120b',                              displayName: 'GPT-OSS 120B (Cerebras)',        intelligenceRank: 6,  speedRank: 1, sizeLabel: 'Large',    rpmLimit: 30, rpdLimit: 1000, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'cerebras',     modelId: 'llama3.1-8b',                               displayName: 'Llama 3.1 8B (Cerebras)',       intelligenceRank: 28,  speedRank: 1, sizeLabel: 'Small',    rpmLimit: 30, rpdLimit: 1000, tpmLimit: 60000, tpdLimit: 1000000, monthlyTokenBudget: '~30M', contextWindow: 131072 },
    { platform: 'groq',         modelId: 'groq/compound',                             displayName: 'Compound (Groq)',                intelligenceRank: 6,  speedRank: 2, sizeLabel: 'Large',    rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'groq',         modelId: 'groq/compound-mini',                        displayName: 'Compound Mini (Groq)',          intelligenceRank: 18,  speedRank: 2, sizeLabel: 'Medium',   rpmLimit: 30, rpdLimit: 1000, tpmLimit: 8000, tpdLimit: 200000, monthlyTokenBudget: '~6M', contextWindow: 131072 },
    { platform: 'kilo',         modelId: 'nvidia/nemotron-3-super-120b-a12b:free',  displayName: 'Nemotron 3 Super 120B (Kilo)',  intelligenceRank: 22, speedRank: 9,  sizeLabel: 'Frontier', rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (200/hr)', contextWindow: 262144 },
    { platform: 'pollinations', modelId: 'openai-fast',                              displayName: 'GPT-OSS 20B (Pollinations)',    intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: null, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~? (anon)',      contextWindow: 131072 },
    { platform: 'llm7',         modelId: 'gpt-oss-20b',                              displayName: 'GPT-OSS 20B (LLM7)',            intelligenceRank: 18, speedRank: 10, sizeLabel: 'Medium',   rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7',         modelId: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo', displayName: 'Llama 3.1 8B Turbo (LLM7)', intelligenceRank: 28, speedRank: 10, sizeLabel: 'Small',    rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7',         modelId: 'codestral-latest',                          displayName: 'Codestral (LLM7)',              intelligenceRank: 16, speedRank: 8,  sizeLabel: 'Medium',   rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)',  contextWindow: 32000 },
    { platform: 'llm7',         modelId: 'ministral-8b-2512',                         displayName: 'Ministral 8B (LLM7)',           intelligenceRank: 28, speedRank: 10, sizeLabel: 'Small',    rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
    { platform: 'llm7',         modelId: 'GLM-4.6V-Flash',                            displayName: 'GLM-4.6V Flash (LLM7)',         intelligenceRank: 15, speedRank: 9,  sizeLabel: 'Large',    rpmLimit: 100, rpdLimit: null, tpmLimit: null, tpdLimit: null, monthlyTokenBudget: '~2-3M (100/hr)', contextWindow: 131072 },
  ];

  for (const a of additions) tx.insert(schema.models).values(a).onConflictDoNothing().run();

  ensureFallbackEntries(tx);
}

function ensureUnifiedKey(tx: Transaction) {
  const existing = tx.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'unified_api_key')).get();
  if (!existing) {
    const key = `freellmapi-${crypto.randomBytes(24).toString('hex')}`;
    tx.insert(schema.settings).values({ key: 'unified_api_key', value: key }).run();
    console.log(`\n  Your unified API key: ${key}\n`);
  }
}

export function getUnifiedApiKey(): string {
  const db = getDb();
  const row = db.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'unified_api_key')).get();
  if (!row) throw new Error('unified_api_key not found');
  return row.value;
}

export function regenerateUnifiedKey(): string {
  const db = getDb();
  const key = `freellmapi-${crypto.randomBytes(24).toString('hex')}`;
  db.update(schema.settings).set({ value: key }).where(eq(schema.settings.key, 'unified_api_key')).run();
  return key;
}
