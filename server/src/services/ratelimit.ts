// In-memory sliding window rate limit tracker
import crypto from 'crypto';
import type { ChatMessage } from '@freellmapi/shared/types.js'; // Imported to keep types unified

interface Window {
  timestamps: number[];
  tokenCount: number;
  tokenTimestamps: { ts: number; tokens: number }[];
}

// Key format: "platform:modelId:keyId:type" where type is rpm|rpd|tpm|tpd
const windows = new Map<string, Window>();

function getWindow(key: string): Window {
  let w = windows.get(key);
  if (!w) {
    w = { timestamps: [], tokenCount: 0, tokenTimestamps: [] };
    windows.set(key, w);
  }
  return w;
}

function pruneTimestamps(timestamps: number[], windowMs: number, now: number): number[] {
  const cutoff = now - windowMs;
  return timestamps.filter(ts => ts > cutoff);
}

const MINUTE = 60 * 1000;
const DAY = 24 * 60 * MINUTE;

export function canMakeRequest(
  platform: string,
  modelId: string,
  keyId: number,
  limits: { rpm: number | null; rpd: number | null; tpm: number | null; tpd: number | null },
): boolean {
  const now = Date.now();

  if (limits.rpm !== null) {
    const key = `${platform}:${modelId}:${keyId}:rpm`;
    const w = getWindow(key);
    w.timestamps = pruneTimestamps(w.timestamps, MINUTE, now);
    if (w.timestamps.length >= limits.rpm) return false;
  }

  if (limits.rpd !== null) {
    const key = `${platform}:${modelId}:${keyId}:rpd`;
    const w = getWindow(key);
    w.timestamps = pruneTimestamps(w.timestamps, DAY, now);
    if (w.timestamps.length >= limits.rpd) return false;
  }

  return true;
}

export function canUseTokens(
  platform: string,
  modelId: string,
  keyId: number,
  estimatedTokens: number,
  limits: { tpm: number | null; tpd: number | null },
): boolean {
  const now = Date.now();

  if (limits.tpm !== null) {
    const key = `${platform}:${modelId}:${keyId}:tpm`;
    const w = getWindow(key);
    w.tokenTimestamps = w.tokenTimestamps.filter(t => t.ts > now - MINUTE);
    const used = w.tokenTimestamps.reduce((sum, t) => sum + t.tokens, 0);
    if (used + estimatedTokens > limits.tpm) return false;
  }

  if (limits.tpd !== null) {
    const key = `${platform}:${modelId}:${keyId}:tpd`;
    const w = getWindow(key);
    w.tokenTimestamps = w.tokenTimestamps.filter(t => t.ts > now - DAY);
    const used = w.tokenTimestamps.reduce((sum, t) => sum + t.tokens, 0);
    if (used + estimatedTokens > limits.tpd) return false;
  }

  return true;
}

export function recordRequest(platform: string, modelId: string, keyId: number) {
  const now = Date.now();

  const rpmKey = `${platform}:${modelId}:${keyId}:rpm`;
  getWindow(rpmKey).timestamps.push(now);

  const rpdKey = `${platform}:${modelId}:${keyId}:rpd`;
  getWindow(rpdKey).timestamps.push(now);
}

export function recordTokens(
  platform: string,
  modelId: string,
  keyId: number,
  tokens: number,
) {
  const now = Date.now();

  const tpmKey = `${platform}:${modelId}:${keyId}:tpm`;
  getWindow(tpmKey).tokenTimestamps.push({ ts: now, tokens });

  const tpdKey = `${platform}:${modelId}:${keyId}:tpd`;
  getWindow(tpdKey).tokenTimestamps.push({ ts: now, tokens });
}

// Cooldown: when a provider returns 429, block that model+key for a period
const cooldowns = new Map<string, number>(); // key -> expiry timestamp

export function setCooldown(platform: string, modelId: string, keyId: number, durationMs = 60_000) {
  const key = `${platform}:${modelId}:${keyId}:cooldown`;
  cooldowns.set(key, Date.now() + durationMs);
}

export function isOnCooldown(platform: string, modelId: string, keyId: number): boolean {
  const key = `${platform}:${modelId}:${keyId}:cooldown`;
  const expiry = cooldowns.get(key);
  if (!expiry) return false;
  if (Date.now() > expiry) {
    cooldowns.delete(key);
    return false;
  }
  return true;
}

export function getRateLimitStatus(
  platform: string,
  modelId: string,
  keyId: number,
  limits: { rpm: number | null; rpd: number | null; tpm: number | null; tpd: number | null },
) {
  const now = Date.now();

  const rpmW = getWindow(`${platform}:${modelId}:${keyId}:rpm`);
  rpmW.timestamps = pruneTimestamps(rpmW.timestamps, MINUTE, now);

  const rpdW = getWindow(`${platform}:${modelId}:${keyId}:rpd`);
  rpdW.timestamps = pruneTimestamps(rpdW.timestamps, DAY, now);

  const tpmW = getWindow(`${platform}:${modelId}:${keyId}:tpm`);
  tpmW.tokenTimestamps = tpmW.tokenTimestamps.filter(t => t.ts > now - MINUTE);
  const tpmUsed = tpmW.tokenTimestamps.reduce((sum, t) => sum + t.tokens, 0);

  return {
    rpm: { used: rpmW.timestamps.length, limit: limits.rpm },
    rpd: { used: rpdW.timestamps.length, limit: limits.rpd },
    tpm: { used: tpmUsed, limit: limits.tpm },
  };
}

// Sticky sessions: track which model served each "session"
// Key: hash of first user message → model_db_id
const stickySessionMap = new Map<string, { modelDbId: number; lastUsed: number }>();
const STICKY_TTL_MS = 30 * 60 * 1000; // 30 min session TTL

export function getSessionKey(messages: ChatMessage[]): string {
  const firstUser = messages.find(m => m.role === 'user');

  // Guard clause checking that content is a valid, non-null string before hashing
  if (!firstUser || typeof firstUser.content !== 'string') return '';

  const hash = crypto.createHash('sha1').update(firstUser.content).digest('hex');
  return `${hash}:${messages.length > 2 ? 'multi' : 'single'}`;
}

export function getStickyModel(messages: ChatMessage[]): number | undefined {
  const hasAssistant = messages.some(m => m.role === 'assistant');
  if (!hasAssistant) return undefined;

  const key = getSessionKey(messages);
  if (!key) return undefined;

  const entry = stickySessionMap.get(key);
  if (!entry) return undefined;

  if (Date.now() - entry.lastUsed > STICKY_TTL_MS) {
    stickySessionMap.delete(key);
    return undefined;
  }
  return entry.modelDbId;
}

export function setStickyModel(messages: ChatMessage[], modelDbId: number) {
  const key = getSessionKey(messages);
  if (!key) return;
  stickySessionMap.set(key, { modelDbId, lastUsed: Date.now() });

  if (stickySessionMap.size > 500) {
    const now = Date.now();
    for (const [k, v] of stickySessionMap) {
      if (now - v.lastUsed > STICKY_TTL_MS) stickySessionMap.delete(k);
    }
  }
}
