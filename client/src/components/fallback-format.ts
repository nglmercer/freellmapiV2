import type { FallbackEntry } from '../../../shared/types'

export type Translate = (key: string, options?: Record<string, unknown>) => string

/** 1.2K, 340K, 1.0M: compact counts for tokens and context windows. */
export function compactNumber(value: number): string {
  if (value >= 1_000_000_000) return `${(value / 1_000_000_000).toFixed(1)}B`
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}K`
  return String(value)
}

export function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value))
}

/** USD per 1M tokens, e.g. "$0.13" or "$12". */
export function usdPerMillion(value: number | null | undefined): string | null {
  if (value == null || !Number.isFinite(value)) return null
  return `$${value.toFixed(value < 10 ? 2 : 1)}`
}

export function qualityOf(entry: FallbackEntry): number | null {
  return entry.quality?.score ?? entry.intelligenceScore ?? null
}

export function speedOf(entry: FallbackEntry): number | null {
  return entry.speed?.tokensPerSecond ?? entry.speedTokensPerSec ?? null
}

/** Why a quality or speed value is missing, phrased as the next step. */
export function missingReason(entry: FallbackEntry, metric: 'quality' | 'speed', t: Translate): string {
  if (metric === 'quality') {
    return entry.qualityState === 'needs_source'
      ? t('fallback.state.needsSource')
      : t('fallback.state.noData')
  }
  return entry.speedState === 'awaiting_requests'
    ? t('fallback.state.awaitingRequests')
    : t('fallback.state.noData')
}

export function formatDate(iso: string | null | undefined): string | null {
  if (!iso) return null
  const date = new Date(iso)
  return Number.isNaN(date.getTime()) ? null : date.toLocaleDateString()
}
