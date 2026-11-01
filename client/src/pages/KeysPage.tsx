import { useMemo } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { ApiKeysPanel } from '@/components/api-keys-panel'
import { JoinCodePanel } from '@/components/join-code-panel'
import { ProviderKeyGroups, type HealthKey } from '@/components/provider-keys'
import { Button } from '@/components/ui/button'
import { PageHeader } from '@/components/page-header'
import { Section } from '@/components/section'
import { SummaryStats } from '@/components/summary-stats'
import { useProviderWizard } from '@/lib/provider-wizard-context'
import { useTranslations } from '@/hooks/useTranslations'
import type { ApiKey } from '../../../shared/types'

interface HealthData {
  platforms: { platform: string }[]
  keys: HealthKey[]
}

const NEEDS_ATTENTION = new Set(['invalid', 'error', 'rate_limited'])

export default function KeysPage() {
  const { t } = useTranslations()
  const wizard = useProviderWizard()
  const queryClient = useQueryClient()

  const keys = useQuery<ApiKey[]>({
    queryKey: ['keys'],
    queryFn: () => apiFetch('/api/keys'),
  })

  const health = useQuery<HealthData>({
    queryKey: ['health'],
    queryFn: () => apiFetch('/api/health'),
    refetchInterval: 30000,
  })

  const apiKeys = useQuery<{ api_keys: { revoked: boolean }[] }>({
    queryKey: ['api-keys'],
    queryFn: () => apiFetch('/api/auth/api-key/list'),
  })
  const activeApiKeys = (apiKeys.data?.api_keys ?? []).filter((key) => !key.revoked).length

  const deleteKey = useMutation({
    mutationFn: (id: number) => apiFetch(`/api/keys/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['keys'] })
      queryClient.invalidateQueries({ queryKey: ['health'] })
      queryClient.invalidateQueries({ queryKey: ['fallback'] })
    },
  })

  const checkAll = useMutation({
    mutationFn: () => apiFetch('/api/health/check-all', { method: 'POST' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['health'] })
      queryClient.invalidateQueries({ queryKey: ['keys'] })
    },
  })

  const checkKey = useMutation({
    mutationFn: (keyId: number) => apiFetch(`/api/health/check/${keyId}`, { method: 'POST' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['health'] })
      queryClient.invalidateQueries({ queryKey: ['keys'] })
    },
  })

  const healthMap = useMemo(() => {
    const map = new Map<number, HealthKey>()
    for (const row of health.data?.keys ?? []) map.set(row.id, row)
    return map
  }, [health.data])

  const rows = keys.data ?? []
  const statusOf = (row: ApiKey) => healthMap.get(row.id)?.status ?? row.status
  const providers = new Set(rows.map((row) => row.platform)).size
  const healthy = rows.filter((row) => statusOf(row) === 'healthy').length
  const attention = rows.filter((row) => NEEDS_ATTENTION.has(statusOf(row))).length

  return (
    <div>
      <PageHeader
        title={t('keys.title')}
        description={t('keys.description')}
        actions={
          <>
            <Button size="sm" onClick={() => wizard.open()}>
              {t('keys.addProvider')}
            </Button>
            {rows.length > 0 && (
              <Button variant="outline" size="sm" onClick={() => checkAll.mutate()} disabled={checkAll.isPending}>
                {checkAll.isPending ? t('keys.checking') : t('keys.checkAll')}
              </Button>
            )}
          </>
        }
      />

      <div className="space-y-10">
        <SummaryStats
          items={[
            { label: t('keys.stats.providers'), value: providers, detail: t('keys.stats.keys', { count: rows.length }) },
            {
              label: t('keys.stats.healthy'),
              value: healthy,
              detail: rows.length ? `${Math.round((healthy / rows.length) * 100)}%` : '0%',
              ratio: rows.length ? healthy / rows.length : 0,
              tone: 'speed',
            },
            { label: t('keys.stats.attention'), value: attention, detail: t('keys.stats.attentionDetail') },
            { label: t('keys.stats.apiKeys'), value: activeApiKeys, detail: t('keys.stats.apiKeysDetail') },
          ]}
        />

        <Section
          title={t('keys.configuredProviders')}
          description={t('keys.providerKeysDescription')}
        >
          {keys.isLoading ? (
            <p className="text-sm text-muted-foreground">{t('keys.loading')}</p>
          ) : rows.length === 0 ? (
            <div className="space-y-4 rounded-lg border border-dashed p-8 text-center">
              <p className="text-sm text-muted-foreground">{t('keys.emptyState')}</p>
              <Button onClick={() => wizard.open()}>{t('keys.addProvider')}</Button>
            </div>
          ) : (
            <ProviderKeyGroups
              keys={rows}
              health={healthMap}
              busy={deleteKey.isPending || checkKey.isPending}
              onCheck={(id) => checkKey.mutate(id)}
              onRemove={(id) => deleteKey.mutate(id)}
            />
          )}
        </Section>

        <Section title={t('keys.apiKeys.title')} description={t('keys.apiKeys.description')}>
          <div className="rounded-lg border bg-card p-5">
            <ApiKeysPanel />
          </div>
        </Section>

        <Section title={t('keys.joinCode.title')} description={t('keys.joinCode.sectionDescription')}>
          <div className="rounded-lg border bg-card p-5">
            <JoinCodePanel />
          </div>
        </Section>
      </div>
    </div>
  )
}
