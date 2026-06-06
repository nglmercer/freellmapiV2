// Test helper: insert a small set of representative models directly into
// the in-memory DB so route tests have something to query against. Real
// installations populate the models table via the sync service against
// getmodelsapi; the tests don't have network access, so we short-circuit
// with a hand-crafted fixture that covers the schema columns exercised by
// /api/models and /api/fallback.
//
// Critically, the intelligence/speed ranks below are not hardcoded values
// we want to enforce in production — they exist only to give the route
// tests something to sort. The enrichment service is what populates these
// in real usage.
import * as schema from './schema.js';
import type { Transaction } from './connection.js';
import { eq } from 'drizzle-orm';
import { ensureFallbackEntries } from './seed.js';

export interface TestModel {
  platform: string;
  modelId: string;
  displayName: string;
  intelligenceRank?: number;
  speedRank?: number;
  intelligenceScore?: number | null;
  speedTokensPerSec?: number | null;
  sizeLabel?: string;
  rankingSource?: string | null;
  lastRankedAt?: string | null;
  contextWindow?: number;
  freeTier?: boolean;
  enabled?: boolean;
  source?: 'manual' | 'getmodelsapi' | 'custom';
}

const FIXTURE: TestModel[] = [
  {
    platform: 'google',
    modelId: 'gemini-2.5-pro',
    displayName: 'Gemini 2.5 Pro',
    intelligenceScore: 65.4,
    speedTokensPerSec: 85.2,
    sizeLabel: 'Frontier',
    contextWindow: 1048576,
    rankingSource: 'artificial-analysis',
    lastRankedAt: '2025-01-01T00:00:00Z',
  },
  {
    platform: 'cerebras',
    modelId: 'qwen-3-coder-480b',
    displayName: 'Qwen3-Coder 480B',
    intelligenceScore: 62.0,
    speedTokensPerSec: 1800.0,
    sizeLabel: 'Frontier',
    contextWindow: 131072,
    rankingSource: 'artificial-analysis',
    lastRankedAt: '2025-01-01T00:00:00Z',
  },
  {
    platform: 'openrouter',
    modelId: 'meta-llama/llama-3.3-70b-instruct:free',
    displayName: 'Llama 3.3 70B (free)',
    intelligenceScore: 38.5,
    speedTokensPerSec: 60.0,
    sizeLabel: 'Large',
    contextWindow: 131072,
    rankingSource: 'artificial-analysis',
    lastRankedAt: '2025-01-01T00:00:00Z',
  },
  {
    platform: 'mistral',
    modelId: 'mistral-large-latest',
    displayName: 'Mistral Large 3',
    intelligenceScore: 28.0,
    speedTokensPerSec: 45.0,
    sizeLabel: 'Large',
    contextWindow: 131072,
    rankingSource: 'artificial-analysis',
    lastRankedAt: '2025-01-01T00:00:00Z',
  },
  {
    platform: 'unknown-platform',
    modelId: 'not-yet-ranked-model',
    displayName: 'Not Yet Ranked',
    intelligenceScore: null,
    speedTokensPerSec: null,
    sizeLabel: '',
    contextWindow: 8192,
    rankingSource: null,
    lastRankedAt: null,
  },
];

export function seedTestModels(tx: Transaction): void {
  for (const m of FIXTURE) {
    const score = m.intelligenceScore ?? null;
    const tps = m.speedTokensPerSec ?? null;
    const last = m.lastRankedAt ?? null;
    const source = m.rankingSource ?? null;
    // Assign ordinal ranks from the cohort's real scores. Unranked rows
    // get the 99/10 sentinel.
    let intRank: number;
    let spdRank: number;
    if (score != null && tps != null) {
      const scores = FIXTURE.map((f) => f.intelligenceScore ?? -Infinity).filter((s) => s > 0);
      const tpsArr = FIXTURE.map((f) => f.speedTokensPerSec ?? -Infinity).filter((s) => s > 0);
      intRank = [...scores].sort((a, b) => b - a).indexOf(score) + 1;
      spdRank = [...tpsArr].sort((a, b) => b - a).indexOf(tps) + 1;
    } else {
      intRank = 99;
      spdRank = 10;
    }

    tx.insert(schema.models)
      .values({
        platform: m.platform,
        modelId: m.modelId,
        displayName: m.displayName,
        intelligenceRank: m.intelligenceRank ?? intRank,
        speedRank: m.speedRank ?? spdRank,
        intelligenceScore: score,
        speedTokensPerSec: tps,
        rankingSource: source,
        lastRankedAt: last,
        sizeLabel: m.sizeLabel ?? '',
        contextWindow: m.contextWindow ?? null,
        freeTier: m.freeTier ? 1 : 0,
        enabled: m.enabled === false ? 0 : 1,
        source: m.source ?? 'manual',
      })
      .run();
  }
  ensureFallbackEntries(tx);
}
