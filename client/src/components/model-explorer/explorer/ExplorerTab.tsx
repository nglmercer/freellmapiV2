import { useMemo, useState } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogDescription,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { SummaryStats } from '@/components/summary-stats'
import { Comparison } from '../Comparison'
import { summarize } from '../metrics'
import { formatPercent } from '../format'
import { ExplorerFilters } from './ExplorerFilters'
import { ModelGroup } from './ModelGroup'
import { CompareTray } from './CompareTray'
import { CatalogPagination } from './CatalogPagination'
import { countActiveFilters, MAX_COMPARE, PAGE_SIZE } from './filter-state'
import { useExplorerQuery } from './use-explorer-query'
import type { ExplorerState } from './use-explorer-state'

/** The model catalog: stats, filters, grouped results and comparison. */
export function ExplorerTab({ state }: { state: ExplorerState }) {
  const { t } = useTranslations()
  const query = useExplorerQuery(state.filters, state.offset)
  const [compareOpen, setCompareOpen] = useState(false)

  const models = useMemo(() => query.data?.data ?? [], [query.data])
  const total = query.data?.total ?? 0
  const deployments = useMemo(() => models.flatMap((m) => m.deployments), [models])
  const summaries = useMemo(() => deployments.map(summarize), [deployments])
  const maxSpeed = Math.max(0, ...summaries.map((s) => s.speed ?? 0))
  const providers = new Set(deployments.map((d) => d.provider)).size
  const freeCount = deployments.filter((d) => d.freeTier).length
  const qualityCount = summaries.filter((s) => s.quality != null).length
  const qualityShare = deployments.length > 0 ? qualityCount / deployments.length : 0
  const selectedIds = useMemo(() => new Set(state.selection.map((s) => s.id)), [state.selection])
  const hasFilters = countActiveFilters(state.filters) > 0
  const showing = query.isPlaceholderData

  return (
    <div className="space-y-5">
      <SummaryStats
        items={[
          {
            label: t('metadata.stats.models'),
            value: total,
            detail: t('metadata.stats.onPage', { count: models.length }),
          },
          {
            label: t('metadata.stats.deployments'),
            value: deployments.length,
            detail: t('metadata.stats.free', { count: freeCount }),
          },
          {
            label: t('metadata.stats.providers'),
            value: providers,
            detail: t('metadata.stats.onPage', { count: models.length }),
          },
          {
            label: t('metadata.stats.withQuality'),
            value: qualityCount,
            detail: qualityCount === 0 ? t('metadata.stats.addSource') : formatPercent(qualityShare),
            ratio: qualityShare,
            tone: 'quality',
          },
        ]}
      />

      <ExplorerFilters value={state.filters} onChange={state.setFilters} onClear={state.resetFilters} />

      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs tabular-nums text-muted-foreground" aria-live="polite">
          {query.isSuccess && total > 0 &&
            t('metadata.range', {
              from: state.offset + 1,
              to: Math.min(state.offset + PAGE_SIZE, total),
              total,
            })}
        </p>
        <p className="text-xs text-muted-foreground">
          {t('metadata.selectUpTo', { max: MAX_COMPARE })}
        </p>
      </div>

      {query.isPending && <ResultsSkeleton />}

      {query.isError && (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/40 bg-destructive/5 p-4">
          <p className="text-sm text-destructive">{query.error.message}</p>
          <Button variant="outline" size="sm" onClick={() => query.refetch()}>
            {t('metadata.retry')}
          </Button>
        </div>
      )}

      {query.isSuccess && models.length === 0 && (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed p-10 text-center">
          <p className="text-sm text-muted-foreground">
            {hasFilters ? t('metadata.emptyFiltered') : t('metadata.empty')}
          </p>
          {hasFilters && (
            <Button variant="outline" size="sm" onClick={state.resetFilters}>
              {t('metadata.filters.clearAll')}
            </Button>
          )}
        </div>
      )}

      {models.length > 0 && (
        <div
          className={showing ? 'space-y-3 opacity-60 transition-opacity' : 'space-y-3 transition-opacity'}
          aria-busy={showing}
        >
          {models.map((model) => (
            <ModelGroup
              key={model.key}
              model={model}
              selectedIds={selectedIds}
              canSelectMore={state.canSelectMore}
              maxSpeed={maxSpeed}
              onToggleCompare={state.toggleCompare}
            />
          ))}
        </div>
      )}

      {models.length > 0 && (
        <CatalogPagination total={total} offset={state.offset} onChange={state.setOffset} />
      )}

      {/* Keeps the last rows reachable above the floating compare tray. */}
      {state.selection.length > 0 && <div className="h-20" aria-hidden />}

      <CompareTray
        items={state.selection}
        onRemove={state.removeCompare}
        onClear={state.clearCompare}
        onCompare={() => setCompareOpen(true)}
      />

      <Dialog open={compareOpen} onOpenChange={setCompareOpen}>
        <DialogPopup className="max-w-4xl">
          <DialogHeader>
            <DialogTitle>{t('metadata.compareTitle')}</DialogTitle>
            <DialogDescription>{t('metadata.compareNote')}</DialogDescription>
          </DialogHeader>
          <div className="mt-4 max-h-[70vh] overflow-auto">
            <Comparison items={state.selection} />
          </div>
        </DialogPopup>
      </Dialog>
    </div>
  )
}

function ResultsSkeleton() {
  return (
    <div className="space-y-3" aria-hidden>
      {[0, 1, 2].map((i) => (
        <div key={i} className="h-20 animate-pulse rounded-xl border bg-card" />
      ))}
    </div>
  )
}
