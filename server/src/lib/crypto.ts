import crypto from 'crypto';
import type { BunSQLiteDatabase } from 'drizzle-orm/bun-sqlite';
import * as schema from '../db/schema.js';
import { eq } from 'drizzle-orm';

const ALGORITHM = 'aes-256-gcm';

let cachedKey: Buffer | null = null;

const KEY_BYTES = 32;
const KEY_HEX_LEN = KEY_BYTES * 2;

function parseHexKey(value: string, source: 'env' | 'db'): Buffer {
  if (value.length !== KEY_HEX_LEN || !/^[0-9a-fA-F]+$/.test(value)) {
    throw new Error(
      `Invalid ENCRYPTION_KEY (${source}): expected ${KEY_HEX_LEN} hex chars (32 bytes), got ${value.length} chars. ` +
      `Generate one with: node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"`,
    );
  }
  return Buffer.from(value, 'hex');
}

/**
 * Initialize encryption key from env, DB, or generate a new one.
 *
 * Priority: ENCRYPTION_KEY env var > DB-stored key > random generate.
 *
 * Auto-fix: If ENCRYPTION_KEY differs from DB key, updates DB to match env.
 */
export function initEncryptionKey(db: BunSQLiteDatabase<typeof schema>): void {
  // 1. Check ENCRYPTION_KEY env var first (auto-fix priority)
  const envKey = process.env.ENCRYPTION_KEY;
  if (envKey && envKey !== 'your-64-char-hex-key-here') {
    const parsedKey = parseHexKey(envKey, 'env');
    cachedKey = parsedKey;

    // Auto-fix: Update DB to match env key to prevent conflicts
    const existingRow = db.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'encryption_key')).get();
    if (!existingRow) {
      db.insert(schema.settings).values({ key: 'encryption_key', value: cachedKey.toString('hex') }).run();
    } else {
      db.update(schema.settings).set({ value: cachedKey.toString('hex') }).where(eq(schema.settings.key, 'encryption_key')).run();
    }

    console.log('[crypto] Using ENCRYPTION_KEY from env and syncing with database');
    return;
  }

  // 2. No ENCRYPTION_KEY. Check DB for stored key.
  const row = db.select({ value: schema.settings.value }).from(schema.settings).where(eq(schema.settings.key, 'encryption_key')).get();
  if (row) {
    cachedKey = parseHexKey(row.value, 'db');
    return;
  }

  // 3. Generate and persist a new random key.
  cachedKey = crypto.randomBytes(KEY_BYTES);
  db.insert(schema.settings).values({ key: 'encryption_key', value: cachedKey.toString('hex') }).run();
}

function getEncryptionKey(): Buffer {
  if (!cachedKey) {
    throw new Error('Encryption key not initialized. Call initEncryptionKey() first.');
  }
  return cachedKey;
}

export function encrypt(text: string): { encrypted: string; iv: string; authTag: string } {
  const key = getEncryptionKey();
  const iv = crypto.randomBytes(16);
  const cipher = crypto.createCipheriv(ALGORITHM, key, iv);

  let encrypted = cipher.update(text, 'utf8', 'hex');
  encrypted += cipher.final('hex');
  const authTag = cipher.getAuthTag().toString('hex');

  return {
    encrypted,
    iv: iv.toString('hex'),
    authTag,
  };
}

export function decrypt(encrypted: string, iv: string, authTag: string): string {
  const key = getEncryptionKey();
  const decipher = crypto.createDecipheriv(ALGORITHM, key, Buffer.from(iv, 'hex'));
  decipher.setAuthTag(Buffer.from(authTag, 'hex'));

  let decrypted = decipher.update(encrypted, 'hex', 'utf8');
  decrypted += decipher.final('utf8');
  return decrypted;
}

export function maskKey(key: string): string {
  if (key.length <= 8) return '****' + key.slice(-4);
  return key.slice(0, 4) + '...' + key.slice(-4);
}
