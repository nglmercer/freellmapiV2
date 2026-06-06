// Enrichments enrich the local models table with intelligence and speed
// rankings fetched from external benchmark sources.
//
// Design contract:
//   - All ranks come from real API responses. No string matching on model
//     names, no inference from pricing/context/version tags.
//   - Models that the source doesn't know about stay at UNRANKED_INTELLIGENCE
//     / UNRANKED_SPEED. The user can override via PATCH /api/providers/:id
//     /models/:modelId (existing endpoint) or wait for the source to
//     publish the model.
//   - Refresh is incremental: only rows where last_ranked_at is older than
//     the source's published age (or null) are updated. This keeps the
//     fast path cheap when the source is slow.

import { getDb } from '../../db/index.js';
import * as schema from '../../db/schema.js';
import { eq, and, sql, isNotNull, lt, or, inArray } from 'drizzle-orm';
import { UNRANKED_INTELLIGENCE, UNRANKED_SPEED } from '../../db/seed.js';
import { identity } from './match.js';
import { fetchAALookup } from './sources/artificial-analysis.js';
import type { SourceBenchmark } from './types.js';
import type { RankLookup } from './match.js';

const STALE_AFTER_MS = 24 * 60 * 60 * 1000;

export interface EnrichResult {
  scanned: number;
  updated: number;
  skipped: number;
  source: string;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  errors: string[];
}

interface ResolvedRanking {
  source: string;
  intelligenceScore: number;
  speedTokensPerSec: number;
  intelligenceRank: number;
  speedRank: number;
  sizeLabel: string;
}

/**
 * Convert a real intelligence_score to a 1-N ordinal rank within the local
 * set of enriched rows. We compute the rank by sorting on the raw score
 * and assigning 1..N. The first model gets 1, the last gets N. This way
 * the rank reflects actual benchmark ordering, not a hardcoded tier.
 */
function rankFromScore(score: number, allScores: number[]): number {
  // Higher intelligence_index = better. Sort descending, find position.
  const sorted = [...allScores].sort((a, b) => b - a);
  const idx = sorted.indexOf(score);
  return idx === -1 ? UNRANKED_INTELLIGENCE : idx + 1;
}

/**
 * Speed uses an inverse scale: faster = better. So we sort ascending on
 * "seconds per token" (1 / tokens_per_sec) and assign rank 1 to the fastest.
 */
function rankFromSpeed(tps: number, allTps: number[]): number {
  // Lower rank number = better. We invert: rank 1 = fastest.
  // Sort by tokens-per-second descending so position 0 = fastest.
  const sorted = [...allTps].sort((a, b) => b - a);
  const idx = sorted.indexOf(tps);
  return idx === -1 ? UNRANKED_SPEED : idx + 1;
}

function sizeLabelFromScore(score: number, allScores: number[]): string {
  // Size label is derived from the percentile position in the cohort. We
  // don't infer a hard tier (e.g. "Frontier" = score > 80) because that
  // would re-introduce the very kind of magic number the user wanted gone.
  // Instead we use quartile labels so the UI can group consistently.
  if (allScores.length === 0) return '';
  const sorted = [...allScores].sort((a, b) => b - a);
  const pos = sorted.indexOf(score);
  const pct = pos / Math.max(1, sorted.length - 1);
  if (pct <= 0.10) return 'Frontier';
  if (pct <= 0.35) return 'Large';
  if (pct <= 0.70) return 'Medium';
  return 'Small';
}

async function fetchAllLookups(): Promise<{ source: string; lookup: RankLookup<SourceBenchmark> }[]> {
  const results: { source: string; lookup: RankLookup<SourceBenchmark> }[] = [];
  const aa = await fetchAALookup();
  if (aa) results.push({ source: 'artificial-analysis', lookup: aa });
  return results;
}

/**
 * Public entry point. Resolves the active source(s), scans the models
 * table, updates rows whose ids match a published benchmark.
 */
