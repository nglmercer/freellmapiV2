import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { SimpleBarChart, SimpleLineChart } from '@/components/charts'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { PageHeader } from '@/components/page-header'
import type { AnalyticsSummary, PlatformStats, TimelinePoint, RequestLog } from '@freellmapi/shared'

type TimeRange = '24h' | '7d' | '30d'

interface ModelStats {
  displayName: string
  platform: string
  requests: number
  successRate: number
  avgLatencyMs: number
  totalInputTokens: number
  totalOutputTokens: number
}

interface ErrorDistribution {
  byCategory: { category: string; count: number }[]
  byPlatform: { platform: string; count: number }[]
  detailed: { id: number; error: string; count: number }[]
}

function formatTokens(n?: number): string {
  if (!n) return '0'
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function Stat({ label, value, className }: { label: string; value: string | number; className?: string }) {
  return (
    <div className="rounded-lg border bg-card px-4 py-3">
      <p className="text-[11px] text-muted-foreground uppercase tracking-wider">{label}</p>
      <p className={`text-xl font-semibold tabular-nums mt-1 ${className ?? ''}`}>{value}</p>
    </div>
  )
}

function Panel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border bg-card">
      <div className="px-4 py-3 border-b">
        <h3 className="text-sm font-medium">{title}</h3>
      </div>
      <div className="p-4">{children}</div>
    </div>
  )
}

