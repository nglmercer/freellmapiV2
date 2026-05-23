import { Hono } from 'hono';
import { getUnifiedApiKey, regenerateUnifiedKey } from '../db/index.js';
import { apiKeyAuth } from '../routes/middleware.js';

export const settingsRouter = new Hono();

// Get the unified API key
settingsRouter.get('/api-key', apiKeyAuth, (c) => {
  return c.json({ apiKey: getUnifiedApiKey() });
});

// Regenerate the unified API key
settingsRouter.post('/api-key/regenerate', apiKeyAuth, (c) => {
  const newKey = regenerateUnifiedKey();
  return c.json({ apiKey: newKey });
});