import { arrayMove } from '@dnd-kit/sortable'
import type { UniqueIdentifier } from '@dnd-kit/core'
import type { FallbackEntry } from '@freellmapi/shared'
import { platformInfo } from '@/lib/platforms'

export type { FallbackEntry }

export type SortPreset = 'manual' | 'balanced' | 'quality' | 'fastest' | 'reliability' | 'budget'

export type TierFilter = 'all' | 'free' | 'paid'
export type StateFilter = 'all' | 'enabled' | 'disabled'
export type RankingFilter =
  | 'all'
  | 'ranked'
  | 'unranked'
  | 'high-confidence'
  | 'has-quality'
  | 'has-speed'
  | 'local-performance'

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

export function hasFilters(filters: FallbackFilters): boolean {
  return (
    filters.search !== '' ||
    filters.platform !== '' ||
    filters.tier !== 'all' ||
    filters.state !== 'all' ||
    filters.ranking !== 'all'
  )
}

/** Quality comes from the registry or the legacy intelligence score. */
export function qualityOf(entry: FallbackEntry): number | null {
  return entry.quality?.score ?? entry.intelligenceScore ?? null
}

/** Speed comes from measured requests or the legacy tokens/sec field. */
export function speedOf(entry: FallbackEntry): number | null {
  return entry.speed?.tokensPerSecond ?? entry.speedTokensPerSec ?? null
}

export function hasQuality(entry: FallbackEntry): boolean {
  return qualityOf(entry) != null
}

export function hasSpeed(entry: FallbackEntry): boolean {
  return speedOf(entry) != null
}

export function hasLocalPerformance(entry: FallbackEntry): boolean {
  return (entry.speed?.sampleCount ?? entry.reliability?.sampleCount ?? 0) > 0
}

export function isRanked(entry: FallbackEntry): boolean {
  return entry.intelligenceScore != null || hasSpeed(entry)
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
      const haystack = `${e.displayName} ${e.modelId} ${e.platform}`.toLowerCase()
      if (!haystack.includes(q)) return false
    }
    return true
  })
}

/**
 * Applies a drag between two visible rows to the full chain. Hidden entries
 * keep their slots: visible rows are reshuffled among the positions they
 * already occupy, then every entry is renumbered so priorities stay contiguous.
 * Returns null when the drop is a no-op.
 */
export function reorderEntries(
  allEntries: FallbackEntry[],
  visibleEntries: FallbackEntry[],
  activeId: UniqueIdentifier,
  overId: UniqueIdentifier,
): FallbackEntry[] | null {
  if (activeId === overId) return null
  const oldIndex = visibleEntries.findIndex((e) => e.modelDbId === activeId)
  const newIndex = visibleEntries.findIndex((e) => e.modelDbId === overId)
  if (oldIndex < 0 || newIndex < 0) return null

  const reorderedVisible = arrayMove(visibleEntries, oldIndex, newIndex)
  const visibleIds = new Set(visibleEntries.map((e) => e.modelDbId))
  let visibleIndex = 0
  return allEntries
    .map((e) => (visibleIds.has(e.modelDbId) ? reorderedVisible[visibleIndex++] : e))
    .map((e, i) => ({ ...e, priority: i + 1 }))
}

export function toSavePayload(entries: FallbackEntry[]) {
  return entries.map((e) => ({
    modelDbId: e.modelDbId,
    priority: e.priority,
    enabled: e.enabled,
  }))
}

export type BulkAction = 'enableAll' | 'disableAll' | 'enableFree'

export interface BulkActionResult {
  modelsUpdated?: number
  fallbackEntriesUpdated?: number
  freeEnabled?: number
  paidDisabled?: number
}

export function bulkActionPath(action: BulkAction): string {
  if (action === 'enableAll') return '/api/models/enable-all'
  if (action === 'disableAll') return '/api/models/disable-all'
  return '/api/models/enable-free'
}

export function buildBulkResultMessage(
  t: (key: string, options?: Record<string, unknown>) => string,
  action: BulkAction,
  data: BulkActionResult,
): string {
  if (action === 'enableFree') {
    return t('fallback.bulk.result.enableFree', {
      count: 1,
      free: data.freeEnabled ?? 0,
      paid: data.paidDisabled ?? 0,
    })
  }
  const count = data.modelsUpdated ?? 0
  return t(`fallback.bulk.result.${action}`, {
    count,
    models: count,
    fallback: data.fallbackEntriesUpdated ?? 0,
  })
}

export interface TokenUsageData {
  totalBudget: number
  totalUsed: number
  models: { displayName: string; platform: string; budget: number }[]
}

/** Display name for a provider id, falling back to the id itself. */
export function providerLabel(platform: string): string {
  return platformInfo(platform)?.label ?? platform
}
