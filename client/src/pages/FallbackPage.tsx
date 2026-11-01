import { useState, useEffect, useRef, useCallback } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { PageHeader } from '@/components/page-header'
import { ControlsPanel } from '@/components/fallback-controls'
import { SummaryStats } from '@/components/summary-stats'
import { useProviderWizard } from '@/lib/provider-wizard-context'
import { hasQuality, hasSpeed } from '@/components/fallback-filters'
import { ChainPreview } from '@/components/fallback-chain-preview'
import { SortableFallbackRow } from '@/components/fallback-row'
import { speedOf } from '@/components/fallback-format'
import {
  EMPTY_FILTERS,
  filterEntries,
  type FallbackEntry,
  type FallbackFilters,
} from '@/components/fallback-filters'

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

interface TokenUsageData {
  totalBudget: number
  totalUsed: number
  models: { displayName: string; platform: string; budget: number }[]
}

const platformColors: Record<string, string> = {
  google:      '#4285f4',
  groq:        '#f55036',
  cerebras:    '#8b5cf6',
  sambanova:   '#14b8a6',
  nvidia:      '#76b900',
  mistral:     '#f59e0b',
  openrouter:  '#ec4899',
  github:      '#6e7b8b',
  cohere:      '#d946ef',
  cloudflare:  '#f38020',
  zhipu:       '#06b6d4',
  ollama:      '#000000',
  kilo:        '#7c3aed',
  pollinations: '#a855f7',
  llm7:        '#0ea5e9',
}

function TokenUsageBar({ data }: { data: TokenUsageData }) {
  const { t } = useTranslations()
  const { totalBudget, totalUsed, models } = data
  const remaining = Math.max(0, totalBudget - totalUsed)
  const remainingPct = totalBudget > 0 ? Math.round((remaining / totalBudget) * 100) : 0

  const modelsWithWidth = models.map(m => ({
    ...m,
    remainingTokens: totalBudget > 0 ? (m.budget / totalBudget) * remaining : 0,
    widthPct: totalBudget > 0 ? (m.budget / totalBudget) * (remaining / totalBudget) * 100 : 0,
  }))
  const usedPct = totalBudget > 0 ? (totalUsed / totalBudget) * 100 : 0

  return (
    <section className="rounded-lg border bg-card p-5">
      <div className="flex items-baseline justify-between mb-3">
        <h2 className="text-sm font-medium">{t('fallback.tokenBudget.title')}</h2>
        <span className="text-xs text-muted-foreground tabular-nums">
          <span className="text-foreground font-medium">{t('fallback.tokenBudget.remaining', { remaining: formatTokens(remaining) })}</span>
          {' · '}
          {t('fallback.tokenBudget.of', { remainingPct, total: formatTokens(totalBudget) })}
        </span>
      </div>

      <div className="flex h-2.5 rounded-full overflow-hidden bg-muted">
        {modelsWithWidth.map((m, i) => (
          <div
            key={i}
            title={`${m.displayName} (${m.platform}) — ${formatTokens(m.remainingTokens)} remaining`}
            style={{
              width: `${m.widthPct}%`,
              backgroundColor: platformColors[m.platform] ?? '#94a3b8',
            }}
          />
        ))}
        {totalUsed > 0 && (
          <div
            title={`Used — ${formatTokens(totalUsed)}`}
            className="bg-muted-foreground/30"
            style={{ width: `${usedPct}%` }}
          />
        )}
      </div>

      <div className="mt-4 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-x-5 gap-y-1.5 text-xs tabular-nums">
        {modelsWithWidth.map((m, i) => (
          <div key={i} className="flex items-center gap-2 min-w-0">
            <span
              className="size-2 rounded-sm flex-shrink-0"
              style={{ backgroundColor: platformColors[m.platform] ?? '#94a3b8' }}
            />
            <span className="truncate">{m.displayName}</span>
            <span className="flex-1" />
            <span className="font-mono text-muted-foreground">{formatTokens(m.remainingTokens)}</span>
          </div>
        ))}
      </div>
    </section>
  )
}

