import type { FallbackEntry } from '../../../shared/types'

export type { FallbackEntry }

export type SortPreset = 'manual' | 'intelligence' | 'quality' | 'speed' | 'fastest' | 'reliability' | 'balanced' | 'budget'

export type TierFilter = 'all' | 'free' | 'paid'
export type StateFilter = 'all' | 'enabled' | 'disabled'
export type RankingFilter = 'all' | 'ranked' | 'unranked' | 'high-confidence' | 'has-quality' | 'has-speed' | 'local-performance'

export interface FallbackFilters {
  search: string
  platform: string
  tier: TierFilter
  state: StateFilter
  ranking: RankingFilter
}

export const EMPTY_FILTERS: FallbackFilters = {
  search: '',
  platform: '',
  tier: 'all',
  state: 'all',
  ranking: 'all',
}

export function isRanked(entry: FallbackEntry): boolean {
  return entry.intelligenceScore != null || hasSpeed(entry)
}

export function hasQuality(entry: FallbackEntry): boolean {
  return entry.intelligenceScore != null || entry.quality?.score != null
}

export function hasSpeed(entry: FallbackEntry): boolean {
  return entry.speedTokensPerSec != null || entry.speed?.tokensPerSecond != null
}

export function hasLocalPerformance(entry: FallbackEntry): boolean {
  return (entry.speed?.sampleCount ?? entry.reliability?.sampleCount ?? 0) > 0
}

export function filterEntries(entries: FallbackEntry[], f: FallbackFilters): FallbackEntry[] {
  const q = f.search.trim().toLowerCase()
  return entries.filter((e) => {
    if (f.platform && e.platform !== f.platform) return false
    if (f.tier === 'free' && !e.freeTier) return false
    if (f.tier === 'paid' && e.freeTier) return false
    if (f.state === 'enabled' && !e.enabled) return false
    if (f.state === 'disabled' && e.enabled) return false
    if (f.ranking === 'ranked' && !isRanked(e)) return false
    if (f.ranking === 'unranked' && isRanked(e)) return false
    if (f.ranking === 'high-confidence' && (e.rankingConfidence ?? 0) < 0.8) return false
    if (f.ranking === 'has-quality' && !hasQuality(e)) return false
    if (f.ranking === 'has-speed' && !hasSpeed(e)) return false
    if (f.ranking === 'local-performance' && !hasLocalPerformance(e)) return false
    if (q) {
      const hay = `${e.displayName} ${e.modelId} ${e.platform}`.toLowerCase()
      if (!hay.includes(q)) return false
    }
    return true
  })
}
