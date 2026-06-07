import { describe, it, expect, beforeAll } from 'bun:test';
import { createApp } from '../src/app.js';
import { getDb, getUnifiedApiKey } from '../src/db/index.js';
import * as schema from '../src/db/schema.js';
import { eq, sql } from 'drizzle-orm';

const COMMIT_PROMPT = `Generate a concise git commit message for the following diff:

diff --git a/src/auth.ts b/src/auth.ts
index 1234567..abcdefg 100644
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -10,6 +10,15 @@ import { createHash } from 'crypto';
+function validateToken(token: string): boolean {
+  const parts = token.split('.');
+  if (parts.length !== 3) return false;
+  try {
+    const payload = JSON.parse(atob(parts[1]!));
+    return payload.exp > Date.now() / 1000;
+  } catch { return false; }
+}

The commit message should follow conventional commits format.`;

const STREAM_COMMIT_PROMPT = `Write a short commit message for this change:

diff --git a/server/src/routes/proxy.ts b/server/src/routes/proxy.ts
--- a/server/src/routes/proxy.ts
+++ b/server/src/routes/proxy.ts
@@ -50,3 +50,8 @@
+// Added retry logic for transient failures
+const MAX_RETRIES = 3;
+for (let i = 0; i < MAX_RETRIES; i++) {
+  try { return await fetch(url); } catch (e) { if (i === MAX_RETRIES - 1) throw e; }
+}`;

function hasApiKeysInDb(): boolean {
  try {
    const db = getDb();
    const result = db.select({ count: sql<number>`count(*)` })
      .from(schema.apiKeys)
      .where(sql`${schema.apiKeys.enabled} = 1`)
      .get();
    return (result?.count ?? 0) > 0;
  } catch {
    return false;
  }
}

function getWorkingPlatforms(): Set<string> {
  try {
    const db = getDb();
    const keys = db.select({ platform: schema.apiKeys.platform })
      .from(schema.apiKeys)
      .where(sql`${schema.apiKeys.enabled} = 1 AND ${schema.apiKeys.status} != 'invalid'`)
      .all();
    return new Set(keys.map(k => k.platform));
  } catch {
    return new Set();
  }
}

function getFallbackPlatforms(): string[] {
  try {
    const db = getDb();
    const chain = db.select({ platform: schema.models.platform })
      .from(schema.fallbackConfig)
      .innerJoin(schema.models, eq(schema.fallbackConfig.modelDbId, schema.models.id))
      .where(eq(schema.fallbackConfig.enabled, 1))
      .orderBy(schema.fallbackConfig.priority)
      .all();
    return [...new Set(chain.map(c => c.platform))];
  } catch {
    return [];
  }
}

function hasFreeTierModels(): boolean {
  try {
    const db = getDb();
    const result = db.select({ count: sql<number>`count(*)` })
      .from(schema.models)
      .where(sql`${schema.models.freeTier} = 1 AND ${schema.models.enabled} = 1`)
      .get();
    return (result?.count ?? 0) > 0;
  } catch {
    return false;
  }
}

function getFreeTierPlatforms(): string[] {
  try {
    const db = getDb();
    const models = db.select({ platform: schema.models.platform })
      .from(schema.models)
      .where(sql`${schema.models.freeTier} = 1 AND ${schema.models.enabled} = 1`)
      .all();
    return [...new Set(models.map(m => m.platform))];
  } catch {
    return [];
  }
}

function hasWorkingProvider(): boolean {
  const keys = getWorkingPlatforms();
  try {
    const db = getDb();
    const freePlatforms = db.selectDistinct({ platform: schema.models.platform })
      .from(schema.fallbackConfig)
      .innerJoin(schema.models, eq(schema.fallbackConfig.modelDbId, schema.models.id))
      .where(sql`${schema.fallbackConfig.enabled} = 1 AND ${schema.models.freeTier} = 1 AND ${schema.models.enabled} = 1`)
      .all();
    return freePlatforms.some(m => keys.has(m.platform));
  } catch {
    return false;
  }
}

function getFreeTierModel(): string | null {
  const keys = getWorkingPlatforms();
  try {
    const db = getDb();
    const models = db.select({ modelId: schema.models.modelId, platform: schema.models.platform })
      .from(schema.fallbackConfig)
      .innerJoin(schema.models, eq(schema.fallbackConfig.modelDbId, schema.models.id))
      .where(sql`${schema.fallbackConfig.enabled} = 1 AND ${schema.models.freeTier} = 1 AND ${schema.models.enabled} = 1`)
      .orderBy(schema.fallbackConfig.priority)
      .all();

    for (const m of models) {
      if (keys.has(m.platform)) {
        return m.modelId;
      }
    }
    return null;
  } catch {
    return null;
  }
}

