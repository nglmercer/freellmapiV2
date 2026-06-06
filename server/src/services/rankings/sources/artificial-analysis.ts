import type { RankingSource, SourceBenchmark } from '../types.js';
import type { RankLookup } from '../match.js';
import { buildRankIndex } from '../match.js';

const DEFAULT_URL = 'https://artificialanalysis.ai/api/v2/models';
const TIMEOUT_MS = 15_000;

interface AARawModel {
  id?: string;
  name?: string;
  slug?: string;
  model_id?: string;
  intelligence_index?: number;
  intelligence?: number;
  output_tokens_per_second?: number;
  speed_tokens_per_second?: number;
  output_speed?: number;
  speed?: number;
  context_window?: number;
}

interface AAResponse {
  data?: AARawModel[];
  models?: AARawModel[];
}

interface AABenchmark extends SourceBenchmark {
  /** The model id as published by AA, used to build the lookup index. */
  readonly _id: string;
}

function pickId(m: AARawModel): string | null {
  return m.id ?? m.slug ?? m.model_id ?? m.name ?? null;
}

function pickIntelligence(m: AARawModel): number | null {
  const v = m.intelligence_index ?? m.intelligence ?? null;
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

function pickSpeed(m: AARawModel): number | null {
  const v =
    m.output_tokens_per_second ??
    m.speed_tokens_per_second ??
    m.output_speed ??
    m.speed ??
    null;
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

async function fetchAABenchmarks(): Promise<AABenchmark[]> {
  const url = process.env.ARTIFICIAL_ANALYSIS_URL ?? DEFAULT_URL;
  const headers: Record<string, string> = {
    'User-Agent': 'freellmapi/1.0',
    Accept: 'application/json',
  };
  const apiKey = process.env.ARTIFICIAL_ANALYSIS_API_KEY;
  if (apiKey) {
    headers['AA-API-Key'] = apiKey;
    headers['Authorization'] = `Bearer ${apiKey}`;
  }

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const resp = await fetch(url, { headers, signal: controller.signal });
    if (!resp.ok) {
      throw new Error(`AA responded ${resp.status} ${resp.statusText}`);
    }
    const body = (await resp.json()) as AAResponse;
    const list = Array.isArray(body.data) ? body.data
      : Array.isArray(body.models) ? body.models
      : [];

    const out: AABenchmark[] = [];
    for (const m of list) {
      const id = pickId(m);
      const intel = pickIntelligence(m);
      const speed = pickSpeed(m);
      if (!id || intel == null || speed == null) continue;
      out.push({ _id: id, intelligenceScore: intel, speedTokensPerSec: speed });
    }
    return out;
  } finally {
    clearTimeout(timeoutId);
  }
}

export const artificialAnalysisSource: RankingSource = {
  name: 'artificial-analysis',
  async fetchAll(): Promise<SourceBenchmark[]> {
    const benchmarks = await fetchAABenchmarks();
    return benchmarks.map(({ _id, ...rest }) => rest);
  },
};

export async function fetchAALookup(): Promise<RankLookup<SourceBenchmark> | null> {
  try {
    const benchmarks = await fetchAABenchmarks();
    const rows = benchmarks.map((b) => ({ key: b._id, value: b }));
    return buildRankIndex<SourceBenchmark>(rows);
  } catch (err) {
    console.warn(
      '[rankings] artificial-analysis fetch failed:',
      err instanceof Error ? err.message : err,
    );
    return null;
  }
}
