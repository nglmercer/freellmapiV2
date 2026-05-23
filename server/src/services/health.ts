import { getDb } from '../db/index.js';
import { getProvider } from '../providers/index.js';
import { decrypt } from '../lib/crypto.js';
import type { Platform, KeyStatus } from '@freellmapi/shared/types.js';
import * as schema from '../db/schema.js';
import { eq, sql } from 'drizzle-orm';

const CHECK_INTERVAL_MS = 5 * 60 * 1000; // 5 minutes
const CONSECUTIVE_FAILURES_TO_DISABLE = 3;

// Track consecutive failures per key
const failureCount = new Map<number, number>();

export async function checkKeyHealth(keyId: number): Promise<KeyStatus> {
  const db = getDb();
  const row = db.select().from(schema.apiKeys).where(eq(schema.apiKeys.id, keyId)).get();
  if (!row) return 'error';

  const provider = getProvider(row.platform as Platform);
  if (!provider) return 'error';

  let apiKey: string;
  try {
    apiKey = decrypt(row.encryptedKey, row.iv, row.authTag);
  } catch {
    // Decryption failed — the encryption key has changed since this key was stored.
    // The key data is unrecoverable; mark as invalid and auto-disable.
    console.error(`[Health] Key ${keyId} decryption failed — encryption key mismatch`);
    const count = (failureCount.get(keyId) ?? 0) + 1;
    failureCount.set(keyId, count);

    db.update(schema.apiKeys)
      .set({ status: 'invalid', lastCheckedAt: sql`(datetime('now'))` })
      .where(eq(schema.apiKeys.id, keyId))
      .run();

    if (count >= CONSECUTIVE_FAILURES_TO_DISABLE) {
      db.update(schema.apiKeys).set({ enabled: 0 }).where(eq(schema.apiKeys.id, keyId)).run();
      console.log(`[Health] Auto-disabled key ${keyId} after ${count} consecutive decryption failures`);
      failureCount.delete(keyId);
    }
    return 'invalid';
  }

  try {
    const isValid = await provider.validateKey(apiKey);

    const status: KeyStatus = isValid ? 'healthy' : 'invalid';

    db.update(schema.apiKeys)
      .set({ status: status, lastCheckedAt: sql`(datetime('now'))` })
      .where(eq(schema.apiKeys.id, keyId))
      .run();

    if (isValid) {
      failureCount.delete(keyId);
    } else {
      const count = (failureCount.get(keyId) ?? 0) + 1;
      failureCount.set(keyId, count);

      if (count >= CONSECUTIVE_FAILURES_TO_DISABLE) {
        db.update(schema.apiKeys).set({ enabled: 0 }).where(eq(schema.apiKeys.id, keyId)).run();
        console.log(`[Health] Auto-disabled key ${keyId} after ${count} consecutive failures`);
      }
    }

    return status;
  } catch (err: unknown) {
    // Transport errors (DNS/timeout/TLS) — provider unreachable, not necessarily
    // a bad key. Mark status='error' but do NOT increment failure counter — auto-
    // disable is reserved for confirmed 401/403 (returned by validateKey as false).
    const message = err instanceof Error ? err.message : String(err);
    console.error(`[Health] Key ${keyId} transport error:`, message);
    db.update(schema.apiKeys)
      .set({ status: 'error', lastCheckedAt: sql`(datetime('now'))` })
      .where(eq(schema.apiKeys.id, keyId))
      .run();
    return 'error';
  }
}

export async function checkAllKeys(): Promise<void> {
  const db = getDb();
  const keys = db.select({ id: schema.apiKeys.id, platform: schema.apiKeys.platform })
    .from(schema.apiKeys)
    .where(eq(schema.apiKeys.enabled, 1))
    .all();

  console.log(`[Health] Checking ${keys.length} keys...`);

  for (const key of keys) {
    await checkKeyHealth(key.id);
  }

  console.log(`[Health] Check complete.`);
}

let intervalId: ReturnType<typeof setInterval> | null = null;

export function startHealthChecker(): void {
  if (intervalId) return;
  console.log(`[Health] Starting health checker (every ${CHECK_INTERVAL_MS / 1000}s)`);
  intervalId = setInterval(() => {
    checkAllKeys().catch(err => console.error('[Health] Check failed:', err));
  }, CHECK_INTERVAL_MS);
}

export function stopHealthChecker(): void {
  if (intervalId) {
    clearInterval(intervalId);
    intervalId = null;
  }
}
