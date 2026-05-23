import { Hono } from 'hono';
import { z } from 'zod';
import { getDb } from '../db/index.js';
import { encrypt, decrypt, maskKey } from '../lib/crypto.js';
import * as schema from '../db/schema.js';
import { eq, desc } from 'drizzle-orm';

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
  const rows = db.select().from(schema.apiKeys).orderBy(desc(schema.apiKeys.createdAt), desc(schema.apiKeys.id)).all();

  const keys = rows.map(row => {
    let maskedKey = '****';
    try {
      const realKey = decrypt(row.encryptedKey, row.iv, row.authTag);
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
      createdAt: row.createdAt,
      lastCheckedAt: row.lastCheckedAt,
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
  const result = db.insert(schema.apiKeys).values({
    platform,
    label: label ?? '',
    encryptedKey: encrypted,
    iv,
    authTag,
    status: 'unknown',
    enabled: 1
  }).run();

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
  const result = db.delete(schema.apiKeys).where(eq(schema.apiKeys.id, id)).run();

  if (result.changes === 0) {
    c.status(404)
    return c.json({ error: { message: 'Key not found' } });
  }

  return c.json({ success: true });
});

// Toggle enable/disable or re-encrypt key value
keysRouter.patch('/:id', async (c) => {
  const id = parseInt(c.req.param('id'), 10);
  if (isNaN(id)) {
    c.status(400)
    return c.json({ error: { message: 'Invalid key ID' } });
  }

  let body: any;
  try {
    body = await c.req.json();
  } catch {
    c.status(400)
    return c.json({ error: { message: 'Malformed JSON body' } });
  }

  const db = getDb();

  // Re-encrypt: update the key value (for decryption-failed keys)
  if (typeof body.key === 'string' && body.key.length > 0) {
    const keyToEncrypt = body.key;
    const { encrypted, iv, authTag } = encrypt(keyToEncrypt);
    const result = db.update(schema.apiKeys)
      .set({ encryptedKey: encrypted, iv, authTag, status: 'unknown' })
      .where(eq(schema.apiKeys.id, id))
      .run();

    if (result.changes === 0) {
      c.status(400)
      return c.json({ error: { message: 'Key not found' } });
    }

    return c.json({ success: true, reEncrypted: true, maskedKey: maskKey(keyToEncrypt) });
  }

  // Toggle enabled
  if (typeof body.enabled === 'boolean') {
    const result = db.update(schema.apiKeys)
      .set({ enabled: body.enabled ? 1 : 0 })
      .where(eq(schema.apiKeys.id, id))
      .run();

    if (result.changes === 0) {
      c.status(400)
      return c.json({ error: { message: 'Key not found' } });
    }

    return c.json({ success: true, enabled: body.enabled });
  }

  c.status(400)
  return c.json({ error: { message: 'Provide either "key" (string) or "enabled" (boolean)' } });
});
