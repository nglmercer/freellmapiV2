import { describe, test, expect } from 'bun:test';
import { PROVIDERS } from '../src/config/providers';
import { getScraper } from '../src/scraper/providers/factory';
import { getModels, getProviders } from '../src/api/index';
import type { ProviderConfig, ScraperResult } from '../src/scraper/providers/index';

// ── Providers that work WITHOUT any API key ──
// google: scrapes HTML from docs page
// mistral: scrapes HTML from docs page
// huggingface: public API, no auth needed
// openrouter: public API, no auth needed
// kilo: public API, no auth needed
// aimlapi: public API, no auth needed
// novita: public API, no auth needed (has free models)
// sambanova: public API, no auth needed

const PUBLIC_PROVIDERS = ['google', 'mistral', 'huggingface', 'openrouter', 'kilo', 'aimlapi', 'novita', 'sambanova'] as const;

function makeConfig(name: string): ProviderConfig {
  const base = PROVIDERS.find(p => p.name === name);
  if (!base) throw new Error(`Unknown provider: ${name}`);
  return { ...base, apiKey: undefined };
}

// ── Provider configuration ──

describe('Provider configuration', () => {
  test('all providers have required fields', () => {
    for (const p of PROVIDERS) {
      expect(typeof p.name).toBe('string');
      expect(p.name.length).toBeGreaterThan(0);
      expect(['provider', 'gateway']).toContain(p.type);
      expect(typeof p.supportsScraping).toBe('boolean');
      expect(typeof p.freeTier).toBe('boolean');
      expect(typeof p.priority).toBe('number');
    }
  });

  test('public providers exist and support scraping', () => {
    for (const name of PUBLIC_PROVIDERS) {
      const p = PROVIDERS.find(pr => pr.name === name);
      expect(p).toBeDefined();
      expect(p!.supportsScraping).toBe(true);
    }
  });

  test('no scraper registered for auth-only providers', () => {
    // together, cohere, groq: no public API, no scraper
    const authOnly = ['together', 'cohere', 'groq'];
    for (const name of authOnly) {
      const config = PROVIDERS.find(p => p.name === name)!;
      expect(() => getScraper(name, config)).toThrow();
    }
  });

  test('free-tier providers list is correct', () => {
    const free = PROVIDERS.filter(p => p.freeTier).map(p => p.name);
    expect(free).toContain('google');
    expect(free).toContain('mistral');
    expect(free).toContain('together');
    expect(free).toContain('cohere');
  });
});

// ── getProviders() ──

describe('getProviders()', () => {
  test('returns all providers without apiKey exposed', async () => {
    const providers = await getProviders();
    expect(providers.length).toBe(PROVIDERS.length);
    for (const p of providers) {
      expect(p).not.toHaveProperty('apiKey');
      expect(typeof p.freeTier).toBe('boolean');
    }
  });

  test('sorted by priority', async () => {
    const providers = await getProviders();
    for (let i = 1; i < providers.length; i++) {
      expect(providers[i]!.priority).toBeGreaterThanOrEqual(providers[i - 1]!.priority);
    }
  });
});

// ── Scraper execution: public providers must return models ──

describe('Scraping without API key', () => {
  for (const name of PUBLIC_PROVIDERS) {
    test(`${name} returns models via scraping`, async () => {
      const config = makeConfig(name);
      const fn = getScraper(name, config);
      const result: ScraperResult = await fn();

      expect(result.success).toBe(true);
      expect(Array.isArray(result.models)).toBe(true);
      expect(result.models.length).toBeGreaterThan(0);

      for (const model of result.models) {
        expect(typeof model.id).toBe('string');
        expect(model.id.length).toBeGreaterThan(0);
        expect(typeof model.name).toBe('string');
        expect(typeof model.provider).toBe('string');
        expect(model.provider.length).toBeGreaterThan(0);
        expect(typeof model.contextWindow).toBe('number');
        expect(model.contextWindow).toBeGreaterThan(0);
        expect(Array.isArray(model.supportedFeatures)).toBe(true);
      }
    });
  }
});

// ── getModels() library ──

describe('getModels()', () => {
  test('returns models from public providers without API keys', async () => {
    const models = await getModels();
    expect(Array.isArray(models)).toBe(true);
    expect(models.length).toBeGreaterThan(0);
    for (const m of models) {
      expect(typeof m.id).toBe('string');
    }
  });

  test('getModels({ free: true }) returns only free-tier', async () => {
    const models = await getModels({ free: true });
    for (const m of models) {
      expect(m.freeTier).toBe(true);
    }
  });

  test('getModels({ provider: "google" }) returns google models', async () => {
    const models = await getModels({ provider: 'google' });
    expect(models.length).toBeGreaterThan(0);
    for (const m of models) {
      expect(m.provider).toBe('google');
    }
  });

  test('getModels({ provider: "mistral" }) returns mistral models', async () => {
    const models = await getModels({ provider: 'mistral' });
    expect(models.length).toBeGreaterThan(0);
    for (const m of models) {
      expect(m.provider).toBe('mistral');
    }
  });

  test('getModels({ provider: "huggingface" }) returns huggingface models', async () => {
    const models = await getModels({ provider: 'huggingface' });
    expect(models.length).toBeGreaterThan(0);
    for (const m of models) {
      expect(m.provider).toBe('huggingface');
    }
  });

  test('getModels({ search: "..." }) filters results', async () => {
    const models = await getModels({ provider: 'huggingface' });
    if (models.length > 0) {
      const term = models[0]!.name.split('/').pop()!.split('-')[0]!;
      const searched = await getModels({ provider: 'huggingface', search: term });
      expect(searched.length).toBeGreaterThan(0);
    }
  });

  test('getModels({ limit: 5 }) caps results', async () => {
    const models = await getModels({ provider: 'huggingface', limit: 5 });
    expect(models.length).toBeLessThanOrEqual(5);
  });
});

// ── Deduplication ──

describe('Deduplication', () => {
  test('no duplicate model IDs', async () => {
    const models = await getModels();
    const ids = models.map(m => m.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });
});

// ── Cache ──

describe('Cache', () => {
  test('cached call returns near-instantly', async () => {
    const opts = { provider: 'huggingface', limit: 10, offset: 99 };

    // First call populates cache
    await getModels(opts);

    // Second call should be near-instant from cache
    const t1 = performance.now();
    await getModels(opts);
    const secondDuration = performance.now() - t1;

    expect(secondDuration).toBeLessThan(10); // under 10ms = cache hit
  });
});
