import { Hono } from 'hono';
import { z } from 'zod';
import { getDb } from '../db/index.js';
import { encrypt, decrypt, maskKey } from '../lib/crypto.js';

// Active providers — must match providers/index.ts registrations + shared/types.ts Platform.
// Hugging Face, Moonshot, and MiniMax direct integrations were dropped in V4
// (see migrateModelsV4 comment block).
const PLATFORMS = [
  'google', 'groq', 'cerebras', 'sambanova', 'nvidia', 'mistral',
  'openrouter', 'github', 'cohere', 'cloudflare', 'zhipu', 'ollama',
  'kilo', 'pollinations', 'llm7',
] as const;

export const keysRouter = new Hono();

const addKeySchema = z.object({
  platform: z.enum(PLATFORMS),
  key: z.string().min(1),
  label: z.string().optional(),
});

// List all keys (masked)
keysRouter.get('/', async (c) => {
  const db = getDb();
  const rows = db.query('SELECT * FROM api_keys ORDER BY created_at DESC, id DESC').all() as any[];

  const keys = rows.map(row => {
    let maskedKey = '****';
    try {
      const realKey = decrypt(row.encrypted_key, row.iv, row.auth_tag);
      maskedKey = maskKey(realKey);
    } catch {
      maskedKey = '[decrypt failed]';
    }
    return {
      id: row.id,
      platform: row.platform,
      label: row.label,
      maskedKey,
      status: row.status,
      enabled: row.enabled === 1,
      createdAt: row.created_at,
      lastCheckedAt: row.last_checked_at,
    };
  });

  return c.json(keys);
});

// Add a key
keysRouter.post('/', async (c) => {
  let body: unknown;
  try {
    body = await c.req.json();
  } catch {
    c.status(400)
    return c.json({ error: { message: 'Malformed JSON body' } });
  }
  const parsed = addKeySchema.safeParse(body);
  if (!parsed.success) {
    c.status(400)
    return c.json({ error: { message: parsed.error.errors.map(e => e.message).join(', ') } });
  }

  const { platform, key, label } = parsed.data;
  const { encrypted, iv, authTag } = encrypt(key);

  const db = getDb();
  const result = db.query(`
    INSERT INTO api_keys (platform, label, encrypted_key, iv, auth_tag, status, enabled)
    VALUES (?, ?, ?, ?, ?, 'unknown', 1)
  `).run([platform, label ?? '', encrypted, iv, authTag]);

  c.status(201)
  return c.json({
    id: result.lastInsertRowid,
    platform,
    label: label ?? '',
    maskedKey: maskKey(key),
    status: 'unknown',
    enabled: true,
  });
});

// Delete a key
keysRouter.delete('/:id', async (c) => {
  const id = parseInt(c.req.param('id'), 10);
  if (isNaN(id)) {
    c.status(400)
    return c.json({ error: { message: 'Invalid key ID' } });
  }

  const db = getDb();
  const result = db.query('DELETE FROM api_keys WHERE id = ?').run(id);

  if (result.changes === 0) {
    c.status(404)
    return c.json({ error: { message: 'Key not found' } });
  }

  return c.json({ success: true });
});

// Toggle enable/disable
keysRouter.patch('/:id', async (c) => {
  const id = parseInt(c.req.param('id'), 10);
  if (isNaN(id)) {
    c.status(400)
    return c.json({ error: { message: 'Invalid key ID' } });
  }

  const { enabled } = await c.req.json();
  if (typeof enabled !== 'boolean') {
    c.status(400)
    return c.json({ error: { message: 'enabled must be a boolean' } });
  }

  const db = getDb();
  const result = db.query('UPDATE api_keys SET enabled = ? WHERE id = ?').run([enabled ? 1 : 0, id]);

  if (result.changes === 0) {
    c.status(400)
    return c.json({ error: { message: 'Key not found' } });
  }

  return c.json({ success: true, enabled });
});
