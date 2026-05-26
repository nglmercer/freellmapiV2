import crypto from 'crypto';
import * as schema from './schema.js';
import { eq } from 'drizzle-orm';
import type { Transaction } from './connection.js';
import { getDb } from './connection.js';

export function ensureUnifiedKey(tx: Transaction): void {
  const existing = tx.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'unified_api_key')).get();
  if (!existing) {
    const key = crypto.randomBytes(24).toString('hex');
    tx.insert(schema.settings).values({ key: 'unified_api_key', value: key }).run();
  }
}

export function getUnifiedApiKey(): string {
  const db = getDb();
   
  const row = db.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'unified_api_key')).get();
  if (!row) {
    const defaultKey = crypto.randomBytes(24).toString('hex');
    db.insert(schema.settings).values({ key: 'unified_api_key', value: defaultKey }).run();
    return defaultKey;
  }
  return row.value;
}

export function regenerateUnifiedKey(): string {
  const db = getDb();
  const key = crypto.randomBytes(24).toString('hex');
  db.update(schema.settings).set({ value: key }).where(eq(schema.settings.key, 'unified_api_key')).run();
  return key;
}