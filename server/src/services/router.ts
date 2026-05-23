import { getDb } from '../db/index.js';
import { getProvider } from '../providers/index.js';
import { decrypt } from '../lib/crypto.js';
import { canMakeRequest, canUseTokens, isOnCooldown } from './ratelimit.js';
import type { BaseProvider } from '../providers/base.js';
import * as schema from '../db/schema.js';
import { eq, and, sql, asc, notInArray, count } from 'drizzle-orm';

export interface RouteResult {
  provider: BaseProvider;
  modelId: string;
  modelDbId: number;
  apiKey: string;
  keyId: number;
  platform: string;
  displayName: string;
}

// Round-robin index per platform
const roundRobinIndex = new Map<string, number>();

// ── Dynamic priority: track 429s per model and demote accordingly ──
// Key: model_db_id → { count, lastHit, penalty }
const rateLimitPenalties = new Map<number, { count: number; lastHit: number; penalty: number }>();

// Penalty decays over time so models recover
const PENALTY_PER_429 = 5;        // each 429 adds this many priority positions
const MAX_PENALTY = 15;            // cap so a model doesn't sink forever
const DECAY_INTERVAL_MS = 5 * 60 * 1000; // penalty decays every 5 minutes
const DECAY_AMOUNT = 1;            // remove this much penalty per decay interval

/**
 * Record a 429 for a model — increases its penalty so it sinks in priority.
 */
export function recordRateLimitHit(modelDbId: number) {
  const existing = rateLimitPenalties.get(modelDbId);
  const now = Date.now();
  if (existing) {
    existing.count++;
    existing.lastHit = now;
    existing.penalty = Math.min(existing.penalty + PENALTY_PER_429, MAX_PENALTY);
  } else {
    rateLimitPenalties.set(modelDbId, { count: 1, lastHit: now, penalty: PENALTY_PER_429 });
  }
}

/**
 * Record a success for a model — reduces its penalty so it rises back up.
 */
export function recordSuccess(modelDbId: number) {
  const existing = rateLimitPenalties.get(modelDbId);
  if (existing) {
    existing.penalty = Math.max(0, existing.penalty - 1);
    if (existing.penalty === 0) {
      rateLimitPenalties.delete(modelDbId);
    }
  }
}

/**
 * Get the current penalty for a model (with time-based decay).
 */
function getPenalty(modelDbId: number): number {
  const entry = rateLimitPenalties.get(modelDbId);
  if (!entry) return 0;

  // Apply time-based decay
  const now = Date.now();
  const elapsed = now - entry.lastHit;
  const decaySteps = Math.floor(elapsed / DECAY_INTERVAL_MS);
  if (decaySteps > 0) {
    entry.penalty = Math.max(0, entry.penalty - (decaySteps * DECAY_AMOUNT));
    entry.lastHit = now; // reset so we don't double-decay
    if (entry.penalty === 0) {
      rateLimitPenalties.delete(modelDbId);
      return 0;
    }
  }

  return entry.penalty;
}

/**
 * Get current penalties for all models (for the API/dashboard).
 */
export function getAllPenalties(): Array<{ modelDbId: number; count: number; penalty: number }> {
  const result: Array<{ modelDbId: number; count: number; penalty: number }> = [];
  for (const [modelDbId, entry] of rateLimitPenalties) {
    const penalty = getPenalty(modelDbId);
    if (penalty > 0) {
      result.push({ modelDbId, count: entry.count, penalty });
    }
  }
  return result.sort((a, b) => b.penalty - a.penalty);
}

/**
 * Route a request to the best available model.
 * Models are sorted by (base_priority + rate_limit_penalty) so frequently
 * rate-limited models automatically sink below working ones.
 *
 * If preferredModelDbId is set, that model gets tried FIRST (sticky sessions).
 * This prevents hallucination from model switching mid-conversation.
 *
 * @param estimatedTokens - estimated total tokens for rate limit check
 * @param skipKeys - set of "platform:modelId:keyId" to skip (failed on this request)
 * @param preferredModelDbId - try this model first (sticky session)
 */
