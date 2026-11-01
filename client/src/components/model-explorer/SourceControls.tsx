import { RefreshCw } from 'lucide-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { MetadataSource } from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Section } from '@/components/section'
import { SourceCard } from '@/components/source-card'
import { SyncRunsTable, type SyncRun } from '@/components/sync-runs-table'
import { useProviderWizard } from '@/lib/provider-wizard-context'

export function SourceControls() {
  const { t } = useTranslations()
  const wizard = useProviderWizard()
  const cache = useQueryClient()
  const query = useQuery<MetadataSource[]>({
    queryKey: ['metadata-sources'],
    queryFn: () => apiFetch('/api/metadata/sources'),
    refetchInterval: 30_000,
  })
  const update = useMutation({
    mutationFn: ({ id, ...patch }: { id: string; enabled?: boolean; ttlSeconds?: number }) =>
      apiFetch(`/api/metadata/sources/${id}`, { method: 'PATCH', body: JSON.stringify(patch) }),
    onSuccess: () => cache.invalidateQueries({ queryKey: ['metadata-sources'] }),
  })
  const sync = useMutation({
    mutationFn: () => apiFetch('/api/metadata/sync', { method: 'POST' }),
    onSuccess: () =>
      cache.invalidateQueries({
        predicate: (q) => String(q.queryKey[0]).startsWith('metadata'),
      }),
  })
  const runs = useQuery<SyncRun[]>({
    queryKey: ['metadata-runs'],
    queryFn: () => apiFetch('/api/metadata/sync/status'),
  })
  const failure = query.error ?? update.error ?? sync.error
  return (
    <div className="space-y-10">
      <Section
        title={t('metadata.sources')}
        description={t('sources.description')}
        action={
          <Button variant="outline" size="sm" disabled={sync.isPending} onClick={() => sync.mutate()}>
            <RefreshCw className={sync.isPending ? 'size-4 animate-spin' : 'size-4'} aria-hidden />
            {t('metadata.refresh')}
          </Button>
        }
      >
        {failure && (
          <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
            {failure.message}
          </p>
        )}
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {query.data?.map((source) => (
            <SourceCard
              key={source.id}
              source={source}
              busy={update.isPending}
              onToggle={(enabled) => update.mutate({ id: source.id, enabled })}
              onInterval={(ttlSeconds) => update.mutate({ id: source.id, ttlSeconds })}
              onAddKey={
                source.id === 'artificial-analysis'
                  ? () => wizard.open({ step: 'benchmarks' })
                  : undefined
              }
            />
          ))}
        </div>
      </Section>

      <Section title={t('sources.runs.title')} description={t('sources.runs.description')}>
        <SyncRunsTable runs={runs.data ?? []} />
      </Section>
    </div>
  )
}
