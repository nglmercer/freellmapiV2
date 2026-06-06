// Common types for the rankings enrichment service.
//
// All sources must return real benchmark data fetched from their API — never
// inferred from model name substrings, version tags, or pricing tiers.

export interface SourceBenchmark {
  /** raw score as reported by the source (e.g. AA intelligence_index) */
  readonly intelligenceScore: number;
  /** tokens/sec output speed as reported by the source */
  readonly speedTokensPerSec: number;
}

export interface RankingSource {
  /** short identifier, stored in models.ranking_source */
  readonly name: string;
  /** fetch the latest snapshot of all benchmarks from this source */
  fetchAll(): Promise<SourceBenchmark[]>;
}
