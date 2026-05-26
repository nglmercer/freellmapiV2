import { getDb } from '../db/index.js';
import * as schema from '../db/schema.js';
import { eq, and } from 'drizzle-orm';
import { OpenAICompatProvider } from './openai-compat.js';
import type { BaseProvider } from './base.js';

const CUSTOM_PREFIX = 'custom:';

let loadedIds = new Set<number>();

export function providerIdToPlatform(id: number): string {
  return `${CUSTOM_PREFIX}${id}`;
}

export function platformToProviderId(platform: string): number | null {
  if (!platform.startsWith(CUSTOM_PREFIX)) return null;
  const id = parseInt(platform.slice(CUSTOM_PREFIX.length), 10);
  return isNaN(id) ? null : id;
}

export function hasCustomProvider(platform: string): boolean {
  return platform.startsWith(CUSTOM_PREFIX);
}

export function loadCustomProvider(platform: string): BaseProvider | undefined {
  const providerId = platformToProviderId(platform);
  if (providerId === null) return undefined;

  const row = getDb().select()
    .from(schema.customProviders)
    .where(and(eq(schema.customProviders.id, providerId), eq(schema.customProviders.enabled, 1)))
    .get();

  if (!row) return undefined;

  let extraHeaders: Record<string, string> = {};
  if (row.extraHeaders) {
    try { extraHeaders = JSON.parse(row.extraHeaders); } catch {}
  }

  const instance = new OpenAICompatProvider({
    platform: platform as any,
    name: row.name,
    baseUrl: row.baseUrl,
    timeoutMs: row.timeoutMs ?? 15000,
    extraHeaders: Object.keys(extraHeaders).length > 0 ? extraHeaders : undefined,
  });

  return instance;
}

export function loadAllCustomProviders(): Map<string, BaseProvider> {
  const map = new Map<string, BaseProvider>();
  const rows = getDb().select()
    .from(schema.customProviders)
    .where(eq(schema.customProviders.enabled, 1))
    .all();

  for (const row of rows) {
    const platform = providerIdToPlatform(row.id);
    let extraHeaders: Record<string, string> = {};
    if (row.extraHeaders) {
      try { extraHeaders = JSON.parse(row.extraHeaders); } catch {}
    }
    map.set(platform, new OpenAICompatProvider({
      platform: platform as any,
      name: row.name,
      baseUrl: row.baseUrl,
      timeoutMs: row.timeoutMs ?? 15000,
      extraHeaders: Object.keys(extraHeaders).length > 0 ? extraHeaders : undefined,
    }));
  }

  loadedIds = new Set(rows.map(r => r.id));
  return map;
}

export function reloadCustomProviders(): Set<number> {
  return loadedIds;
}
