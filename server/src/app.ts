import { Hono } from 'hono';
import { cors } from 'hono/cors';
import { bodyLimit } from 'hono/body-limit';
import { serveStatic } from '@hono/node-server/serve-static';
import { fileURLToPath } from 'url';
import path from 'path';
import { keysRouter } from './routes/keys.js';
import { modelsRouter } from './routes/models.js';
import { proxyRouter } from './routes/proxy.js';
import { fallbackRouter } from './routes/fallback.js';
import { analyticsRouter } from './routes/analytics.js';
import { healthRouter } from './routes/health.js';
import { settingsRouter } from './routes/settings.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DIST_DIR = path.resolve(__dirname, '../../client/dist');

export function createApp() {
  const app = new Hono();

  // Middleware - Allow all origins by default (as requested)
  app.use('*', cors());
  app.use(bodyLimit({
    maxSize: 1024 * 1024,
    onError: (c) => {
      return c.text('Payload too large', 413);
    }
  }));

  // API routes
  app.route('/api/keys', keysRouter);
  app.route('/api/models', modelsRouter);
  app.route('/api/fallback', fallbackRouter);
  app.route('/api/analytics', analyticsRouter);
  app.route('/api/health', healthRouter);
  app.route('/api/settings', settingsRouter);

  // OpenAI-compatible proxy
  app.route('/v1', proxyRouter);

  // Health check
  app.get('/api/ping', (c) => {
    return c.json({ status: 'ok', timestamp: new Date().toISOString() });
  });

  // Error handler (for API routes)
  app.onError((err, c) => {
    console.error(`${err}`)
    return c.text(`${err}`, 500)
  })

  // Serve client static files — SPA fallback included
  app.use('*', serveStatic({
    root: DIST_DIR,
    rewriteRequestPath: (p) => p.startsWith('/') && !path.extname(p) ? '/index.html' : p,
  }));

  return app;
}
