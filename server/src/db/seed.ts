import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, and, max, asc, isNull, count, sql } from 'drizzle-orm';

export const UNRANKED_INTELLIGENCE = 99;
export const UNRANKED_SPEED = 10;

export function ensureFallbackEntries(tx: Transaction): void {
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
