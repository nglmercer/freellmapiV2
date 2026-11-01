import { useMemo } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { ChainFilters } from './ChainFilters'
import { SortMenu } from './SortMenu'
import { BulkMenu } from './BulkMenu'
import type { BulkAction, FallbackEntry, FallbackFilters, SortPreset } from './chain-model'
import { hasQuality, hasSpeed } from './chain-model'

/** Filters, a one-line tally of the chain, and the actions that change it. */
export function ChainToolbar({
  entries,
  filters,
  onFiltersChange,
  busy,
  bulkPending,
  onSort,
  onRunBulk,
}: {
  entries: FallbackEntry[]
  filters: FallbackFilters
  onFiltersChange: (next: FallbackFilters) => void
  busy: boolean
  bulkPending: boolean
  onSort: (preset: SortPreset) => void
  onRunBulk: (action: BulkAction) => Promise<unknown>
}) {
  const { t } = useTranslations()
  const counts = useMemo(
    () => ({
      total: entries.length,
      enabled: entries.filter((e) => e.enabled).length,
      free: entries.filter((e) => e.freeTier).length,
      qualityRanked: entries.filter(hasQuality).length,
      speedRanked: entries.filter(hasSpeed).length,
    }),
    [entries],
  )

  return (
    <section className="rounded-xl border bg-card">
      <div className="p-4">
        <ChainFilters entries={entries} value={filters} onChange={onFiltersChange} />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 border-t px-4 py-2.5">
        <p className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs tabular-nums text-muted-foreground">
          <span className="rounded-md bg-muted px-2 py-0.5 font-medium text-foreground">
            {t('fallback.summary.total', { count: counts.total })}
          </span>
          <span>{t('fallback.summary.enabled', { count: counts.enabled })}</span>
          {counts.free > 0 && <span>{t('fallback.summary.free', { count: counts.free })}</span>}
          <span>{t('fallback.summary.qualityRanked', { count: counts.qualityRanked })}</span>
          <span>{t('fallback.summary.speedRanked', { count: counts.speedRanked })}</span>
        </p>

        <div className="flex flex-wrap items-center gap-2">
          <SortMenu disabled={busy} onSelect={onSort} />
          <BulkMenu
            disabled={busy}
            hasFree={counts.free > 0}
            pending={bulkPending}
            onRun={onRunBulk}
          />
        </div>
      </div>
    </section>
  )
}