export default function FallbackPage() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [localEntries, setLocalEntries] = useState<FallbackEntry[] | null>(null)
  const [filters, setFilters] = useState<FallbackFilters>(EMPTY_FILTERS)
  const [flash, setFlash] = useState<{ tone: 'success' | 'error'; message: string } | null>(null)
  const [showUnconfigured, setShowUnconfigured] = useState(false)
  const wizard = useProviderWizard()

  function showFlash(tone: 'success' | 'error', message: string) {
    setFlash({ tone, message })
    window.setTimeout(() => setFlash((f) => (f?.message === message ? null : f)), 5000)
  }

  const { data: entries = [], isLoading } = useQuery<FallbackEntry[]>({
    queryKey: ['fallback'],
    queryFn: () => apiFetch('/api/fallback'),
  })

  const { data: tokenUsage } = useQuery<TokenUsageData>({
    queryKey: ['fallback', 'token-usage'],
    queryFn: () => apiFetch('/api/fallback/token-usage'),
  })

  const saveMutation = useMutation({
    mutationFn: (data: { modelDbId: number; priority: number; enabled: boolean }[]) =>
      apiFetch('/api/fallback', { method: 'PUT', body: JSON.stringify(data) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['fallback'] })
      setLocalEntries(null)
    },
  })

  function handleSortedLocally() {
    setLocalEntries(null)
  }

  const syncMutation = useMutation({
    mutationFn: () => apiFetch<{ added?: number; updated?: number; disabled?: number }>('/api/models/sync', { method: 'POST' }),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['fallback'] })
      queryClient.invalidateQueries({ queryKey: ['fallback', 'token-usage'] })
      const parts = []
      if (data.added) parts.push(`+${data.added} added`)
      if (data.updated) parts.push(`${data.updated} updated`)
      if (data.disabled) parts.push(`−${data.disabled} removed`)
      if (parts.length === 0) parts.push(t('fallback.noChanges'))
      console.log(`[Model sync] ${parts.join(', ')}`)
    },
  })

  const allEntries = localEntries ?? entries
  // Only providers with an enabled key take part in the chain by default.
  const configuredEntries = allEntries.filter(e => e.keyCount > 0)
  const unconfiguredEntries = allEntries.filter(e => e.keyCount === 0)
  const displayEntries = filterEntries(
    showUnconfigured ? allEntries : configuredEntries,
    filters,
  )
  // Speed bars are scaled against the fastest visible entry so they compare.
  const maxSpeed = Math.max(0, ...displayEntries.map(e => speedOf(e) ?? 0))

  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event
    if (!over || active.id === over.id) return
    const oldIndex = displayEntries.findIndex(e => e.modelDbId === active.id)
    const newIndex = displayEntries.findIndex(e => e.modelDbId === over.id)
    const reorderedVisible = arrayMove(displayEntries, oldIndex, newIndex)
    let visibleIndex = 0
    const reorderedIds = new Set(displayEntries.map(e => e.modelDbId))
    const merged = allEntries.map(e => {
      if (!reorderedIds.has(e.modelDbId)) return e
      return reorderedVisible[visibleIndex++]
    }).map((e, i) => ({ ...e, priority: i + 1 }))
    setLocalEntries(merged)
  }

  function handleToggle(modelDbId: number, enabled: boolean) {
    const updated = allEntries.map(e =>
      e.modelDbId === modelDbId ? { ...e, enabled } : e
    )
    setLocalEntries(updated)
  }

  const handleSave = useCallback(() => {
    if (!localEntries) return
    saveMutation.mutate(
      allEntries.map(e => ({
        modelDbId: e.modelDbId,
        priority: e.priority,
        enabled: e.enabled,
      }))
    )
  }, [localEntries, allEntries, saveMutation])

  const handleSaveRef = useRef(handleSave)
  useEffect(() => {
    handleSaveRef.current = handleSave
  })

  // Autosave: debounce 800ms after drag reorder or toggle
  const autosaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => {
    if (!localEntries) return
    if (autosaveTimer.current) clearTimeout(autosaveTimer.current)
    autosaveTimer.current = setTimeout(() => {
      handleSaveRef.current()
    }, 800)
    return () => {
      if (autosaveTimer.current) clearTimeout(autosaveTimer.current)
    }
  }, [localEntries])

  const hasChanges = localEntries !== null

  return (
    <div>
      <PageHeader
        title={t('fallback.title')}
        description={t('fallback.description')}
        actions={
          <>
            <Button size="sm" onClick={() => wizard.open()}>
              {t('fallback.configureProviders')}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => syncMutation.mutate()}
              disabled={syncMutation.isPending}
            >
              {syncMutation.isPending ? t('fallback.syncing') : t('fallback.sync')}
            </Button>
          </>
        }
      />

      <div className="space-y-6">
        <SummaryStats
          items={[
            {
              label: t('fallback.stats.inChain'),
              value: configuredEntries.length,
              detail: t('fallback.stats.freeCount', { count: configuredEntries.filter(e => e.freeTier).length }),
            },
            {
              label: t('fallback.stats.enabled'),
              value: configuredEntries.filter(e => e.enabled).length,
              detail: `${configuredEntries.length ? Math.round((configuredEntries.filter(e => e.enabled).length / configuredEntries.length) * 100) : 0}%`,
              ratio: configuredEntries.length ? configuredEntries.filter(e => e.enabled).length / configuredEntries.length : 0,
            },
            {
              label: t('fallback.stats.qualityScored'),
              value: configuredEntries.filter(hasQuality).length,
              detail: `${configuredEntries.length ? Math.round((configuredEntries.filter(hasQuality).length / configuredEntries.length) * 100) : 0}%`,
              ratio: configuredEntries.length ? configuredEntries.filter(hasQuality).length / configuredEntries.length : 0,
              tone: 'quality',
            },
            {
              label: t('fallback.stats.speedMeasured'),
              value: configuredEntries.filter(hasSpeed).length,
              detail: `${configuredEntries.length ? Math.round((configuredEntries.filter(hasSpeed).length / configuredEntries.length) * 100) : 0}%`,
              ratio: configuredEntries.length ? configuredEntries.filter(hasSpeed).length / configuredEntries.length : 0,
              tone: 'speed',
            },
          ]}
        />

        <ChainPreview entries={configuredEntries} />

        {flash && (
          <div
            className={
              flash.tone === 'success'
                ? 'rounded-md border border-emerald-300 dark:border-emerald-700 bg-emerald-50 dark:bg-emerald-950/30 px-3 py-2 text-sm text-emerald-900 dark:text-emerald-100'
                : 'rounded-md border border-red-300 dark:border-red-700 bg-red-50 dark:bg-red-950/30 px-3 py-2 text-sm text-red-900 dark:text-red-100'
            }
          >
            {flash.message}
          </div>
        )}

        <ControlsPanel
          entries={configuredEntries}
          filters={filters}
          onFiltersChange={setFilters}
          onSortedLocally={handleSortedLocally}
          onFlash={showFlash}
        />

        {unconfiguredEntries.length > 0 && (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-dashed px-4 py-3">
            <p className="text-sm text-muted-foreground">
              {t('fallback.unconfiguredNote', {
                count: unconfiguredEntries.length,
                platforms: [...new Set(unconfiguredEntries.map(e => e.platform))].join(', '),
              })}
            </p>
            <Button variant="outline" size="sm" onClick={() => setShowUnconfigured(v => !v)}>
              {showUnconfigured
                ? t('fallback.hideUnconfigured')
                : t('fallback.showUnconfigured', { count: unconfiguredEntries.length })}
            </Button>
          </div>
        )}

        {tokenUsage && tokenUsage.totalBudget > 0 && (
          <TokenUsageBar data={tokenUsage} />
        )}

        {isLoading ? (
          <p className="text-sm text-muted-foreground">{t('fallback.loading')}</p>
        ) : configuredEntries.length === 0 && !showUnconfigured ? (
          <div className="space-y-4 rounded-lg border border-dashed p-8 text-center">
            <p className="text-sm text-muted-foreground">
              {t('fallback.emptyState')}
            </p>
            <Button onClick={() => wizard.open()}>{t('fallback.configureProviders')}</Button>
          </div>
        ) : displayEntries.length === 0 ? (
          <div className="rounded-lg border border-dashed p-8 text-center">
            <p className="text-sm text-muted-foreground">
              {t('fallback.filters.empty')}
            </p>
          </div>
        ) : (
          <>
            <div className="rounded-lg border divide-y overflow-hidden">
              <DndContext
                sensors={sensors}
                collisionDetection={closestCenter}
                onDragEnd={handleDragEnd}
              >
                <SortableContext
                  items={displayEntries.map(e => e.modelDbId)}
                  strategy={verticalListSortingStrategy}
                >
                  {displayEntries.map((entry, index) => (
                    <SortableFallbackRow
                      key={entry.modelDbId}
                      entry={entry}
                      index={index}
                      maxSpeed={maxSpeed}
                      onToggle={handleToggle}
                    />
                  ))}
                </SortableContext>
              </DndContext>
            </div>

            {hasChanges && (
              <div className="flex justify-end gap-2">
                <Button variant="outline" size="sm" onClick={() => setLocalEntries(null)}>
                  {t('fallback.discard')}
                </Button>
                <Button size="sm" onClick={handleSave} disabled={saveMutation.isPending}>
                  {saveMutation.isPending ? t('fallback.saving') : t('fallback.saveOrder')}
                </Button>
              </div>
            )}


          </>
        )}
      </div>
    </div>
  )
}
