/** Shared display helpers. Other pages import these, so keep the exports stable. */

export function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value))
}

export function formatDate(iso: string | null | undefined): string | null {
  if (!iso) return null
  const date = new Date(iso)
  return Number.isNaN(date.getTime()) ? null : date.toLocaleDateString()
}
