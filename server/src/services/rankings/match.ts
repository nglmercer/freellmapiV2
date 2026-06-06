// Exact-match model ID resolution across heterogeneous provider namespaces.
//
// Providers in getmodelsapi (and the local DB) use different id formats:
//   openrouter     "google/gemini-2.5-pro", "meta-llama/llama-3.3-70b-instruct:free"
//   google         "gemini-2.5-pro"
//   cerebras       "qwen-3-coder-480b"
//   cloudflare     "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
//   openrouter-v   "deepseek/deepseek-v3.1:free"
//
// External ranking sources (Artificial Analysis, LMArena) use yet another
// format. Rather than substring / regex matching (which is brittle — "gemini
// -2.5-pro" is not "gemini-2.5-pro-preview"), this module generates a fixed
// set of normalised candidate keys and performs strict equality lookups.
//
// No fuzzy matching. If the model isn't in the source, the row stays
// unranked. The user can then either:
//   1. wait for the source to publish a new snapshot, or
//   2. override the rank via the existing PATCH /api/providers/:id/models/:modelId
//      endpoint.

export interface ModelIdentity {
  /** original id from the local DB, e.g. "google/gemini-2.5-pro" */
  readonly raw: string;
  /** lowercased, ready for case-insensitive equality checks */
  readonly lower: string;
  /** id with the publisher prefix stripped, e.g. "gemini-2.5-pro" */
  readonly bare: string;
  /** bare id, lowercased */
  readonly bareLower: string;
  /** bare id with trailing ":free" / "-free" / version tags stripped */
  readonly canonical: string;
  /** canonical, lowercased */
  readonly canonicalLower: string;
}

const PUBLISHER_PREFIX_RE = /^[^/\s]+\//;
const FREE_SUFFIX_RE = /[:-]free$/i;
const DATE_TAG_RE = /-\d{4}-\d{2}-\d{2}$/;
const PREVIEW_TAG_RE = /-(preview|alpha|beta|exp)(\.\d+)?$/i;
const PROVIDER_TAG_RE = /:nitro$|:exacto$|:floor$/i;
const SHA_TAG_RE = /@[a-f0-9]{6,40}$/i;

function stripPublisher(id: string): string {
  return id.replace(PUBLISHER_PREFIX_RE, '');
}

function canonicalize(id: string): string {
  return id
    .replace(FREE_SUFFIX_RE, '')
    .replace(DATE_TAG_RE, '')
    .replace(PREVIEW_TAG_RE, '')
    .replace(SHA_TAG_RE, '')
    .replace(PROVIDER_TAG_RE, '');
}

export function identity(platform: string, modelId: string): ModelIdentity {
  const raw = modelId;
  const lower = modelId.toLowerCase();
  const bare = stripPublisher(modelId);
  const bareLower = bare.toLowerCase();
  const canonical = canonicalize(bare);
  const canonicalLower = canonical.toLowerCase();
  return { raw, lower, bare, bareLower, canonical, canonicalLower };
}

export interface RankLookup<T> {
  /**
   * Look up a ranking for the given local model. Returns the row from the
   * source if and only if the model is present under one of the candidate
   * keys. Must perform strict equality checks only — no normalisation on
   * the source side, no substring matching.
   */
  find(id: ModelIdentity): T | undefined;
}

export function buildRankIndex<T>(
  rows: Iterable<{ key: string; value: T }>,
): RankLookup<T> {
  const index = new Map<string, T>();
  for (const { key, value } of rows) {
    index.set(key.toLowerCase(), value);
  }
  return {
    find(id: ModelIdentity): T | undefined {
      return (
        index.get(id.lower) ??
        index.get(id.bareLower) ??
        index.get(id.canonicalLower) ??
        undefined
      );
    },
  };
}
