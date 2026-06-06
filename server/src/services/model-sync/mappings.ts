import type { Platform } from '@freellmapi/shared/types.js';
import { UNRANKED_INTELLIGENCE, UNRANKED_SPEED } from '../../db/seed.js';

export const PROVIDER_TO_PLATFORM: Record<string, Platform | null> = {
  google: 'google',
  mistral: 'mistral',
  openrouter: 'openrouter',
  groq: 'groq',
  cohere: 'cohere',
  sambanova: 'sambanova',
  kilo: 'kilo',
  together: null,
  aimlapi: null,
  novita: null,
  huggingface: null,
};

// Sentinels used when no real benchmark has been applied. The enrichment
// service (server/src/services/rankings/enrich.ts) overwrites these with
// real scores fetched from external sources. There is intentionally no
// name-based fallback here — see enrich.ts for the rationale.
export const CURATION_DEFAULTS = {
  intelligenceRank: UNRANKED_INTELLIGENCE,
  speedRank: UNRANKED_SPEED,
  sizeLabel: '',
  rpmLimit: null as number | null,
  rpdLimit: null as number | null,
  tpmLimit: null as number | null,
  tpdLimit: null as number | null,
  monthlyTokenBudget: '',
  enabled: 1,
};

export function getPlatformByProvider(rawProvider: string): Platform | null {
  return PROVIDER_TO_PLATFORM[rawProvider] ?? null;
}
