import type { Platform } from '@freellmapi/shared/types.js';

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

export const CURATION_DEFAULTS = {
  intelligenceRank: 99,
  speedRank: 10,
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
