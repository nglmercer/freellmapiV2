/**
 * Presentation formatting for the model explorer. Numbers stay in en-US so
 * technical values (tokens, prices, tok/s) read the same in every locale.
 */

export { formatTokens, formatUsd } from '@/lib/format'

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
