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
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { PageHeader } from '@/components/page-header'
import { ControlsPanel } from '@/components/fallback-controls'
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

function formatQuality(entry: FallbackEntry): string {
  const score = entry.quality?.score ?? entry.intelligenceScore
  const rank = entry.quality?.rank ?? entry.intelligenceRank
  if (score == null) return 'Quality —'
  return `Quality ${score.toFixed(1)}${rank == null ? '' : ` · #${rank}`}`
}

function formatSpeed(entry: FallbackEntry): string {
  const speed = entry.speed?.tokensPerSecond ?? entry.speedTokensPerSec
  const rank = entry.speed?.rank ?? entry.speedRank
  if (speed == null) return 'Speed —'
  return `Speed ${speed.toFixed(1)} tok/s${rank == null ? '' : ` · #${rank}`}`
}

function metricDetails(entry: FallbackEntry): string {
  const quality = entry.quality
  const speed = entry.speed
  const reliability = entry.reliability
  const parts = [
    quality?.source && `Quality: ${quality.source}`,
    quality?.confidence != null && quality.confidence > 0 && `Quality confidence: ${(quality.confidence * 100).toFixed(0)}%`,
    quality?.updatedAt && `Quality updated: ${quality.updatedAt}`,
    quality?.status === 'stale' && 'Quality: stale',
    speed?.source && `Speed: ${speed.source}`,
    speed?.confidence != null && speed.confidence > 0 && `Speed confidence: ${(speed.confidence * 100).toFixed(0)}%`,
    speed?.updatedAt && `Speed updated: ${speed.updatedAt}`,
    speed?.status === 'stale' && 'Speed: stale',
    reliability?.successRate != null && `Reliability: ${(reliability.successRate * 100).toFixed(1)}%`,
    reliability?.sampleCount ? `Samples: ${reliability.sampleCount}` : undefined,
  ].filter(Boolean)
  return parts.join(' · ') || 'No ranking data available'
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
          <span className="text-foreground font-medium">{formatTokens(remaining)}</span> {t('fallback.tokenBudget.remaining').replace('{{remaining}}', formatTokens(remaining)).replace('remaining', '')} {remainingPct}% {t('fallback.tokenBudget.of').split('{{remainingPct}}% of {{total}}')[0]} {formatTokens(totalBudget)}
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

function SortableModelRow({
  entry,
  index,
  onToggle,
}: {
  entry: FallbackEntry
  index: number
  onToggle: (modelDbId: number, enabled: boolean) => void
}) {
  const { t } = useTranslations()
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: entry.modelDbId,
  })

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  }

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`group flex items-center gap-3 px-4 py-3 bg-card ${isDragging ? 'opacity-50' : ''} ${entry.enabled ? '' : 'opacity-50'}`}
    >
      <button
        {...attributes}
        {...listeners}
        className="cursor-grab active:cursor-grabbing text-muted-foreground/50 hover:text-foreground transition-colors"
        aria-label={t('fallback.dragToReorder')}
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="9" cy="6" r="1.5" /><circle cx="15" cy="6" r="1.5" />
          <circle cx="9" cy="12" r="1.5" /><circle cx="15" cy="12" r="1.5" />
          <circle cx="9" cy="18" r="1.5" /><circle cx="15" cy="18" r="1.5" />
        </svg>
      </button>
      <span className="text-xs font-mono text-muted-foreground w-5 tabular-nums">{index + 1}</span>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="font-medium text-sm">{entry.displayName}</span>
          <span className="text-xs text-muted-foreground">{entry.platform}</span>
          {entry.penalty > 0 && (
            <span className="text-xs text-amber-600 dark:text-amber-400">
              {t('fallback.penalty', { count: entry.penalty })}
            </span>
          )}
        </div>
        <div className="flex gap-3 mt-0.5 text-xs text-muted-foreground tabular-nums">
          <span title={metricDetails(entry)}>{formatQuality(entry)}</span>
          <span title={metricDetails(entry)}>{formatSpeed(entry)}</span>
          {entry.reliability?.successRate != null && (
            <span title={metricDetails(entry)}>
              {t('fallback.reliability', { rate: (entry.reliability.successRate * 100).toFixed(1) })}
            </span>
          )}
          {entry.rpmLimit && <span>{t('fallback.rpm', { count: entry.rpmLimit })}</span>}
          {entry.rpdLimit && <span>{t('fallback.rpd', { count: entry.rpdLimit })}</span>}
          <span>{t('fallback.tokensPerMonth', { count: entry.monthlyTokenBudget })}</span>
        </div>
      </div>
      <Switch
        checked={entry.enabled}
        onCheckedChange={(checked) => onToggle(entry.modelDbId, checked)}
      />
    </div>
  )
}

export default function FallbackPage() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [localEntries, setLocalEntries] = useState<FallbackEntry[] | null>(null)
  const [filters, setFilters] = useState<FallbackFilters>(EMPTY_FILTERS)
  const [flash, setFlash] = useState<{ tone: 'success' | 'error'; message: string } | null>(null)

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
  const configuredEntries = allEntries.filter(e => e.keyCount > 0)
  const displayEntries = filterEntries(configuredEntries, filters)
  const unconfiguredPlatforms = [...new Set(allEntries.filter(e => e.keyCount === 0).map(e => e.platform))]

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
          <Button
            variant="outline"
            size="sm"
            onClick={() => syncMutation.mutate()}
            disabled={syncMutation.isPending}
          >
            {syncMutation.isPending ? t('fallback.syncing') : t('fallback.sync')}
          </Button>
        }
      />

      <div className="space-y-6">
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

        {tokenUsage && tokenUsage.totalBudget > 0 && (
          <TokenUsageBar data={tokenUsage} />
        )}

        {isLoading ? (
          <p className="text-sm text-muted-foreground">{t('fallback.loading')}</p>
        ) : configuredEntries.length === 0 ? (
          <div className="rounded-lg border border-dashed p-8 text-center">
            <p className="text-sm text-muted-foreground">
              {t('fallback.emptyState')}
            </p>
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
                    <SortableModelRow
                      key={entry.modelDbId}
                      entry={entry}
                      index={index}
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

            {unconfiguredPlatforms.length > 0 && (
              <p className="text-xs text-muted-foreground">
                {t('fallback.hiddenPlatforms', { platforms: unconfiguredPlatforms.join(', ') })}
              </p>
            )}
          </>
        )}
      </div>
    </div>
  )
}
