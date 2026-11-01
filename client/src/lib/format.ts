/**
 * Shared number formatting for the dashboard. Technical values stay en-US so
 * tokens, prices and rates read the same in every locale.
 */

/** Prices are USD per 1M tokens. Zero is a real value (free), so it is kept. */
export function formatUsd(value: number | null | undefined): string | null {
  if (value == null || !Number.isFinite(value)) return null
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: value > 0 && value < 1 ? 4 : 2,
  }).format(value)
}

/** 1048576 → "1M", 128000 → "128K", 1200000000 → "1.2B". */
export function formatTokens(value: number): string {
  return new Intl.NumberFormat('en-US', { notation: 'compact', maximumFractionDigits: 1 }).format(value)
}
