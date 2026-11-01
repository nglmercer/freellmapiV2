import { useState, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { apiFetch } from '@/lib/api'
import type { FallbackEntry, FallbackFilters, SortPreset } from './fallback-filters'
import { hasLocalPerformance, hasQuality, hasSpeed } from './fallback-filters'
import { FallbackFilterBar } from './fallback-filter-bar'
import { FallbackSortMenu } from './fallback-sort-menu'
import {
  buildBulkResultMessage,
  bulkActionPath,
  type BulkAction,
  type BulkActionResult,
} from './fallback-bulk'

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
  const [pendingConfirm, setPendingConfirm] = useState<BulkAction | null>(null)

  const counts = useMemo(() => {
    return {
      total: entries.length,
      enabled: entries.filter((e) => e.enabled).length,
      free: entries.filter((e) => e.freeTier).length,
      qualityRanked: entries.filter((e) => hasQuality(e)).length,
      speedRanked: entries.filter((e) => hasSpeed(e)).length,
      locallyMeasured: entries.filter((e) => hasLocalPerformance(e)).length,
      unranked: entries.filter((e) => !hasQuality(e) && !hasSpeed(e)).length,
    }
  }, [entries])

  const sortMutation = useMutation({
    mutationFn: (preset: SortPreset) =>
      apiFetch(`/api/fallback/sort/${preset}`, { method: 'POST' }),
    onSuccess: () => {
      void queryClient.refetchQueries({ queryKey: ['fallback'] })
      onSortedLocally()
    },
  })

  const bulkMutation = useMutation({
    mutationFn: (action: BulkAction) =>
      apiFetch<BulkActionResult>(bulkActionPath(action), {
        method: 'POST',
        body: JSON.stringify({ confirm: true }),
      }),
    onSuccess: async (data, action) => {
      setPendingConfirm(null)
      await queryClient.invalidateQueries({ queryKey: ['fallback'] })
      await queryClient.invalidateQueries({ queryKey: ['models'] })
      onFlash('success', buildBulkResultMessage(t, action, data))
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
      <FallbackFilterBar entries={entries} filters={filters} onFiltersChange={onFiltersChange} />

      <div className="flex flex-wrap items-center justify-between gap-3 pt-3 border-t">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground tabular-nums">
          <span className="px-2 py-0.5 rounded-md bg-muted text-foreground">
            {t('fallback.summary.total', { count: counts.total })}
          </span>
          <span>{t('fallback.summary.enabled', { count: counts.enabled })}</span>
          {hasFreeModels && <span>{t('fallback.summary.free', { count: counts.free })}</span>}
          <span>{t('fallback.summary.qualityRanked', { count: counts.qualityRanked })}</span>
          <span>{t('fallback.summary.speedRanked', { count: counts.speedRanked })}</span>
          <span>{t('fallback.summary.localMeasured', { count: counts.locallyMeasured })}</span>
          <span>{t('fallback.summary.unranked', { count: counts.unranked })}</span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <FallbackSortMenu disabled={isBusy} onSelect={(preset) => sortMutation.mutate(preset)} />

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
