export interface FallbackEntry {
  modelDbId: number
  priority: number
  effectivePriority: number
  penalty: number
  rateLimitHits: number
  enabled: boolean
  freeTier: boolean
  platform: string
  modelId: string
  displayName: string
  intelligenceRank: number
  speedRank: number
  intelligenceScore: number | null
  speedTokensPerSec: number | null
  rankingSource: string | null
  lastRankedAt: string | null
  sizeLabel: string
  rpmLimit: number | null
  rpdLimit: number | null
  monthlyTokenBudget: string
  keyCount: number
}

export type SortPreset = 'intelligence' | 'speed' | 'budget'

export type TierFilter = 'all' | 'free' | 'paid'
export type StateFilter = 'all' | 'enabled' | 'disabled'
export type RankingFilter = 'all' | 'ranked' | 'unranked'

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
  return entry.intelligenceScore != null
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
    if (q) {
      const hay = `${e.displayName} ${e.modelId} ${e.platform}`.toLowerCase()
      if (!hay.includes(q)) return false
    }
    return true
  })
}