describe.skipIf(!hasApiKeysInDb())('E2E: AI Editor Commit Message Generation', () => {
  let app: ReturnType<typeof createApp>;
  let apiKey: string;

  beforeAll(() => {
    app = createApp();
    apiKey = getUnifiedApiKey();
  });

  const authHeaders = () => ({
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${apiKey}`,
  });

  it('diagnostic: show API keys, fallback chain, and missing platforms', async () => {
    const db = getDb();
    const keyPlatforms = getWorkingPlatforms();
    const fallbackPlatforms = getFallbackPlatforms();

    const keys = db.select({
      platform: schema.apiKeys.platform,
      status: schema.apiKeys.status,
      enabled: schema.apiKeys.enabled,
    }).from(schema.apiKeys).all();

    console.log('\n[E2E DIAG] API Keys in database:');
    const keysByPlatform = keys.reduce((acc, k) => {
      acc[k.platform] = (acc[k.platform] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);
    for (const [platform, count] of Object.entries(keysByPlatform)) {
      console.log(`  - ${platform}: ${count} key(s) [${keyPlatforms.has(platform) ? 'WORKING' : 'INVALID'}]`);
    }

    const fallback = db.select({
      priority: schema.fallbackConfig.priority,
      enabled: schema.fallbackConfig.enabled,
      modelId: schema.models.modelId,
      platform: schema.models.platform,
    }).from(schema.fallbackConfig)
      .innerJoin(schema.models, eq(schema.fallbackConfig.modelDbId, schema.models.id))
      .orderBy(schema.fallbackConfig.priority)
      .limit(10)
      .all();

    console.log('\n[E2E DIAG] Fallback chain (first 10):');
    for (const f of fallback) {
      const hasKey = keyPlatforms.has(f.platform);
      console.log(`  ${f.priority}. ${f.platform}/${f.modelId} ${hasKey ? '[HAS KEY]' : '[NO KEY]'}`);
    }

    const missingPlatforms = fallbackPlatforms.filter(p => !keyPlatforms.has(p));
    if (missingPlatforms.length > 0) {
      console.log(`\n[E2E DIAG] Missing API keys for platforms: ${missingPlatforms.join(', ')}`);
      console.log(`[E2E DIAG] Add keys for these platforms in the dashboard to enable the fallback chain.`);
    }

    if (!hasWorkingProvider()) {
      console.log(`\n[E2E DIAG] No working provider found. Connection tests will be skipped.`);
      console.log(`[E2E DIAG] To fix: add a valid API key for one of: ${fallbackPlatforms.slice(0, 5).join(', ')}`);
    }

    expect(keys.length).toBeGreaterThan(0);
  }, 10000);

  it('should list models in OpenAI format (for AI editor model discovery)', async () => {
    const res = await app.request('/v1/models');
    expect(res.status).toBe(200);
    const data = await res.json() as any;

    expect(data.object).toBe('list');
    expect(Array.isArray(data.data)).toBe(true);
    expect(data.data.length).toBeGreaterThan(0);

    const autoModel = data.data.find((m: any) => m.id === 'auto');
    expect(autoModel).toBeDefined();

    for (const model of data.data) {
      expect(model).toHaveProperty('id');
      expect(model).toHaveProperty('object', 'model');
      expect(model).toHaveProperty('owned_by');
      expect(typeof model.id).toBe('string');
      expect(() => encodeURIComponent(model.id)).not.toThrow();
    }

    console.log(`[E2E] Listed ${data.data.length} models for AI editor discovery`);
  });

  describe.skipIf(!hasWorkingProvider())('Connection tests (requires working provider)', () => {
    let testModel: string;

    beforeAll(() => {
      const model = getFreeTierModel();
      if (!model) {
        throw new Error('No free tier model found with valid key');
      }
      testModel = model;
      console.log(`[E2E] Using free tier model: ${testModel}`);
    });

    it('should handle non-streaming commit message request (like Cursor/Continue)', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: testModel,
          messages: [
            { role: 'system', content: 'You are a helpful assistant that generates git commit messages.' },
            { role: 'user', content: COMMIT_PROMPT },
          ],
          max_tokens: 200,
          temperature: 0.7,
        }),
      });

      const data = await res.json() as any;
      if (res.status !== 200) {
        console.error(`[E2E DEBUG] Status: ${res.status}`);
        console.error(`[E2E DEBUG] Error: ${JSON.stringify(data.error)}`);
        console.error(`[E2E DEBUG] Headers: routed=${res.headers.get('X-Routed-Via')}, fallback=${res.headers.get('X-Fallback-Attempts')}`);
      }
      expect(res.status).toBe(200);

      expect(data).toHaveProperty('id');
      expect(data).toHaveProperty('choices');
      expect(data.choices.length).toBeGreaterThan(0);
      expect(data.choices[0]).toHaveProperty('message');
      expect(data.choices[0].message).toHaveProperty('content');
      expect(typeof data.choices[0].message.content).toBe('string');
      expect(data.choices[0].message.content.length).toBeGreaterThan(0);

      const routedVia = res.headers.get('X-Routed-Via');
      console.log(`[E2E] Non-stream routed via: ${routedVia}`);
      console.log(`[E2E] Response preview: ${data.choices[0].message.content.slice(0, 100)}...`);
    }, 120000);

    it('should handle streaming commit message request (like Cursor/Continue)', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: testModel,
          stream: true,
          messages: [
            { role: 'system', content: 'You generate concise git commit messages.' },
            { role: 'user', content: STREAM_COMMIT_PROMPT },
          ],
          max_tokens: 150,
          temperature: 0.5,
          stream_options: { include_usage: true },
        }),
      });

      if (res.status !== 200) {
        const errData = await res.json().catch(() => ({})) as any;
        console.error(`[E2E DEBUG] Stream Status: ${res.status}`);
        console.error(`[E2E DEBUG] Stream Error: ${JSON.stringify(errData.error)}`);
      }
      expect(res.status).toBe(200);
      expect(res.headers.get('content-type')).toBe('text/event-stream');

      const reader = res.body!.getReader();
      const decoder = new TextDecoder();
      let fullContent = '';
      let chunkCount = 0;
      let sawDone = false;
      let lastChunk: any = null;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const text = decoder.decode(value, { stream: true });
        const lines = text.split('\n');

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed.startsWith('data: ')) continue;
          const payload = trimmed.slice(6);
          if (payload === '[DONE]') {
            sawDone = true;
            break;
          }
          try {
            const chunk = JSON.parse(payload);
            lastChunk = chunk;
            chunkCount++;
            const delta = chunk.choices?.[0]?.delta?.content ?? '';
            fullContent += delta;
          } catch {
          }
        }
        if (sawDone) break;
      }

      expect(sawDone).toBe(true);
      expect(chunkCount).toBeGreaterThan(0);
      expect(fullContent.length).toBeGreaterThan(0);

      const routedVia = res.headers.get('X-Routed-Via');
      console.log(`[E2E] Stream routed via: ${routedVia}`);
      console.log(`[E2E] Streamed ${chunkCount} chunks, content: ${fullContent.slice(0, 100)}...`);

      if (lastChunk?.usage) {
        console.log(`[E2E] Usage: prompt=${lastChunk.usage.prompt_tokens}, completion=${lastChunk.usage.completion_tokens}, total=${lastChunk.usage.total_tokens}`);
      }
    }, 120000);

    it('should handle commit message with special characters in diff (URI safety)', async () => {
      const trickyDiff = `diff --git a/src/utils/parser.ts b/src/utils/parser.ts
--- a/src/utils/parser.ts
+++ b/src/utils/parser.ts
@@ -1,5 +1,10 @@
+// Fix: handle special chars like <>&"'\\\`$!#%~
+// URL-like strings in code: https://example.com/path?q=test&foo=bar#hash
+// Unicode: 日本語 中文 한국어 العربية
+const REGEX = /^[a-zA-Z0-9_.-]+$/;
+function sanitize(input: string): string {
+  return input.replace(/[<>]/g, (c) => c === '<' ? '&lt;' : '&gt;');
+}`;

      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: testModel,
          messages: [
            { role: 'system', content: 'Generate a commit message for the given diff.' },
            { role: 'user', content: trickyDiff },
          ],
          max_tokens: 100,
        }),
      });

      const data = await res.json() as any;
      if (res.status !== 200) {
        console.error(`[E2E DEBUG] URI Safety Status: ${res.status}`);
        console.error(`[E2E DEBUG] URI Safety Error: ${JSON.stringify(data.error)}`);
      }
      expect(res.status).toBe(200);

      expect(data.choices[0].message.content).toBeDefined();
      expect(typeof data.choices[0].message.content).toBe('string');

      const content = data.choices[0].message.content;
      expect(() => encodeURIComponent(content)).not.toThrow();

      console.log(`[E2E] Special chars test passed. Response: ${content.slice(0, 80)}`);
    }, 120000);

    it.skip('should return valid OpenAI-compatible response shape', async () => {
      const res = await app.request('/v1/chat/completions', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({
          model: testModel,
          messages: [{ role: 'user', content: 'Say "test"' }],
          max_tokens: 10,
        }),
      });

      const data = await res.json() as any;
      if (res.status !== 200) {
        console.error(`[E2E DEBUG] Shape Status: ${res.status}`);
        console.error(`[E2E DEBUG] Shape Error: ${JSON.stringify(data.error)}`);
      }
      expect(res.status).toBe(200);

      expect(data.id).toMatch(/^chatcmpl-/);
      expect(data.object).toBe('chat.completion');
      expect(typeof data.created).toBe('number');
      expect(typeof data.model).toBe('string');
      expect(Array.isArray(data.choices)).toBe(true);
      expect(data.choices[0].index).toBe(0);
      expect(data.choices[0].message.role).toBe('assistant');
      expect(data.choices[0].finish_reason).toBeDefined();
      expect(data.usage).toBeDefined();
      expect(typeof data.usage.prompt_tokens).toBe('number');
      expect(typeof data.usage.completion_tokens).toBe('number');
      expect(typeof data.usage.total_tokens).toBe('number');
    }, 120000);
  })
})
