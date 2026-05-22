import crypto from 'crypto';
import { Database } from 'sqlite-napi'; // Tu librería nativa
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
 * Confeccionado para usar los métodos específicos de tu sqlite-napi.
 */
export function initEncryptionKey(db: Database): void {
  // 1. Check env var
  const envKey = process.env.ENCRYPTION_KEY;
  if (envKey && envKey !== 'your-64-char-hex-key-here') {
    cachedKey = parseHexKey(envKey, 'env');
    return;
  }

  // 2. Check DB for persisted key
  // Cambiado: db.query(...) -> db.query(...)
  const row = db.query("SELECT value FROM settings WHERE key = 'encryption_key'").get() as { value: string } | undefined;
  if (row) {
    cachedKey = parseHexKey(row.value, 'db');
    return;
  }

  // 3. Generate and persist
  cachedKey = crypto.randomBytes(KEY_BYTES);

  // Cambiado: db.query(...) -> db.query(...)
  // Pasamos el argumento directamente al método .run(...) del Statement de tu NAPI
  db.query("INSERT INTO settings (key, value) VALUES ('encryption_key', ?)")
    .run(cachedKey.toString('hex'));
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
