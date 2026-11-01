import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { UniqueIdentifier } from '@dnd-kit/core'
import type { FallbackEntry } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch } from '@/lib/api'
import type { FlashTone } from './use-flash'
import {
  EMPTY_FILTERS,
  bulkActionPath,
  buildBulkResultMessage,
  filterEntries,
  reorderEntries,
  speedOf,
  toSavePayload,
  type BulkAction,
  type BulkActionResult,
  type FallbackFilters,
  type SortPreset,
  type TokenUsageData,
} from './chain-model'

/** Edits are saved this long after the last change, so a drag is one request. */
const AUTOSAVE_MS = 800

const NO_ENTRIES: FallbackEntry[] = []

export type SaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error'

interface SyncResult {
  added?: number
  updated?: number
  disabled?: number
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * The fallback chain's server state, its unsaved local order, and autosave.
 * Every server call lives here so the components only render and dispatch.
 */
export function useFallbackChain(notify: (tone: FlashTone, message: string) => void) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  // Local order held until the autosave lands. Null means "server state".
  const [local, setLocal] = useState<FallbackEntry[] | null>(null)
  const [filters, setFilters] = useState<FallbackFilters>(EMPTY_FILTERS)
  const [showUnconfigured, setShowUnconfigured] = useState(false)

  const entriesQuery = useQuery<FallbackEntry[]>({
    queryKey: ['fallback'],
    queryFn: () => apiFetch('/api/fallback'),
  })
  const usageQuery = useQuery<TokenUsageData>({
    queryKey: ['fallback', 'token-usage'],
    queryFn: () => apiFetch('/api/fallback/token-usage'),
  })

  const all = local ?? entriesQuery.data ?? NO_ENTRIES
  // Providers without a key cannot serve requests, so they stay hidden by default.
  const configured = useMemo(() => all.filter((e) => e.keyCount > 0), [all])
  const unconfigured = useMemo(() => all.filter((e) => e.keyCount === 0), [all])
  const visible = useMemo(
    () => filterEntries(showUnconfigured ? all : configured, filters),
    [all, configured, showUnconfigured, filters],
  )
  // Speed bars are scaled against the fastest visible entry so they compare.
  const maxSpeed = useMemo(() => Math.max(0, ...visible.map((e) => speedOf(e) ?? 0)), [visible])

  const save = useMutation({
    mutationFn: (snapshot: FallbackEntry[]) =>
      apiFetch('/api/fallback', { method: 'PUT', body: JSON.stringify(toSavePayload(snapshot)) }),
    onSuccess: (_data, snapshot) => {
      queryClient.setQueryData(['fallback'], snapshot)
      // Clear the local order only if nothing changed while this request was in flight.
      setLocal((current) => (current === snapshot ? null : current))
      void queryClient.invalidateQueries({ queryKey: ['fallback'] })
    },
    onError: (error) => notify('error', t('fallback.save.error', { message: messageOf(error) })),
  })

  const { mutate: saveNow } = save
  useEffect(() => {
    if (!local) return
    const id = window.setTimeout(() => saveNow(local), AUTOSAVE_MS)
    return () => window.clearTimeout(id)
  }, [local, saveNow])

  const saveStatus: SaveStatus =
    save.isError && local
      ? 'error'
      : save.isPending
        ? 'saving'
        : local
          ? 'pending'
          : save.isSuccess
            ? 'saved'
            : 'idle'

  const sort = useMutation({
    mutationFn: (preset: SortPreset) => apiFetch(`/api/fallback/sort/${preset}`, { method: 'POST' }),
    onSuccess: () => {
      setLocal(null)
      void queryClient.invalidateQueries({ queryKey: ['fallback'] })
    },
    onError: (error) => notify('error', t('fallback.sort.error', { message: messageOf(error) })),
  })

  const bulk = useMutation({
    mutationFn: (action: BulkAction) =>
      apiFetch<BulkActionResult>(bulkActionPath(action), {
        method: 'POST',
        body: JSON.stringify({ confirm: true }),
      }),
    onSuccess: async (data, action) => {
      setLocal(null)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['fallback'] }),
        queryClient.invalidateQueries({ queryKey: ['models'] }),
      ])
      notify('success', buildBulkResultMessage(t, action, data))
    },
    onError: (error) => notify('error', t('fallback.bulk.error', { message: messageOf(error) })),
  })

  const sync = useMutation({
    mutationFn: () => apiFetch<SyncResult>('/api/models/sync', { method: 'POST' }),
    onSuccess: (data) => {
      setLocal(null)
      void queryClient.invalidateQueries({ queryKey: ['fallback'] })
      void queryClient.invalidateQueries({ queryKey: ['fallback', 'token-usage'] })
      const parts: string[] = []
      if (data.added) parts.push(t('fallback.syncResult.added', { count: data.added }))
      if (data.updated) parts.push(t('fallback.syncResult.updated', { count: data.updated }))
      if (data.disabled) parts.push(t('fallback.syncResult.removed', { count: data.disabled }))
      notify(
        'success',
        parts.length > 0 ? t('fallback.syncResult.result', { changes: parts.join(', ') }) : t('fallback.syncResult.none'),
      )
    },
    onError: (error) => notify('error', t('fallback.syncResult.error', { message: messageOf(error) })),
  })

  function reorder(activeId: UniqueIdentifier, overId: UniqueIdentifier) {
    const merged = reorderEntries(all, visible, activeId, overId)
    if (merged) setLocal(merged)
  }

  function toggle(modelDbId: number, enabled: boolean) {
    setLocal(all.map((e) => (e.modelDbId === modelDbId ? { ...e, enabled } : e)))
  }

  return {
    isLoading: entriesQuery.isPending,
    all,
    configured,
    unconfigured,
    visible,
    maxSpeed,
    filters,
    setFilters,
    resetFilters: () => setFilters(EMPTY_FILTERS),
    showUnconfigured,
    toggleUnconfigured: () => setShowUnconfigured((value) => !value),
    tokenUsage: usageQuery.data,
    reorder,
    toggle,
    saveStatus,
    retrySave: () => {
      if (local) saveNow(local)
    },
    sort: (preset: SortPreset) => sort.mutate(preset),
    sorting: sort.isPending,
    runBulk: (action: BulkAction) => bulk.mutateAsync(action),
    bulkPending: bulk.isPending,
    sync: () => sync.mutate(),
    syncing: sync.isPending,
  }
}

export type FallbackChain = ReturnType<typeof useFallbackChain>
