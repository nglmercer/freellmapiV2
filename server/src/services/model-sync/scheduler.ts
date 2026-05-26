import { syncModels } from './sync.js';

const SYNC_INTERVAL_MS = 6 * 60 * 60 * 1000;

let intervalId: ReturnType<typeof setInterval> | null = null;

export async function runInitialSync(): Promise<void> {
  console.log('[ModelSync] Running initial sync...');
  try {
    const result = await syncModels();
    console.log(
      `[ModelSync] Initial sync done: ${result.added} added (active), ${result.storedDisabled} stored (disabled), ` +
      `${result.updated} updated, ${result.freeToPaid} free→paid, ${result.paidToFree} paid→free`
    );
  } catch (err) {
    console.error('[ModelSync] Initial sync failed:', err);
  }
}

export function startSyncScheduler(): void {
  if (intervalId) return;
  console.log(`[ModelSync] Scheduler started (every ${SYNC_INTERVAL_MS / 3600000}h)`);

  intervalId = setInterval(async () => {
    console.log('[ModelSync] Running scheduled sync...');
    try {
      const result = await syncModels();
      console.log(
        `[ModelSync] Sync done: ${result.added} added, ${result.updated} updated, ${result.disabled} disabled`
      );
    } catch (err) {
      console.error('[ModelSync] Scheduled sync failed:', err);
    }
  }, SYNC_INTERVAL_MS);
}

export function stopSyncScheduler(): void {
  if (intervalId) {
    clearInterval(intervalId);
    intervalId = null;
  }
}