export default function AnalyticsPage() {
  const { t } = useTranslations()
  const [range, setRange] = useState<TimeRange>('7d')

  const { data: summary } = useQuery<AnalyticsSummary>({
    queryKey: ['analytics', 'summary', range],
    queryFn: () => apiFetch<AnalyticsSummary>(`/api/analytics/summary?range=${range}`),
  })

  const { data: byPlatform = [] } = useQuery<PlatformStats[]>({
    queryKey: ['analytics', 'by-platform', range],
    queryFn: () => apiFetch<PlatformStats[]>(`/api/analytics/by-platform?range=${range}`),
  })

  const { data: timeline = [] } = useQuery<TimelinePoint[]>({
    queryKey: ['analytics', 'timeline', range],
    queryFn: () => apiFetch<TimelinePoint[]>(`/api/analytics/timeline?range=${range}`),
  })

  const { data: byModel = [] } = useQuery<ModelStats[]>({
    queryKey: ['analytics', 'by-model', range],
    queryFn: () => apiFetch<ModelStats[]>(`/api/analytics/by-model?range=${range}`),
  })

  const { data: errors = [] } = useQuery<RequestLog[]>({
    queryKey: ['analytics', 'errors', range],
    queryFn: () => apiFetch<RequestLog[]>(`/api/analytics/errors?range=${range}`),
  })

  const { data: errorDist } = useQuery<ErrorDistribution>({
    queryKey: ['analytics', 'error-distribution', range],
    queryFn: () => apiFetch<ErrorDistribution>(`/api/analytics/error-distribution?range=${range}`),
  })

  return (
    <div>
      <PageHeader
        title={t('analytics.title')}
        description={t('analytics.description')}
        actions={
          <div className="flex gap-1 rounded-md border p-0.5">
            {(['24h', '7d', '30d'] as TimeRange[]).map(r => (
              <Button
                key={r}
                variant={range === r ? 'secondary' : 'ghost'}
                size="xs"
                onClick={() => setRange(r)}
              >
                {t(`analytics.timeRange.${r}`)}
              </Button>
            ))}
          </div>
        }
      />

      <div className="space-y-6">
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3">
          <Stat label={t('analytics.requests')} value={summary?.totalRequests ?? 0} />
          <Stat label={t('analytics.successRate')} value={`${summary?.successRate ?? 0}%`} />
          <Stat label={t('analytics.inputTokens')} value={formatTokens(summary?.totalInputTokens)} />
          <Stat label={t('analytics.outputTokens')} value={formatTokens(summary?.totalOutputTokens)} />
          <Stat label={t('analytics.avgLatency')} value={`${summary?.avgLatencyMs ?? 0} ms`} />
          <Stat label={t('analytics.estimatedSavings')} value={`$${summary?.estimatedCostSavings ?? '0.00'}`} />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <Panel title={t('analytics.requestsByProvider')}>
            {byPlatform.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noData')}</p>
            ) : (
              <SimpleBarChart data={byPlatform} dataKey="requests" xKey="platform" />
            )}
          </Panel>

          <Panel title={t('analytics.avgLatencyByProvider')}>
            {byPlatform.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noData')}</p>
            ) : (
              <SimpleBarChart
                data={byPlatform}
                dataKey="avgLatencyMs"
                xKey="platform"
                unit="ms"
                name="Latency"
                color="var(--muted-foreground)"
              />
            )}
          </Panel>

          <div className="lg:col-span-2">
            <Panel title={t('analytics.requestsOverTime')}>
              {timeline.length === 0 ? (
                <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noData')}</p>
              ) : (
                <SimpleLineChart
                  data={timeline}
                  xKey="timestamp"
                  lines={[
                    { dataKey: 'successCount', name: t('analytics.success'), color: 'var(--foreground)' },
                    { dataKey: 'failureCount', name: t('analytics.failures'), color: 'var(--destructive)' },
                  ]}
                />
              )}
            </Panel>
          </div>

          <div className="lg:col-span-2">
            <Panel title={t('analytics.perModelBreakdown')}>
              {byModel.length === 0 ? (
                <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noData')}</p>
              ) : (
                <div className="max-h-[360px] overflow-y-auto -mx-4">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead className="pl-4">{t('analytics.table.model')}</TableHead>
                        <TableHead>{t('analytics.table.provider')}</TableHead>
                        <TableHead className="text-right">{t('analytics.table.requests')}</TableHead>
                        <TableHead className="text-right">{t('analytics.table.success')}</TableHead>
                        <TableHead className="text-right">{t('analytics.table.latency')}</TableHead>
                        <TableHead className="text-right">{t('analytics.table.inTokens')}</TableHead>
                        <TableHead className="text-right pr-4">{t('analytics.table.outTokens')}</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {byModel.map((m: ModelStats, i: number) => (
                        <TableRow key={i}>
                          <TableCell className="pl-4 text-sm font-medium">{m.displayName}</TableCell>
                          <TableCell className="text-xs text-muted-foreground">{m.platform}</TableCell>
                          <TableCell className="text-right tabular-nums">{m.requests}</TableCell>
                          <TableCell className="text-right tabular-nums">{m.successRate}%</TableCell>
                          <TableCell className="text-right tabular-nums">{m.avgLatencyMs} ms</TableCell>
                          <TableCell className="text-right tabular-nums">{formatTokens(m.totalInputTokens)}</TableCell>
                          <TableCell className="text-right tabular-nums pr-4">{formatTokens(m.totalOutputTokens)}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              )}
            </Panel>
          </div>

          <Panel title={t('analytics.errorsByProvider')}>
            {!errorDist?.byPlatform?.length ? (
              <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noErrors')}</p>
            ) : (
              <SimpleBarChart
                data={errorDist.byPlatform}
                dataKey="count"
                xKey="platform"
                color="var(--destructive)"
              />
            )}
          </Panel>

          <Panel title={t('analytics.recentErrors')}>
            {errors.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-8">{t('analytics.noErrors')}</p>
            ) : (
              <div className="max-h-[240px] overflow-y-auto -mx-4">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="pl-4">{t('analytics.recentErrorsTable.provider')}</TableHead>
                      <TableHead>{t('analytics.recentErrorsTable.message')}</TableHead>
                      <TableHead className="text-right pr-4">{t('analytics.recentErrorsTable.time')}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {errors.slice(0, 20).map((e: RequestLog) => (
                      <TableRow key={e.id}>
                        <TableCell className="pl-4 text-xs">{e.platform}</TableCell>
                        <TableCell className="text-xs max-w-[200px] truncate">{e.error}</TableCell>
                        <TableCell className="text-right text-xs text-muted-foreground tabular-nums pr-4">
                          {new Date(e.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            )}
          </Panel>
        </div>
      </div>
    </div>
  )
}
