import type { ProviderConfig, ScraperResult } from './index';
import { scrapeHuggingFace } from './huggingface';
import { scrapeGoogle } from './google';
import { scrapeMistral } from './mistral';
import { scrapeOpenRouter } from './openrouter';
import { scrapeKilo } from './kilo';
import { scrapeAIMLAPI } from './aimlapi';
import { scrapeNovita } from './novita';
import { scrapeSambaNova } from './sambanova';

const scraperMap: Record<string, (config: object) => Promise<ScraperResult>> = {
  huggingface: (config) => scrapeHuggingFace(config as ProviderConfig),
  google: (config) => scrapeGoogle(config as ProviderConfig),
  mistral: (config) => scrapeMistral(config as ProviderConfig),
  openrouter: (config) => scrapeOpenRouter(config as ProviderConfig),
  kilo: (config) => scrapeKilo(config as ProviderConfig),
  aimlapi: (config) => scrapeAIMLAPI(config as ProviderConfig),
  novita: (config) => scrapeNovita(config as ProviderConfig),
  sambanova: (config) => scrapeSambaNova(config as ProviderConfig),
};

export function getScraper(providerName: string, config: ProviderConfig): () => Promise<ScraperResult> {
  const fn = scraperMap[providerName];
  if (!fn) {
    throw new Error(`Unknown or unsupported provider: ${providerName}`);
  }
  return () => fn(config);
}
