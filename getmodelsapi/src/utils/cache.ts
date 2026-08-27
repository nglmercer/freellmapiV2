import { existsSync, mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const CACHE_DIR = join(process.cwd(), '.cache');
const ONE_HOUR = 36e5;
const FIVE_MINUTES = 3e5;

interface CacheEntry<T> {
  data: T;
  timestamp: number;
  ttl: number;
}

function getCachePath(key: string): string {
  const safeKey = key.replace(/[^a-z0-9-_.]/gi, '_').substring(0, 200);
  mkdirSync(CACHE_DIR, { recursive: true });
  return join(CACHE_DIR, `${safeKey}.json`);
}

export function cacheGet<T>(key: string): T | null {
  try {
    const path = getCachePath(key);
    if (!existsSync(path)) return null;
    const raw = readFileSync(path, 'utf-8');
    const entry: CacheEntry<T> = JSON.parse(raw);
    if (Date.now() - entry.timestamp > entry.ttl) {
      return null; // expired
    }
    return entry.data;
  } catch {
    return null;
  }
}

export function cacheSet<T>(key: string, data: T, ttlMs = ONE_HOUR): void {
  try {
    const entry: CacheEntry<T> = { data, timestamp: Date.now(), ttl: ttlMs };
    writeFileSync(getCachePath(key), JSON.stringify(entry));
  } catch {
    // Fail silently — cache is optional
  }
}

export function clearCache(): number {
  let count = 0;
  try {
    const files = readdirSync(CACHE_DIR);
    for (const file of files) {
      if (file.endsWith('.json')) {
        unlinkSync(join(CACHE_DIR, file));
        count++;
      }
    }
  } catch {
    // directory may not exist
  }
  return count;
}

export { CACHE_DIR, ONE_HOUR, FIVE_MINUTES };
