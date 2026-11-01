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
import { TokenUsageBar, type TokenUsageData } from '@/components/fallback-token-usage'
import { reorderFallbackEntries } from '@/components/fallback-order'
import {
  EMPTY_FILTERS,
  filterEntries,
  type FallbackEntry,
  type FallbackFilters,
} from '@/components/fallback-filters'

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
    if (!over) return
    const merged = reorderFallbackEntries(allEntries, displayEntries, active.id, over.id)
    if (merged) setLocalEntries(merged)
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
