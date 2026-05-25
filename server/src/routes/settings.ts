import { Hono } from 'hono';
import { getUnifiedApiKey, regenerateUnifiedKey } from '../db/index.js';

export const settingsRouter = new Hono();

// Get the unified API key
settingsRouter.get('/api-key', (c) => {
  return c.json({ apiKey: getUnifiedApiKey() });
});

// Regenerate the unified API key
settingsRouter.post('/api-key/regenerate', (c) => {
  const newKey = regenerateUnifiedKey();
  return c.json({ apiKey: newKey });
});
