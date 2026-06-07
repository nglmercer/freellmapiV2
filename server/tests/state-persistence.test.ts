import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import fs from 'fs';
import path from 'path';
import { DB_PATH } from '../src/db/connection.js';
import {
  snapshotRateLimitState,
  restoreRateLimitState,
  setCooldown,
  isOnCooldown,
  recordRequest,
  canMakeRequest,
  setStickyModel,
  getStickyModel,
} from '../src/services/ratelimit.js';
import {
  snapshotRouterState,
  restoreRouterState,
  recordRateLimitHit,
  getAllPenalties,
} from '../src/services/router.js';
import {
  snapshotHealthState,
  restoreHealthState,
} from '../src/services/health.js';
import {
  saveRuntimeState,
  restoreRuntimeState,
  clearRuntimeState,
} from '../src/services/state-persistence.js';
import type { ChatMessage } from '@freellmapi/shared/types.js';

describe('State Persistence', () => {
  describe('Rate Limit snapshot/restore', () => {
    it('should preserve cooldowns across snapshot/restore', () => {
      setCooldown('google', 'gemini-pro', 1, 60_000);
      expect(isOnCooldown('google', 'gemini-pro', 1)).toBe(true);

      const snap = snapshotRateLimitState();

      restoreRateLimitState({ windows: [], cooldowns: [], stickySessions: [] });
      expect(isOnCooldown('google', 'gemini-pro', 1)).toBe(false);

      restoreRateLimitState(snap);
      expect(isOnCooldown('google', 'gemini-pro', 1)).toBe(true);
    });

    it('should preserve rate limit windows across snapshot/restore', () => {
      recordRequest('groq', 'llama-3', 5);
      const canBefore = canMakeRequest('groq', 'llama-3', 5, { rpm: 1, rpd: null, tpm: null, tpd: null });
      expect(canBefore).toBe(false);

      const snap = snapshotRateLimitState();

      restoreRateLimitState({ windows: [], cooldowns: [], stickySessions: [] });
      const canAfterClear = canMakeRequest('groq', 'llama-3', 5, { rpm: 1, rpd: null, tpm: null, tpd: null });
      expect(canAfterClear).toBe(true);

      restoreRateLimitState(snap);
      const canAfterRestore = canMakeRequest('groq', 'llama-3', 5, { rpm: 1, rpd: null, tpm: null, tpd: null });
      expect(canAfterRestore).toBe(false);
    });

    it('should preserve sticky sessions across snapshot/restore', () => {
      const messages: ChatMessage[] = [
        { role: 'user', content: 'hello world test session' },
        { role: 'assistant', content: 'hi there' },
      ];
      setStickyModel(messages, 42);
      expect(getStickyModel(messages)).toBe(42);

      const snap = snapshotRateLimitState();

      restoreRateLimitState({ windows: [], cooldowns: [], stickySessions: [] });
      expect(getStickyModel(messages)).toBeUndefined();

      restoreRateLimitState(snap);
      expect(getStickyModel(messages)).toBe(42);
    });
  });

  describe('Router snapshot/restore', () => {
    it('should preserve rate limit penalties across snapshot/restore', () => {
      recordRateLimitHit(1);
      recordRateLimitHit(1);
      const penaltiesBefore = getAllPenalties();
      expect(penaltiesBefore.length).toBeGreaterThan(0);
      const model1 = penaltiesBefore.find(p => p.modelDbId === 1);
      expect(model1).toBeDefined();
      expect(model1!.penalty).toBeGreaterThan(0);

      const snap = snapshotRouterState();

      restoreRouterState({ roundRobinIndex: [], rateLimitPenalties: [] });
      const penaltiesAfterClear = getAllPenalties();
      expect(penaltiesAfterClear.find(p => p.modelDbId === 1)).toBeUndefined();

      restoreRouterState(snap);
      const penaltiesAfterRestore = getAllPenalties();
      const model1Restored = penaltiesAfterRestore.find(p => p.modelDbId === 1);
      expect(model1Restored).toBeDefined();
      expect(model1Restored!.penalty).toBe(model1!.penalty);
    });

    it('should preserve round-robin index across snapshot/restore', () => {
      const snap = snapshotRouterState();
      expect(snap.roundRobinIndex).toBeDefined();
      expect(Array.isArray(snap.roundRobinIndex)).toBe(true);
    });
  });

  describe('Health snapshot/restore', () => {
    it('should produce a valid snapshot', () => {
      const snap = snapshotHealthState();
      expect(snap).toHaveProperty('failureCount');
      expect(Array.isArray(snap.failureCount)).toBe(true);
    });

    it('should restore from snapshot without error', () => {
      const snap = snapshotHealthState();
      expect(() => restoreHealthState(snap)).not.toThrow();
    });
  });

  describe('Full save/restore cycle to disk', () => {
    const stateFile = path.resolve(path.dirname(DB_PATH), 'runtime-state.json');

    beforeEach(() => {
      restoreRateLimitState({ windows: [], cooldowns: [], stickySessions: [] });
      restoreRouterState({ roundRobinIndex: [], rateLimitPenalties: [] });
      restoreHealthState({ failureCount: [] });
    });

    afterEach(() => {
      clearRuntimeState();
    });

    it('should save state to disk and restore it', () => {
      setCooldown('openai', 'gpt-4', 10, 60_000);
      recordRateLimitHit(3);

      saveRuntimeState();

      restoreRateLimitState({ windows: [], cooldowns: [], stickySessions: [] });
      restoreRouterState({ roundRobinIndex: [], rateLimitPenalties: [] });

      expect(isOnCooldown('openai', 'gpt-4', 10)).toBe(false);

      const restored = restoreRuntimeState();
      expect(restored).toBe(true);

      expect(isOnCooldown('openai', 'gpt-4', 10)).toBe(true);
      const penalties = getAllPenalties();
      expect(penalties.find(p => p.modelDbId === 3)).toBeDefined();
    });

    it('should return false when no state file exists', () => {
      const restored = restoreRuntimeState();
      expect(restored).toBe(false);
    });

    it('should discard stale state older than 60 minutes', () => {
      setCooldown('test', 'model', 1, 60_000);
      saveRuntimeState();

      const raw = fs.readFileSync(stateFile, 'utf-8');
      const parsed = JSON.parse(raw);
      parsed.savedAt = Date.now() - 61 * 60 * 1000;
      fs.writeFileSync(stateFile, JSON.stringify(parsed));

      const restored = restoreRuntimeState();
      expect(restored).toBe(false);
    });
  });
});
