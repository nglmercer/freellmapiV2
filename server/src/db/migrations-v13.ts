import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq, sql, and } from 'drizzle-orm';
import { UNRANKED_INTELLIGENCE, UNRANKED_SPEED, ensureFallbackEntries } from './seed.js';

const RANKING_COLS = [
  'intelligence_score REAL',
  'speed_tokens_per_sec REAL',
  'ranking_source TEXT',
  'last_ranked_at TEXT',
];

export function migrateModelsV13(tx: Transaction): void {
  const existing = tx.all<{ name: string }>('PRAGMA table_info(models)');
  const existingNames = new Set(existing.map(r => r.name));

  for (const colDef of RANKING_COLS) {
    const colName = colDef.split(' ')[0]!;
    if (!existingNames.has(colName)) {
      tx.run(`ALTER TABLE models ADD COLUMN ${colDef}`);
    }
  }

  // Reset hardcoded intelligence/speed ranks on every row that was inserted
  // by a legacy migration or seed. The enrichment service will repopulate them
  // from real benchmark sources on the next sync. Sentinel values:
  //   intelligence_rank = 99  → never ranked yet
  //   speed_rank        = 10  → unknown / provider-default
  tx.update(schema.models)
    .set({
      intelligenceRank: UNRANKED_INTELLIGENCE,
      speedRank: UNRANKED_SPEED,
      intelligenceScore: null,
      speedTokensPerSec: null,
      rankingSource: null,
      lastRankedAt: null,
    })
    .run();

  // Re-sequence fallback priorities so unranked rows don't all sit at 99.
  // Lower priority number = tried first. Unranked models go to the end of
  // the chain but stay enabled (they're still functional; the rank just
  // affects ordering).
  const ordered = tx.select({ id: schema.models.id })
    .from(schema.models)
    .orderBy(
      sql`CASE WHEN ${schema.models.intelligenceRank} = ${UNRANKED_INTELLIGENCE} THEN 1 ELSE 0 END`,
      schema.models.intelligenceRank,
      schema.models.id
    )
    .all();

  tx.delete(schema.fallbackConfig).run();
  for (let i = 0; i < ordered.length; i++) {
    tx.insert(schema.fallbackConfig).values({
      modelDbId: ordered[i]!.id,
      priority: i + 1,
      enabled: 1,
    }).run();
  }
}
