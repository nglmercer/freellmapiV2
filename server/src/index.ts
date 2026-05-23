import './env.js';
import { createApp } from './app.js';
import { initDb } from './db/index.js';
import { startHealthChecker } from './services/health.js';
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
}

main().catch(console.error);
