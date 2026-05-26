/**
 * Probe every enabled model with a minimal request to find broken model IDs.
 * Usage: npx tsx src/scripts/test-all-models.ts
 */
import { initDb, getDb } from '../db/index.js';
import { decrypt } from '../lib/crypto.js';
import { getProvider } from '../providers/index.js';
import * as schema from '../db/schema.js';
import { eq, and, exists, asc } from 'drizzle-orm';
import type { Platform } from '@freellmapi/shared/types.js';

initDb();
const db = getDb();

const modelsWithKeys = db.select({
  id: schema.models.id,
  platform: schema.models.platform,
  modelId: schema.models.modelId,
  displayName: schema.models.displayName
})
.from(schema.models)
.where(and(
  eq(schema.models.enabled, 1),
  exists(
    db.select()
      .from(schema.apiKeys)
      .where(and(
        eq(schema.apiKeys.platform, schema.models.platform),
        eq(schema.apiKeys.enabled, 1)
      ))
  )
))
.orderBy(asc(schema.models.intelligenceRank), asc(schema.models.platform))
.all();

const results: { row: typeof modelsWithKeys[0]; ok: boolean; ms: number; error?: string; reply?: string }[] = [];

for (const row of modelsWithKeys) {
  const keyRow = db.select({
    encryptedKey: schema.apiKeys.encryptedKey,
    iv: schema.apiKeys.iv,
    authTag: schema.apiKeys.authTag
  })
  .from(schema.apiKeys)
  .where(and(
    eq(schema.apiKeys.platform, row.platform),
    eq(schema.apiKeys.enabled, 1)
  ))
  .orderBy(asc(schema.apiKeys.id))
  .limit(1)
  .get();

  if (!keyRow) {
    results.push({ row, ok: false, ms: 0, error: 'no key' });
    continue;
  }

  const apiKey = decrypt(keyRow.encryptedKey, keyRow.iv, keyRow.authTag);
  const provider = getProvider(row.platform as Platform);
  if (!provider) {
    results.push({ row, ok: false, ms: 0, error: 'no provider' });
    continue;
  }

  const start = Date.now();
  try {
    const res = await provider.chatCompletion(apiKey, [{ role: 'user', content: 'hi' }], row.modelId, { max_tokens: 5 });
    const replyContent = res.choices?.[0]?.message?.content;
    const reply = typeof replyContent === 'string' ? replyContent.slice(0, 40) : '';
    results.push({ row, ok: true, ms: Date.now() - start, reply });
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    results.push({ row, ok: false, ms: Date.now() - start, error: message.slice(0, 200) });
  }
}

console.log('\n=== Results ===\n');
const pad = (s: string, n: number) => s.length > n ? s.slice(0, n - 1) + '…' : s.padEnd(n);
for (const r of results) {
  const status = r.ok ? '✓' : '✗';
  console.log(`${status} ${pad(r.row.platform, 12)} ${pad(r.row.modelId, 52)} ${String(r.ms).padStart(5)}ms  ${r.ok ? `"${r.reply}"` : r.error}`);
}
const okCount = results.filter(r => r.ok).length;
console.log(`\n${okCount}/${results.length} models working\n`);

process.exit(0);
