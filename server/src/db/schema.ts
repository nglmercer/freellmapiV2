import { sqliteTable, text, integer, uniqueIndex, index } from 'drizzle-orm/sqlite-core';
import { sql } from 'drizzle-orm';

export const models = sqliteTable('models', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  platform: text('platform').notNull(),
  modelId: text('model_id').notNull(),
  displayName: text('display_name').notNull(),
  intelligenceRank: integer('intelligence_rank').notNull(),
  speedRank: integer('speed_rank').notNull(),
  sizeLabel: text('size_label').notNull().default(''),
  rpmLimit: integer('rpm_limit'),
  rpdLimit: integer('rpd_limit'),
  tpmLimit: integer('tpm_limit'),
  tpdLimit: integer('tpd_limit'),
  monthlyTokenBudget: text('monthly_token_budget').notNull().default(''),
  contextWindow: integer('context_window'),
  enabled: integer('enabled').notNull().default(1),
}, (table) => ({
  unq: uniqueIndex('models_platform_model_id_unique').on(table.platform, table.modelId),
}));

export const apiKeys = sqliteTable('api_keys', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  platform: text('platform').notNull(),
  label: text('label').notNull().default(''),
  encryptedKey: text('encrypted_key').notNull(),
  iv: text('iv').notNull(),
  authTag: text('auth_tag').notNull(),
  status: text('status').notNull().default('unknown'),
  enabled: integer('enabled').notNull().default(1),
  createdAt: text('created_at').notNull().default(sql`(datetime('now'))`),
  lastCheckedAt: text('last_checked_at'),
}, (table) => ({
  platformIdx: index('idx_api_keys_platform').on(table.platform),
}));

export const requests = sqliteTable('requests', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  platform: text('platform').notNull(),
  modelId: text('model_id').notNull(),
  status: text('status').notNull(),
  inputTokens: integer('input_tokens').notNull().default(0),
  outputTokens: integer('output_tokens').notNull().default(0),
  latencyMs: integer('latency_ms').notNull().default(0),
  error: text('error'),
  createdAt: text('created_at').notNull().default(sql`(datetime('now'))`),
}, (table) => ({
  createdAtIdx: index('idx_requests_created_at').on(table.createdAt),
  platformIdx: index('idx_requests_platform').on(table.platform),
}));

export const fallbackConfig = sqliteTable('fallback_config', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  modelDbId: integer('model_db_id').notNull().references(() => models.id),
  priority: integer('priority').notNull(),
  enabled: integer('enabled').notNull().default(1),
}, (table) => ({
  modelDbIdUnq: uniqueIndex('fallback_config_model_db_id_unique').on(table.modelDbId),
}));

export const settings = sqliteTable('settings', {
  key: text('key').primaryKey(),
  value: text('value').notNull(),
});