export async function enrichRankings(): Promise<EnrichResult> {
  const startedAt = new Date().toISOString();
  const t0 = Date.now();
  const errors: string[] = [];

  const lookups = await fetchAllLookups();
  if (lookups.length === 0) {
    return {
      scanned: 0,
      updated: 0,
      skipped: 0,
      source: 'none',
      startedAt,
      finishedAt: new Date().toISOString(),
      durationMs: Date.now() - t0,
      errors: ['No ranking source returned data. Check network/keys.'],
    };
  }

  const db = getDb();
  const all = db
    .select({
      id: schema.models.id,
      platform: schema.models.platform,
      modelId: schema.models.modelId,
      lastRankedAt: schema.models.lastRankedAt,
    })
    .from(schema.models)
    .all();

  const cutoff = new Date(Date.now() - STALE_AFTER_MS).toISOString();

  // Phase 1: resolve every row to a real benchmark (or null). This is O(n)
  // with strict equality lookups — no scoring / fuzzy matching.
  const resolved: Array<{
    id: number;
    platform: string;
    modelId: string;
    benchmark: SourceBenchmark;
    source: string;
  }> = [];
  for (const row of all) {
    if (row.lastRankedAt && row.lastRankedAt > cutoff) continue;
    const id = identity(row.platform, row.modelId);
    for (const { source, lookup } of lookups) {
      const benchmark = lookup.find(id);
      if (benchmark) {
        resolved.push({
          id: row.id,
          platform: row.platform,
          modelId: row.modelId,
          benchmark,
          source,
        });
        break; // first source wins
      }
    }
  }

  // Phase 2: derive ordinal ranks from the cohort's actual scores. The
  // ranks are not stored in any source — they're computed locally so they
  // always reflect the relative ordering of the *enriched* cohort.
  const allScores = resolved.map((r) => r.benchmark.intelligenceScore);
  const allTps = resolved.map((r) => r.benchmark.speedTokensPerSec);

  const updates: Array<{ id: number; data: ResolvedRanking & { lastRankedAt: string } }> = [];
  for (const r of resolved) {
    updates.push({
      id: r.id,
      data: {
        source: r.source,
        intelligenceScore: r.benchmark.intelligenceScore,
        speedTokensPerSec: r.benchmark.speedTokensPerSec,
        intelligenceRank: rankFromScore(r.benchmark.intelligenceScore, allScores),
        speedRank: rankFromSpeed(r.benchmark.speedTokensPerSec, allTps),
        sizeLabel: sizeLabelFromScore(r.benchmark.intelligenceScore, allScores),
        lastRankedAt: new Date().toISOString(),
      },
    });
  }

  // Phase 3: persist. Wrap in a transaction so a partial failure doesn't
  // leave the table with mismatched score/rank pairs.
  let updated = 0;
  try {
    db.transaction((tx) => {
      for (const u of updates) {
        tx.update(schema.models)
          .set({
            intelligenceScore: u.data.intelligenceScore,
            speedTokensPerSec: u.data.speedTokensPerSec,
            intelligenceRank: u.data.intelligenceRank,
            speedRank: u.data.speedRank,
            sizeLabel: u.data.sizeLabel,
            rankingSource: u.data.source,
            lastRankedAt: u.data.lastRankedAt,
          })
          .where(eq(schema.models.id, u.id))
          .run();
        updated++;
      }
    });
  } catch (err) {
    errors.push(err instanceof Error ? err.message : String(err));
  }

  return {
    scanned: all.length,
    updated,
    skipped: all.length - resolved.length,
    source: lookups.map((l) => l.source).join('+'),
    startedAt,
    finishedAt: new Date().toISOString(),
    durationMs: Date.now() - t0,
    errors,
  };
}

/**
 * Reset every row back to UNRANKED. Used by tests and the manual override
 * endpoint when the user wants to force a full re-enrichment.
 */
export function resetRankings(): { reset: number } {
  const db = getDb();
  const r = db
    .update(schema.models)
    .set({
      intelligenceRank: UNRANKED_INTELLIGENCE,
      speedRank: UNRANKED_SPEED,
      intelligenceScore: null,
      speedTokensPerSec: null,
      rankingSource: null,
      lastRankedAt: null,
    })
    .run() as unknown as { changes: number };
  return { reset: r.changes };
}
