import { useState, useMemo, useId } from 'react'
import { useTranslation } from 'react-i18next'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Search, X, ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { apiFetch } from '@/lib/api'
import type { FallbackEntry, FallbackFilters, SortPreset, TierFilter, StateFilter, RankingFilter } from './fallback-filters'
import { EMPTY_FILTERS, isRanked } from './fallback-filters'

type BulkActionKey = 'enableAll' | 'disableAll' | 'enableFree' | null

interface BulkActionResult {
  modelsUpdated?: number
  fallbackEntriesUpdated?: number
  freeEnabled?: number
  paidDisabled?: number
  fallbackFreeEnabled?: number
  fallbackPaidDisabled?: number
}

interface ControlsPanelProps {
  entries: FallbackEntry[]
  filters: FallbackFilters
  onFiltersChange: (next: FallbackFilters) => void
  onSortedLocally: () => void
  onFlash: (tone: 'success' | 'error', message: string) => void
}

export function ControlsPanel({
  entries,
  filters,
  onFiltersChange,
  onSortedLocally,
  onFlash,
}: ControlsPanelProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [pendingConfirm, setPendingConfirm] = useState<BulkActionKey>(null)
  const [sortMenuOpen, setSortMenuOpen] = useState(false)
  const baseId = useId()

  const platforms = useMemo(() => {
    const set = new Set<string>()
    for (const e of entries) set.add(e.platform)
    return [...set].sort()
  }, [entries])

  const counts = useMemo(() => {
    return {
      total: entries.length,
      enabled: entries.filter((e) => e.enabled).length,
      free: entries.filter((e) => e.freeTier).length,
      ranked: entries.filter((e) => isRanked(e)).length,
    }
  }, [entries])

  const hasActiveFilters =
    filters.search !== '' ||
    filters.platform !== '' ||
    filters.tier !== 'all' ||
    filters.state !== 'all' ||
    filters.ranking !== 'all'

  function patch(next: Partial<FallbackFilters>) {
    onFiltersChange({ ...filters, ...next })
  }

  function clearFilters() {
    onFiltersChange(EMPTY_FILTERS)
  }

  const sortMutation = useMutation({
    mutationFn: (preset: SortPreset) =>
      apiFetch(`/api/fallback/sort/${preset}`, { method: 'POST' }),
    onSuccess: () => {
      void queryClient.refetchQueries({ queryKey: ['fallback'] })
      onSortedLocally()
    },
  })

  const bulkMutation = useMutation({
    mutationFn: async (key: Exclude<BulkActionKey, null>) => {
      const path =
        key === 'enableAll'
          ? '/api/models/enable-all'
          : key === 'disableAll'
            ? '/api/models/disable-all'
            : '/api/models/enable-free'
      return apiFetch<BulkActionResult>(path, {
        method: 'POST',
        body: JSON.stringify({ confirm: true }),
      })
    },
    onSuccess: async (data, key) => {
      setPendingConfirm(null)
      await queryClient.invalidateQueries({ queryKey: ['fallback'] })
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      const message = buildResultMessage(t, key, data)
      onFlash('success', message)
    },
    onError: (err: unknown) => {
      const msg = err instanceof Error ? err.message : String(err)
      onFlash('error', t('fallback.bulk.error', { message: msg }))
    },
  })

  const hasFreeModels = counts.free > 0
  const isBusy = bulkMutation.isPending || sortMutation.isPending

  const confirmTitle = pendingConfirm
    ? t(`fallback.bulk.confirm.title.${pendingConfirm}`)
    : ''
  const confirmDescription = pendingConfirm
    ? t(`fallback.bulk.confirm.${pendingConfirm}`)
    : ''

  return (
    <section className="rounded-lg border bg-card p-4 space-y-4">
      <div className="flex flex-wrap items-end gap-3">
        <div className="flex-1 min-w-48 space-y-1.5">
          <Label htmlFor={`${baseId}-search`} className="text-xs text-muted-foreground">
            {t('fallback.filters.search')}
          </Label>
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-muted-foreground pointer-events-none" />
            <Input
              id={`${baseId}-search`}
              value={filters.search}
              onChange={(e) => patch({ search: e.target.value })}
              placeholder={t('fallback.filters.search')}
              className="pl-8"
            />
          </div>
        </div>

        <div className="space-y-1.5 min-w-40">
          <Label className="text-xs text-muted-foreground">{t('fallback.filters.platform')}</Label>
          <Select
            value={filters.platform || '__all__'}
            onValueChange={(v) => patch({ platform: v && v !== '__all__' ? v : '' })}
          >
            <SelectTrigger className="w-full">
              <SelectValue placeholder={t('fallback.filters.platformAll')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">{t('fallback.filters.platformAll')}</SelectItem>
              {platforms.map((p) => (
                <SelectItem key={p} value={p}>
                  {p}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5 min-w-32">
          <Label className="text-xs text-muted-foreground">{t('fallback.filters.tier')}</Label>
          <Select
            value={filters.tier}
            onValueChange={(v) => patch({ tier: v as TierFilter })}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('fallback.filters.tierAll')}</SelectItem>
              <SelectItem value="free">{t('fallback.filters.tierFree')}</SelectItem>
              <SelectItem value="paid">{t('fallback.filters.tierPaid')}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5 min-w-32">
          <Label className="text-xs text-muted-foreground">{t('fallback.filters.state')}</Label>
          <Select
            value={filters.state}
            onValueChange={(v) => patch({ state: v as StateFilter })}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('fallback.filters.stateAll')}</SelectItem>
              <SelectItem value="enabled">{t('fallback.filters.stateEnabled')}</SelectItem>
              <SelectItem value="disabled">{t('fallback.filters.stateDisabled')}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="space-y-1.5 min-w-32">
          <Label className="text-xs text-muted-foreground">{t('fallback.filters.ranking')}</Label>
          <Select
            value={filters.ranking}
            onValueChange={(v) => patch({ ranking: v as RankingFilter })}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t('fallback.filters.rankingAll')}</SelectItem>
              <SelectItem value="ranked">{t('fallback.filters.rankingRanked')}</SelectItem>
              <SelectItem value="unranked">{t('fallback.filters.rankingUnranked')}</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {hasActiveFilters && (
          <Button variant="ghost" size="sm" onClick={clearFilters} className="gap-1">
            <X className="size-3.5" />
            {t('fallback.filters.clear')}
          </Button>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 pt-3 border-t">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground tabular-nums">
          <span className="px-2 py-0.5 rounded-md bg-muted text-foreground">
            {t('fallback.summary.total', { count: counts.total })}
          </span>
          <span>{t('fallback.summary.enabled', { count: counts.enabled })}</span>
          {hasFreeModels && <span>{t('fallback.summary.free', { count: counts.free })}</span>}
          <span>{t('fallback.summary.ranked', { count: counts.ranked })}</span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="relative">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setSortMenuOpen((o) => !o)}
              disabled={isBusy}
              className="gap-1"
            >
              {t('fallback.bulk.sort.label')}
              <ChevronDown className="size-3.5" />
            </Button>
            {sortMenuOpen && (
              <>
                <div
                  className="fixed inset-0 z-10"
                  onClick={() => setSortMenuOpen(false)}
                />
                <div className="absolute right-0 mt-1 z-20 min-w-44 rounded-md border bg-popover shadow-md p-1 text-sm">
                  {(['intelligence', 'speed', 'budget'] as const).map((preset) => (
                    <button
                      key={preset}
                      className="w-full text-left px-2 py-1.5 rounded hover:bg-accent hover:text-accent-foreground flex items-center justify-between"
                      onClick={() => {
                        setSortMenuOpen(false)
                        sortMutation.mutate(preset)
                      }}
                    >
                      {t(`fallback.bulk.sort.${preset}`)}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>

          <Button
            variant="outline"
            size="sm"
            onClick={() => setPendingConfirm('enableAll')}
            disabled={isBusy}
          >
            {t('fallback.bulk.enableAll')}
          </Button>

          <Button
            variant="outline"
            size="sm"
            onClick={() => setPendingConfirm('disableAll')}
            disabled={isBusy}
          >
            {t('fallback.bulk.disableAll')}
          </Button>

          {hasFreeModels && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPendingConfirm('enableFree')}
              disabled={isBusy}
            >
              {t('fallback.bulk.enableFree')}
            </Button>
          )}
        </div>
      </div>

      {pendingConfirm && (
        <ConfirmDialog
          open={!!pendingConfirm}
          onOpenChange={(open) => {
            if (!open && !bulkMutation.isPending) setPendingConfirm(null)
          }}
          title={confirmTitle}
          description={confirmDescription}
          confirmLabel={t('fallback.bulk.proceed')}
          cancelLabel={t('fallback.bulk.cancel')}
          variant={pendingConfirm === 'disableAll' ? 'danger' : 'default'}
          busy={bulkMutation.isPending}
          onConfirm={() => bulkMutation.mutate(pendingConfirm)}
        />
      )}
    </section>
  )
}

function buildResultMessage(
  t: (key: string, opts?: Record<string, unknown>) => string,
  key: Exclude<BulkActionKey, null>,
  data: BulkActionResult | undefined,
): string {
  if (!data) return ''
  if (key === 'enableAll' || key === 'disableAll') {
    const count = data.modelsUpdated ?? 0
    return t(`fallback.bulk.result.${key}`, {
      count,
      models: count,
      fallback: data.fallbackEntriesUpdated ?? 0,
    })
  }
  return t('fallback.bulk.result.enableFree', {
    count: 1,
    free: data.freeEnabled ?? 0,
    paid: data.paidDisabled ?? 0,
  })
}
