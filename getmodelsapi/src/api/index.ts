import { PROVIDERS } from '../config/providers';
import { fetchByProvider } from '../api/providers';
import { scrapeAllProviders } from '../scraper';
import { cacheGet as diskCacheGet, cacheSet as diskCacheSet, ONE_HOUR, clearCache } from '../utils/cache';
import type { Model, ProviderConfig, SearchParams } from '../types';

const FIVE_MINUTES = 300000;

// Two-level cache: in-memory (fast) + disk (persistent)
const memCache = new Map<string, { data: Model[]; timestamp: number }>();

function getMemCached(key: string): Model[] | null {
  const entry = memCache.get(key);
  if (!entry) return null;
  if (Date.now() - entry.timestamp > FIVE_MINUTES) {
    memCache.delete(key);
    return null;
  }
  return entry.data;
}

function setMemCache(key: string, data: Model[]): void {
  memCache.set(key, { data, timestamp: Date.now() });
}

function getCacheKey(provider: string, params: Partial<SearchParams>): string {
  return `models:${provider}:${JSON.stringify(params)}`;
}

function sortProvidersByFreeTier(providers: ProviderConfig[]): ProviderConfig[] {
  return [...providers].sort((a, b) => {
    const aScore = (a.freeTier && a.apiKey) ? 0 : a.freeTier ? 1 : 2;
    const bScore = (b.freeTier && b.apiKey) ? 0 : b.freeTier ? 1 : 2;
    if (aScore !== bScore) return aScore - bScore;
    return a.priority - b.priority;
  });
}

async function fetchModels(provider: string, params: Partial<SearchParams>): Promise<Model[]> {
  const key = getCacheKey(provider, params);

  // 1. Try memory cache
  const memResult = getMemCached(key);
  if (memResult) return memResult;

  // 2. Try disk cache
  const diskResult = diskCacheGet<Model[]>(key);
  if (diskResult) {
    setMemCache(key, diskResult);
    return diskResult;
  }

  // 3. Fetch from provider
  let models: Model[] = [];

  const providerConfig = PROVIDERS.find(p => p.name === provider);
  if (providerConfig && fetchByProvider[provider]) {
    try {
      models = await fetchByProvider[provider](providerConfig);
    } catch {
      // fall through to scraper
    }
  }

  if (models.length === 0 && providerConfig?.supportsScraping) {
    try {
      const scraperResults = await scrapeAllProviders([providerConfig]);
      for (const result of scraperResults) {
        if (result.success && Array.isArray(result.models)) {
          models = models.concat(result.models);
        }
      }
    } catch {
      // provider may be down
    }
  }

  // Apply filters
  if (params.search) {
    const q = params.search.toLowerCase();
    models = models.filter(m =>
      m.name.toLowerCase().includes(q) ||
      (m.description?.toLowerCase().includes(q)) ||
      m.id.toLowerCase().includes(q)
    );
  }

  if (params.limit) models = models.slice(0, params.limit);
  if (params.offset) models = models.slice(params.offset);

  // Cache: memory + disk
  setMemCache(key, models);
  diskCacheSet(key, models, ONE_HOUR);

  return models;
}

export async function getModels(options: {
  provider?: string;
  gateway?: string;
  search?: string;
  free?: boolean;
  exclude?: string | string[];
  limit?: number;
  offset?: number;
} = {}): Promise<Model[]> {
  const { provider, gateway, search, free, exclude, limit = 100, offset = 0 } = options;

  const excludeList: string[] = exclude
    ? (typeof exclude === 'string' ? [exclude] : exclude)
    : [];

  if (provider) {
    if (excludeList.includes(provider)) return [];
    return fetchModels(provider, { search, limit, offset });
  }

  if (gateway) {
    if (excludeList.includes(gateway)) return [];
    const gw = PROVIDERS.find(p => p.name === gateway && p.type === 'gateway');
    if (!gw) return [];
    return fetchModels(gw.name, { search, limit, offset });
  }

  const allModels = new Map<string, Model>();
  const sorted = sortProvidersByFreeTier(PROVIDERS);

  for (const p of sorted) {
    if (excludeList.includes(p.name)) continue;
    try {
      const models = await fetchModels(p.name, { search, limit, offset });
      for (const m of models) allModels.set(m.id, m);
    } catch {
      // skip failed providers
    }
  }

  let models = Array.from(allModels.values());

  if (search) {
    const q = search.toLowerCase();
    models = models.filter(m =>
      m.name.toLowerCase().includes(q) ||
      (m.description?.toLowerCase().includes(q)) ||
      m.id.toLowerCase().includes(q)
    );
  }

  if (free) {
    models = models.filter(m => m.freeTier === true);
  }

  if (limit) models = models.slice(0, limit);
  if (offset) models = models.slice(offset);

  return models;
}

export async function getProviders(): Promise<Omit<ProviderConfig, 'apiKey'>[]> {
  return PROVIDERS.map(p => ({
    name: p.name,
    type: p.type,
    baseUrl: p.baseUrl,
    supportsScraping: p.supportsScraping,
    priority: p.priority,
    freeTier: p.freeTier,
  }));
}

export { clearCache };
export { enrichModel, enrichModels } from '../utils/enrich';
export { PROVIDERS } from '../config/providers';
export type { Model, ProviderConfig, SearchParams, ApiResponse } from '../types';
