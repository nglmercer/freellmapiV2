import './env.js';
import { createApp } from './app.js';
import { initDb } from './db/index.js';
import { startHealthChecker } from './services/health.js';
import { serve } from '@hono/node-server';

const PORT = process.env.PORT ?? 3001;

async function main() {
  initDb();
  const app = createApp();

  serve({
    fetch: app.fetch,
    port: Number(PORT),
  });

  console.log(`Server running on http://0.0.0.0:${PORT}`);
  console.log(`Proxy endpoint: http://0.0.0.0:${PORT}/v1/chat/completions`);
  startHealthChecker();
}

main().catch(console.error);
