import fs from 'fs';
import path from 'path';
import { DB_PATH } from '../db/connection.js';
import { snapshotRateLimitState, restoreRateLimitState, type RateLimitSnapshot } from './ratelimit.js';
import { snapshotRouterState, restoreRouterState, type RouterSnapshot } from './router.js';
import { snapshotHealthState, restoreHealthState, type HealthSnapshot } from './health.js';

const STATE_FILE = path.resolve(path.dirname(DB_PATH), 'runtime-state.json');

interface FullSnapshot {
  version: 1;
  savedAt: number;
  rateLimit: RateLimitSnapshot;
  router: RouterSnapshot;
  health: HealthSnapshot;
}

export function saveRuntimeState(): void {
  const snapshot: FullSnapshot = {
    version: 1,
    savedAt: Date.now(),
    rateLimit: snapshotRateLimitState(),
    router: snapshotRouterState(),
    health: snapshotHealthState(),
  };

  const tmp = STATE_FILE + '.tmp';
  fs.writeFileSync(tmp, JSON.stringify(snapshot), 'utf-8');
  fs.renameSync(tmp, STATE_FILE);
  //console.log(`[StatePersistence] Saved runtime state to ${STATE_FILE}`);
}

export function restoreRuntimeState(): boolean {
  if (!fs.existsSync(STATE_FILE)) {
    console.log('[StatePersistence] No saved runtime state found');
    return false;
  }

  try {
    const raw = fs.readFileSync(STATE_FILE, 'utf-8');
    const snapshot: FullSnapshot = JSON.parse(raw);

    if (snapshot.version !== 1) {
      console.warn('[StatePersistence] Unknown state version, skipping restore');
      return false;
    }

    const ageMinutes = (Date.now() - snapshot.savedAt) / 60_000;
    if (ageMinutes > 60) {
      console.log(`[StatePersistence] Saved state is ${ageMinutes.toFixed(0)}min old, discarding stale state`);
      fs.unlinkSync(STATE_FILE);
      return false;
    }

    restoreRateLimitState(snapshot.rateLimit);
    restoreRouterState(snapshot.router);
    restoreHealthState(snapshot.health);

    console.log(`[StatePersistence] Restored runtime state (saved ${ageMinutes.toFixed(1)}min ago)`);
    return true;
  } catch (err) {
    console.error('[StatePersistence] Failed to restore state:', err);
    return false;
  }
}

export function clearRuntimeState(): void {
  if (fs.existsSync(STATE_FILE)) {
    fs.unlinkSync(STATE_FILE);
  }
}

let persistInterval: ReturnType<typeof setInterval> | null = null;

export function startPeriodicSave(intervalMs = 30_000): void {
  if (persistInterval) return;
  persistInterval = setInterval(() => {
    try { saveRuntimeState(); } catch (e) { console.error('[StatePersistence] Periodic save failed:', e); }
  }, intervalMs);
}

export function stopPeriodicSave(): void {
  if (persistInterval) {
    clearInterval(persistInterval);
    persistInterval = null;
  }
}

export function registerShutdownHandlers(): void {
  const shutdown = () => {
    console.log('[StatePersistence] Shutting down, saving state...');
    try { saveRuntimeState(); } catch (e) { console.error('[StatePersistence] Shutdown save failed:', e); }
    stopPeriodicSave();
    process.exit(0);
  };

  process.on('SIGTERM', shutdown);
  process.on('SIGINT', shutdown);
}
