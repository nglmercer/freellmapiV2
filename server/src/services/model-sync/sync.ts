import { getModels } from 'getmodelsapi';
import { getDb, runInTransaction } from '../../db/index.js';
import * as schema from '../../db/schema.js';
import { eq, and, max } from 'drizzle-orm';
import { getPlatformByProvider, CURATION_DEFAULTS } from './mappings.js';

export interface SyncChange {
  changeType: 'added' | 'updated' | 'disabled' | 'free_to_paid' | 'paid_to_free' | 'stored';
  platform: string;
  modelId: string;
  displayName: string;
  details?: string;
}

export interface SyncResult {
  totalDiscovered: number;
  added: number;
  updated: number;
  disabled: number;
  freeToPaid: number;
  paidToFree: number;
  storedDisabled: number;
  changes: SyncChange[];
}

function insertSyncChanges(
  db: ReturnType<typeof getDb>,
  logId: number,
  changes: SyncChange[],
): void {
  for (const change of changes) {
    db.insert(schema.syncChanges).values({
      syncLogId: logId,
      changeType: change.changeType,
      platform: change.platform,
      modelId: change.modelId,
      displayName: change.displayName,
      details: change.details ?? null,
    }).run();
  }
}

export async function syncModels(): Promise<SyncResult> {
  const db = getDb();
  const startedAt = new Date().toISOString();

  const logResult = db.insert(schema.syncLog).values({
    startedAt,
    status: 'running',
  }).returning({ id: schema.syncLog.id }).get();
  const logId = logResult!.id;

  try {
    const externalModels = await getModels({ limit: 2000 });
    const result: SyncResult = {
      totalDiscovered: externalModels.length,
      added: 0,
      updated: 0,
      disabled: 0,
      freeToPaid: 0,
      paidToFree: 0,
      storedDisabled: 0,
      changes: [],
    };

    const currentModels = db.select().from(schema.models).all();
    const currentMap = new Map<string, (typeof currentModels)[number]>();
    for (const m of currentModels) {
      currentMap.set(`${m.platform}:${m.modelId}`, m);
    }

    const seenKeys = new Set<string>();
    const newModelInserts: Array<typeof schema.models.$inferInsert> = [];
    const updates: Array<{ id: number; data: Record<string, unknown> }> = [];
    const disables: number[] = [];

    for (const ext of externalModels) {
      const platform = getPlatformByProvider(ext.provider);
      if (!platform) continue;

      const key = `${platform}:${ext.id}`;
      seenKeys.add(key);
      const existing = currentMap.get(key);
      const isFree = ext.freeTier === true;

      if (!existing) {
        newModelInserts.push({
          platform,
          modelId: ext.id,
          displayName: ext.name,
          ...CURATION_DEFAULTS,
          contextWindow: ext.contextWindow,
          pricingPrompt: ext.pricing?.prompt ?? null,
          pricingCompletion: ext.pricing?.completion ?? null,
          freeTier: isFree ? 1 : 0,
          gateway: ext.gateway ?? null,
          supportedFeatures: JSON.stringify(ext.supportedFeatures ?? []),
          externalUrl: ext.url ?? null,
          description: ext.description ?? null,
          lastSyncedAt: startedAt,
          source: 'getmodelsapi',
          enabled: isFree ? 1 : 0,
        });

        if (isFree) {
          result.added++;
          result.changes.push({
            changeType: 'added', platform, modelId: ext.id, displayName: ext.name,
          });
        } else {
          result.storedDisabled++;
          result.changes.push({
            changeType: 'stored', platform, modelId: ext.id, displayName: ext.name,
          });
        }
      } else {
        const oldFreeTier = existing.freeTier === 1;
        const newFreeTier = isFree;

        if (oldFreeTier && !newFreeTier) {
          result.freeToPaid++;
          result.changes.push({
            changeType: 'free_to_paid', platform, modelId: ext.id, displayName: ext.name,
            details: JSON.stringify({ before: { freeTier: true }, after: { freeTier: false } }),
          });
          disables.push(existing.id);
          continue;
        }

        if (!oldFreeTier && newFreeTier) {
          result.paidToFree++;
          result.changes.push({
            changeType: 'paid_to_free', platform, modelId: ext.id, displayName: ext.name,
          });
        }

        updates.push({
          id: existing.id,
          data: {
            displayName: ext.name,
            contextWindow: ext.contextWindow,
            pricingPrompt: ext.pricing?.prompt ?? null,
            pricingCompletion: ext.pricing?.completion ?? null,
            freeTier: isFree ? 1 : 0,
            gateway: ext.gateway ?? null,
            supportedFeatures: JSON.stringify(ext.supportedFeatures ?? []),
            externalUrl: ext.url ?? null,
            description: ext.description ?? null,
          },
        });
        result.updated++;
      }
    }

    for (const [key, model] of currentMap) {
      if (!seenKeys.has(key) && model.source === 'getmodelsapi' && model.enabled === 1) {
        disables.push(model.id);
        result.disabled++;
        result.changes.push({
          changeType: 'disabled', platform: model.platform,
          modelId: model.modelId, displayName: model.displayName,
        });
      }
    }

    runInTransaction(() => {
      for (const ins of newModelInserts) {
        db.insert(schema.models).values(ins).onConflictDoNothing().run();
      }
      for (const up of updates) {
        const data = up.data as any;
        db.update(schema.models)
          .set({ ...data, lastSyncedAt: startedAt })
          .where(eq(schema.models.id, up.id))
          .run();
      }
      for (const id of disables) {
        db.update(schema.models)
          .set({ enabled: 0, lastSyncedAt: startedAt })
          .where(eq(schema.models.id, id))
          .run();
      }
      // Only add free + enabled models to fallback_config
      for (const ins of newModelInserts) {
        if (!ins.enabled) continue;
        const inserted = db.select({ id: schema.models.id })
          .from(schema.models)
          .where(and(eq(schema.models.platform, ins.platform), eq(schema.models.modelId, ins.modelId!)))
          .get();
        if (!inserted) continue;
        const exists = db.select({ id: schema.fallbackConfig.id })
          .from(schema.fallbackConfig)
          .where(eq(schema.fallbackConfig.modelDbId, inserted.id))
          .get();
        if (!exists) {
          const maxP = db.select({ mx: max(schema.fallbackConfig.priority) })
            .from(schema.fallbackConfig).get();
          db.insert(schema.fallbackConfig).values({
            modelDbId: inserted.id,
            priority: (maxP?.mx ?? 0) + 1,
            enabled: 1,
          }).run();
        }
      }
    });

    insertSyncChanges(db, logId, result.changes);

    db.update(schema.syncLog).set({
      completedAt: new Date().toISOString(),
      status: 'completed',
      totalDiscovered: result.totalDiscovered,
      added: result.added,
      updated: result.updated,
      disabled: result.disabled,
      freeToPaid: result.freeToPaid,
      paidToFree: result.paidToFree,
      storedDisabled: result.storedDisabled,
    }).where(eq(schema.syncLog.id, logId)).run();

    return result;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    db.update(schema.syncLog).set({
      completedAt: new Date().toISOString(),
      status: 'failed',
      error: message,
    }).where(eq(schema.syncLog.id, logId)).run();
    throw err;
  }
}
