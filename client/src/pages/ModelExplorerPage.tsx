import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { PageHeader } from '@/components/page-header'
import { SummaryStats } from '@/components/summary-stats'
import { ModelCard } from '@/components/model-card'
import { ModelFilters } from '@/components/model-filters'
import { DEFAULT_CATALOG_FILTERS, type CatalogFilters } from '@/components/catalog-filter-state'
import { summarize } from '@/components/catalog-metrics'
import { LazyModelDetail } from '@/components/model-explorer/ModelDetail'
import { Comparison } from '@/components/model-explorer/Comparison'
import { SourceControls } from '@/components/model-explorer/SourceControls'
import { MatchReview } from '@/components/model-explorer/MatchReview'
import { Simulator } from '@/components/model-explorer/Simulator'
import type { Page } from '@/components/model-explorer/types'

const PAGE_SIZE = 25
const MAX_COMPARE = 4

export default function ModelExplorerPage() {
  const { t } = useTranslations()
  const [tab, setTab] = useState('explorer')
  const [filters, setFilters] = useState<CatalogFilters>(DEFAULT_CATALOG_FILTERS)
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<number[]>([])
  const params = new URLSearchParams({
    query: filters.query,
    provider: filters.provider,
    capability: filters.capability,
    sort: filters.sort,
    limit: String(PAGE_SIZE),
    offset: String(offset),
    ...(filters.free ? { free: 'true' } : {}),
    ...(filters.context ? { minContext: filters.context } : {}),
  })
  const models = useQuery<Page>({
    queryKey: ['metadata-explorer', params.toString()],
    queryFn: () => apiFetch(`/api/metadata/explorer?${params}`),
    enabled: tab === 'explorer',
  })

  function updateFilters(patch: Partial<CatalogFilters>) {
    setFilters((current) => ({ ...current, ...patch }))
    setOffset(0)
  }

  function toggleSelected(id: number, checked: boolean) {
    setSelected((current) =>
      checked ? [...current, id] : current.filter((value) => value !== id),
    )
  }

  const pageModels = models.data?.data ?? []
  const deployments = pageModels.flatMap((m) => m.deployments)
  const summaries = deployments.map(summarize)
  const maxSpeed = Math.max(0, ...summaries.map((s) => s.speed ?? 0))
  const total = models.data?.total ?? 0
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const page = Math.floor(offset / PAGE_SIZE) + 1
  const providers = new Set(deployments.map((d) => d.provider)).size
  const freeCount = deployments.filter((d) => d.freeTier).length
  const qualityCount = summaries.filter((s) => s.quality != null).length
  const qualityShare = deployments.length > 0 ? qualityCount / deployments.length : 0

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('metadata.title')}
        description={t('metadata.description')}
        actions={
          <nav aria-label={t('metadata.title')} className="inline-flex flex-wrap gap-0.5 rounded-md border bg-muted/40 p-0.5">
            {['explorer', 'sources', 'matches', 'simulator'].map((s) => (
              <button
                key={s}
                type="button"
                aria-pressed={tab === s}
                onClick={() => setTab(s)}
                className={cn(
                  'rounded px-3 py-1.5 text-sm transition-colors',
                  tab === s ? 'bg-background font-medium shadow-sm' : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {t(`metadata.${s}`)}
              </button>
            ))}
          </nav>
        }
      />

      {tab === 'sources' && <SourceControls />}
      {tab === 'matches' && <MatchReview />}
      {tab === 'simulator' && <Simulator />}
      {tab === 'explorer' && (
        <>
          <SummaryStats
            items={[
              { label: t('metadata.stats.models'), value: total, detail: t('metadata.stats.onPage', { count: pageModels.length }) },
              { label: t('metadata.stats.deployments'), value: deployments.length, detail: t('metadata.stats.free', { count: freeCount }) },
              { label: t('metadata.stats.providers'), value: providers, detail: t('metadata.stats.onPage', { count: pageModels.length }) },
              { label: t('metadata.stats.withQuality'), value: qualityCount, detail: `${Math.round(qualityShare * 100)}%`, ratio: qualityShare, tone: 'quality' },
            ]}
          />

          <ModelFilters
            value={filters}
            onChange={updateFilters}
            onClear={() => updateFilters(DEFAULT_CATALOG_FILTERS)}
          />

          {selected.length > 0 && (
            <section className="space-y-3 rounded-lg border bg-card p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className="text-sm font-medium">
                  {t('metadata.stats.comparing', { count: selected.length, max: MAX_COMPARE })}
                </h2>
                <Button variant="ghost" size="sm" onClick={() => setSelected([])}>
                  {t('metadata.stats.clearComparison')}
                </Button>
              </div>
              <Comparison ids={selected} />
            </section>
          )}

          {models.isLoading && <p className="text-sm text-muted-foreground">{t('metadata.loading')}</p>}
          {models.error && <p role="alert" className="text-sm text-destructive">{models.error.message}</p>}
          {!models.isLoading && !models.error && pageModels.length === 0 && (
            <div className="rounded-lg border border-dashed p-8 text-center">
              <p className="text-sm text-muted-foreground">{t('metadata.empty')}</p>
            </div>
          )}

          <div className="grid gap-3">
            {pageModels.map((m) => (
              <ModelCard
                key={m.key}
                model={m}
                selected={selected}
                limitReached={selected.length >= MAX_COMPARE}
                maxSpeed={maxSpeed}
                onSelect={toggleSelected}
                renderDetails={(id) => <LazyModelDetail id={id} />}
              />
            ))}
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
            <p className="text-xs tabular-nums text-muted-foreground">
              {t('metadata.range', {
                from: total === 0 ? 0 : offset + 1,
                to: Math.min(offset + PAGE_SIZE, total),
                total,
              })}
            </p>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={!offset}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                {t('metadata.previous')}
              </Button>
              <span className="text-xs tabular-nums">{t('metadata.page', { page, pages })}</span>
              <Button
                variant="outline"
                size="sm"
                disabled={offset + PAGE_SIZE >= total}
                onClick={() => setOffset(offset + PAGE_SIZE)}
              >
                {t('metadata.next')}
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  )
}
