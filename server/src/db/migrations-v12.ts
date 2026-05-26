import * as schema from './schema.js';
import type { Transaction } from './connection.js';

const MODEL_NEW_COLS = [
  'pricing_prompt REAL',
  'pricing_completion REAL',
  'free_tier INTEGER NOT NULL DEFAULT 0',
  'gateway TEXT',
  'supported_features TEXT',
  'external_url TEXT',
  'description TEXT',
  'last_synced_at TEXT',
  'source TEXT NOT NULL DEFAULT \'manual\'',
];

export function migrateModelsV12(tx: Transaction): void {
  const existing = tx.all<{ name: string }>('PRAGMA table_info(models)');
  const existingNames = new Set(existing.map(r => r.name));

  for (const colDef of MODEL_NEW_COLS) {
    const colName = colDef.split(' ')[0]!;
    if (!existingNames.has(colName)) {
      tx.run(`ALTER TABLE models ADD COLUMN ${colDef}`);
    }
  }

  const hasSyncLog = tx.get<{ name: string }>(
    `SELECT name FROM sqlite_master WHERE type='table' AND name='sync_log'`
  );
  if (!hasSyncLog) {
    tx.run(`CREATE TABLE sync_log (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      started_at TEXT NOT NULL,
      completed_at TEXT,
      status TEXT NOT NULL DEFAULT 'running',
      total_discovered INTEGER DEFAULT 0,
      added INTEGER DEFAULT 0,
      updated INTEGER DEFAULT 0,
      disabled INTEGER DEFAULT 0,
      free_to_paid INTEGER DEFAULT 0,
      paid_to_free INTEGER DEFAULT 0,
      stored_disabled INTEGER DEFAULT 0,
      error TEXT
    )`);
  }

  const hasSyncChanges = tx.get<{ name: string }>(
    `SELECT name FROM sqlite_master WHERE type='table' AND name='sync_changes'`
  );
  if (!hasSyncChanges) {
    tx.run(`CREATE TABLE sync_changes (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      sync_log_id INTEGER NOT NULL REFERENCES sync_log(id),
      change_type TEXT NOT NULL,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      display_name TEXT,
      details TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )`);
    tx.run(`CREATE INDEX IF NOT EXISTS idx_sync_changes_log_id ON sync_changes(sync_log_id)`);
  }

  const hasCustomProviders = tx.get<{ name: string }>(
    `SELECT name FROM sqlite_master WHERE type='table' AND name='custom_providers'`
  );
  if (!hasCustomProviders) {
    tx.run(`CREATE TABLE custom_providers (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL UNIQUE,
      base_url TEXT NOT NULL,
      timeout_ms INTEGER DEFAULT 15000,
      extra_headers TEXT,
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )`);
  }
}