export function routeRequest(estimatedTokens = 1000, skipKeys?: Set<string>, preferredModelDbId?: number): RouteResult {
  const db = getDb();
  const totalKeyCountResult = db.select({ count: sql<number>`count(*)` }).from(schema.apiKeys).where(eq(schema.apiKeys.enabled, 1)).get();
  const totalKeyCount = totalKeyCountResult?.count ?? 0;

  if (totalKeyCount === 0) {
    const err = new Error('No API keys configured. Add at least one key in the dashboard first.') as any;
    err.status = 503;
    throw err;
  }

  // Get fallback chain ordered by priority
  const fallbackChain = db.select({
    modelDbId: schema.fallbackConfig.modelDbId,
    priority: schema.fallbackConfig.priority,
    enabled: schema.fallbackConfig.enabled
  })
  .from(schema.fallbackConfig)
  .orderBy(asc(schema.fallbackConfig.priority))
  .all();

  // Apply dynamic penalties: sort by (base priority + penalty)
  const sortedChain = fallbackChain.map(entry => ({
    ...entry,
    effectivePriority: entry.priority + getPenalty(entry.modelDbId),
  })).sort((a, b) => a.effectivePriority - b.effectivePriority);

  // Sticky session: move preferred model to front of chain
  if (preferredModelDbId) {
    const idx = sortedChain.findIndex(e => e.modelDbId === preferredModelDbId);
    if (idx > 0) {
      const [preferred] = sortedChain.splice(idx, 1);
      sortedChain.unshift(preferred);
    }
  }

  for (const entry of sortedChain) {
    if (entry.enabled !== 1) continue;

    // Get model details
    const model = db.select().from(schema.models).where(and(eq(schema.models.id, entry.modelDbId), eq(schema.models.enabled, 1))).get();
    if (!model) continue;

    // Check if we have a provider for this platform
    const provider = getProvider(model.platform as any);
    if (!provider) continue;

    // Get all healthy, enabled keys for this platform
    const keys = db.select()
      .from(schema.apiKeys)
      .where(and(
        eq(schema.apiKeys.platform, model.platform),
        eq(schema.apiKeys.enabled, 1),
        notInArray(schema.apiKeys.status, ['invalid', 'rate_limited'])
      ))
      .all();

    if (keys.length === 0) continue;

    // Get limits once for this model
    const limits = {
      rpm: model.rpmLimit,
      rpd: model.rpdLimit,
      tpm: model.tpmLimit,
      tpd: model.tpdLimit,
    };

    // Try all keys for this model before giving up on it
    const rrKey = `${model.platform}:${model.modelId}`;
    let idx = roundRobinIndex.get(rrKey) ?? 0;

    for (let attempt = 0; attempt < keys.length; attempt++) {
      const key = keys[idx % keys.length];
      idx++;

      const skipId = `${model.platform}:${model.modelId}:${key.id}`;
      if (skipKeys?.has(skipId)) continue;

      // Check cooldown (from previous 429s)
      if (isOnCooldown(model.platform, model.modelId, key.id)) continue;

      if (!canMakeRequest(model.platform, model.modelId, key.id, limits)) continue;
      if (!canUseTokens(model.platform, model.modelId, key.id, estimatedTokens, limits)) continue;

      // We found a working key for this model!
      roundRobinIndex.set(rrKey, idx);
      let decryptedKey: string;
      try {
        decryptedKey = decrypt(key.encryptedKey, key.iv, key.authTag);
      } catch {
        // Decryption failed (mismatched encryption key). Skip this key.
        continue;
      }

      return {
        provider,
        modelId: model.modelId,
        modelDbId: model.id,
        apiKey: decryptedKey,
        keyId: key.id,
        platform: model.platform,
        displayName: model.displayName,
      };
    }

    // If we reach here, this specific model has NO available keys.
    // Update round-robin index even if we failed so we don't get stuck.
    roundRobinIndex.set(rrKey, idx);

    // We don't explicitly penalize the model here because the fact that we
    // couldn't find a key means we will naturally move to the next model
    // in the `sortedChain` for THIS specific request.
  }

  // Check if there are any non-invalid keys at all to give a better diagnostic
  const decryptableCountResult = db.select({ count: sql<number>`count(*)` })
    .from(schema.apiKeys)
    .where(and(eq(schema.apiKeys.enabled, 1), sql`${schema.apiKeys.status} != 'invalid'`))
    .get();
  const decryptableCount = decryptableCountResult?.count ?? 0;

  let message: string;
  if (decryptableCount === 0) {
    message = 'All configured API keys are invalid. Check your keys in the dashboard.';
  } else if (totalKeyCount > 0 && skipKeys && skipKeys.size >= totalKeyCount) {
    message = `All ${totalKeyCount} API key(s) have been tried and failed. Wait for rate-limit cooldown or add more keys.`;
  } else {
    message = 'All models are currently unavailable due to rate limits or cooldowns. Try again later.';
  }

  const err = new Error(message) as any;
  err.status = 429;
  throw err;
}
