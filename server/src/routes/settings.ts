import { Hono } from 'hono';
import { getUnifiedApiKey, regenerateUnifiedKey, getDb } from '../db/index.js';
import * as schema from '../db/schema.js';

export const settingsRouter = new Hono();

// Get setup status (first-time detection)
settingsRouter.get('/setup-status', (c) => {
  const db = getDb();
  const keys = db.select().from(schema.apiKeys).all();
  const unifiedKey = getUnifiedApiKey();

  return c.json({
    isSetup: keys.length > 0,
    keyCount: keys.length,
    unifiedKey,
  });
});

// Get the unified API key
settingsRouter.get('/api-key', (c) => {
  return c.json({ apiKey: getUnifiedApiKey() });
});

// Regenerate the unified API key
settingsRouter.post('/api-key/regenerate', (c) => {
  const newKey = regenerateUnifiedKey();
  return c.json({ apiKey: newKey });
});
