/**
 * Presentation formatting for the model explorer. Numbers stay in en-US so
 * technical values (tokens, prices, tok/s) read the same in every locale.
 */

/** Prices are USD per 1M tokens. Zero is a real value (free), so it is kept. */
export function formatUsd(value: number | null | undefined): string | null {
  if (value == null) return null
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: value > 0 && value < 1 ? 4 : 2,
  }).format(value)
}

/** 1048576 → "1M", 128000 → "128K". */
export function formatTokens(value: number): string {
  return new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 }).format(value)
}

export function formatScore(value: number): string {
  return value.toFixed(1)
}

export function formatTps(value: number): string {
  return `${value.toFixed(1)} tok/s`
}

export function formatMs(value: number): string {
  return `${Math.round(value).toLocaleString('en-US')} ms`
}

export function formatPercent(ratio: number): string {
  return `${Math.round(ratio * 100)}%`
}

export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString()
}

/** Clamp a 0–1 ratio so bars never overflow their track. */
export function clampRatio(value: number): number {
  return Math.min(1, Math.max(0, value))
}
