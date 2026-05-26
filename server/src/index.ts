import './env.js';
import { createApp } from './app.js';
import { initDb } from './db/index.js';
import { startHealthChecker } from './services/health.js';
import { runInitialSync, startSyncScheduler } from './services/model-sync/scheduler.js';
import { serve } from '@hono/node-server';
import { env } from './env.js';

async function main() {
  await initDb();
  const app = createApp();

  serve({
    fetch: app.fetch,
    port: env.getPort(),
  });

  console.log(`Server running on http://0.0.0.0:${env.getPort()}`);
  console.log(`Proxy endpoint: http://0.0.0.0:${env.getPort()}/v1/chat/completions`);
  startHealthChecker();

  // start model sync in background without crashing server on failure
  setImmediate(async () => {
    try { await runInitialSync(); } catch (e) { console.error('[ModelSync] Startup sync crashed:', e); }
    try { startSyncScheduler(); } catch (e) { console.error('[ModelSync] Scheduler crashed:', e); }
  });
}

main().catch(console.error);
