import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and, max, asc, isNull, count, sql } from 'drizzle-orm';

export const UNRANKED_INTELLIGENCE = 99;
export const UNRANKED_SPEED = 10;

export function ensureFallbackEntries(tx: Transaction): void {
  // 1. Remove orphaned entries (models that no longer exist)
  const orphaned = tx.select({ id: schema.fallbackConfig.id })
    .from(schema.fallbackConfig)
    .leftJoin(schema.models, eq(schema.fallbackConfig.modelDbId, schema.models.id))
    .where(isNull(schema.models.id))
    .all();

  if (orphaned.length > 0) {
    console.log(`[seed] Removing ${orphaned.length} orphaned fallback entries`);
    tx.delete(schema.fallbackConfig)
      .where(sql`${schema.fallbackConfig.id} IN (${sql.join(orphaned.map(o => o.id), sql`, `)})`)
      .run();
  }

  // 2. Sync enabled state: disable fallback entries for disabled models
  tx.update(schema.fallbackConfig)
    .set({ enabled: 0 })
    .where(sql`${schema.fallbackConfig.modelDbId} IN (
      SELECT ${schema.models.id} FROM ${schema.models} WHERE ${schema.models.enabled} = 0
    ) AND ${schema.fallbackConfig.enabled} = 1`)
    .run();

  // 3. Add missing entries
  const missing = tx.select({ id: schema.models.id })
    .from(schema.models)
    .leftJoin(schema.fallbackConfig, eq(schema.models.id, schema.fallbackConfig.modelDbId))
    .where(isNull(schema.fallbackConfig.id))
    .orderBy(asc(schema.models.intelligenceRank), asc(schema.models.id))
    .all();

  if (missing.length > 0) {
    const maxPriorityResult = tx.select({ mx: max(schema.fallbackConfig.priority) }).from(schema.fallbackConfig).get();
    const maxPriority = maxPriorityResult?.mx ?? 0;

    for (let i = 0; i < missing.length; i++) {
      tx.insert(schema.fallbackConfig).values({
        modelDbId: missing[i]!.id,
        priority: maxPriority + i + 1,
        enabled: 1
      }).run();
    }
  }
}

export function seedModels(_tx: Transaction): void {
  // No hardcoded models. The models table is populated exclusively by:
  //   1. The sync service fetching from getmodelsapi (server/src/services/model-sync/sync.ts)
  //   2. The custom-provider model creation endpoint
  //   3. Real-data enrichment updates intelligenceRank/speedRank from external APIs
  //      (server/src/services/rankings/enrich.ts)
  //
  // The schema defaults (intelligence_rank=99, speed_rank=10) mark rows as
  // "unranked" until the enrichment service fetches real benchmark data.
  return;
}

// used in tests
export function _countModels(tx: Transaction): number {
  const r = tx.select({ c: count() }).from(schema.models).get();
  return r?.c ?? 0;
}
