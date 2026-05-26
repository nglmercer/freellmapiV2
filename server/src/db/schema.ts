import { sqliteTable, text, integer, real, uniqueIndex, index } from 'drizzle-orm/sqlite-core';
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
  pricingPrompt: real('pricing_prompt'),
  pricingCompletion: real('pricing_completion'),
  freeTier: integer('free_tier').notNull().default(0),
  gateway: text('gateway'),
  supportedFeatures: text('supported_features'),
  externalUrl: text('external_url'),
  description: text('description'),
  lastSyncedAt: text('last_synced_at'),
  source: text('source').notNull().default('manual'),
}, (table) => ({
  unq: uniqueIndex('models_platform_model_id_unique').on(table.platform, table.modelId),
}));

export const syncLog = sqliteTable('sync_log', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  startedAt: text('started_at').notNull(),
  completedAt: text('completed_at'),
  status: text('status').notNull().default('running'),
  totalDiscovered: integer('total_discovered').default(0),
  added: integer('added').default(0),
  updated: integer('updated').default(0),
  disabled: integer('disabled').default(0),
  freeToPaid: integer('free_to_paid').default(0),
  paidToFree: integer('paid_to_free').default(0),
  storedDisabled: integer('stored_disabled').default(0),
  error: text('error'),
});

export const syncChanges = sqliteTable('sync_changes', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  syncLogId: integer('sync_log_id').notNull().references(() => syncLog.id),
  changeType: text('change_type').notNull(),
  platform: text('platform').notNull(),
  modelId: text('model_id').notNull(),
  displayName: text('display_name'),
  details: text('details'),
  createdAt: text('created_at').notNull().default(sql`(datetime('now'))`),
});

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

export const customProviders = sqliteTable('custom_providers', {
  id: integer('id').primaryKey({ autoIncrement: true }),
  name: text('name').notNull(),
  baseUrl: text('base_url').notNull(),
  timeoutMs: integer('timeout_ms').default(15000),
  extraHeaders: text('extra_headers'),
  enabled: integer('enabled').notNull().default(1),
  createdAt: text('created_at').notNull().default(sql`(datetime('now'))`),
  updatedAt: text('updated_at').notNull().default(sql`(datetime('now'))`),
}, (table) => ({
  nameUnq: uniqueIndex('custom_providers_name_unique').on(table.name),
}));

export const settings = sqliteTable('settings', {
  key: text('key').primaryKey(),
  value: text('value').notNull(),
});
