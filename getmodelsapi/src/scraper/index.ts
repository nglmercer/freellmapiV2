import type { ProviderConfig } from '../types';
import { getScraper } from './providers/factory';
import type { ScraperResult } from './providers/index';

export async function scrapeModels(provider: ProviderConfig): Promise<ScraperResult[]> {
  if (!provider.supportsScraping) {
    return [];
  }

  const scraper = getScraper(provider.name, provider);
  const results = await scraper();

  return [results];
}

export async function scrapeAllProviders(providers: ProviderConfig[]): Promise<ScraperResult[]> {
  const results = await Promise.all(
    providers
      .filter(p => p.supportsScraping)
      .map(async (provider) => {
        return scrapeModels(provider);
      })
  );

  return results.flat();
}
