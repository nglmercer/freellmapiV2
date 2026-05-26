import { Database } from 'bun:sqlite';
import { drizzle, BunSQLiteDatabase } from 'drizzle-orm/bun-sqlite';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import * as schema from './schema.js';
import { initEncryptionKey } from '../lib/crypto.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const DB_PATH = path.resolve(__dirname, '../../data/freeapi.db');

let db: BunSQLiteDatabase<typeof schema> | undefined;

export type Transaction = BunSQLiteDatabase<typeof schema>;

export function runInTransaction<T>(cb: (tx: Transaction) => T): T {
  if (!db) {
    throw new Error('Database not initialized. Call initDb() first.');
  }
  return db.transaction(cb);
}

export function getDb(): BunSQLiteDatabase<typeof schema> {
  if (!db) {
    throw new Error('Database not initialized. Call initDb() first.');
  }
  return db;
}

export function resetDb(): void {
  db = undefined;
}

function createTables(sqlite: Database): void {
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
      pricing_prompt REAL,
      pricing_completion REAL,
      free_tier INTEGER NOT NULL DEFAULT 0,
      gateway TEXT,
      supported_features TEXT,
      external_url TEXT,
      description TEXT,
      last_synced_at TEXT,
      source TEXT NOT NULL DEFAULT 'manual',
      UNIQUE(platform, model_id)
    );

    CREATE TABLE IF NOT EXISTS sync_log (
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
    );

    CREATE TABLE IF NOT EXISTS sync_changes (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      sync_log_id INTEGER NOT NULL REFERENCES sync_log(id),
      change_type TEXT NOT NULL,
      platform TEXT NOT NULL,
      model_id TEXT NOT NULL,
      display_name TEXT,
      details TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
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

    CREATE TABLE IF NOT EXISTS custom_providers (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL UNIQUE,
      base_url TEXT NOT NULL,
      timeout_ms INTEGER DEFAULT 15000,
      extra_headers TEXT,
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_requests_created_at ON requests(created_at);
    CREATE INDEX IF NOT EXISTS idx_requests_platform ON requests(platform);
    CREATE INDEX IF NOT EXISTS idx_api_keys_platform ON api_keys(platform);
    CREATE INDEX IF NOT EXISTS idx_sync_changes_log_id ON sync_changes(sync_log_id);
  `);

  // Add columns that may not exist on pre-existing databases (idempotent)
  const existingCols = new Set(
    sqlite.prepare('PRAGMA table_info(models)').all()
      .map((r: any) => r.name)
  );
  const newCols = [
    ['pricing_prompt', 'REAL'],
    ['pricing_completion', 'REAL'],
    ['free_tier', 'INTEGER NOT NULL DEFAULT 0'],
    ['gateway', 'TEXT'],
    ['supported_features', 'TEXT'],
    ['external_url', 'TEXT'],
    ['description', 'TEXT'],
    ['last_synced_at', 'TEXT'],
    ['source', "TEXT NOT NULL DEFAULT 'manual'"],
  ];
  for (const [name, def] of newCols) {
    if (!existingCols.has(name)) {
      sqlite.exec(`ALTER TABLE models ADD COLUMN ${name} ${def}`);
    }
  }
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

  db = drizzle(sqlite, { schema }) as BunSQLiteDatabase<typeof schema>;

  createTables(sqlite);
  initEncryptionKey(db);
  return db;
}