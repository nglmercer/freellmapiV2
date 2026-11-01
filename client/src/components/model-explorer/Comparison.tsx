import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import type { MetadataMetric } from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { MetricValue } from './MetricTable'
import type { CompareItem } from './explorer/use-explorer-state'

interface CompareResponse {
  data: { id: number; metrics: MetadataMetric[] }[]
}

/**
 * Only the same metric, scope, source and benchmark version share a row, so a
 * cell is never compared against a different measurement.
 */
function rowKey(m: MetadataMetric): string {
  return `${m.metric}|${m.scope}|${m.source}|${m.benchmarkVersion}`
}

export function Comparison({ items }: { items: CompareItem[] }) {
  const { t } = useTranslations()
  const ids = items.map((item) => item.id)
  const query = useQuery<CompareResponse>({
    queryKey: ['metadata-compare', ids],
    queryFn: () => apiFetch(`/api/models/compare?ids=${ids.join(',')}`),
    enabled: ids.length >= 2,
  })

  const rows = useMemo(() => {
    const byKey = new Map<string, MetadataMetric>()
    for (const deployment of query.data?.data ?? []) {
      for (const metric of deployment.metrics) {
        const key = rowKey(metric)
        if (!byKey.has(key)) byKey.set(key, metric)
      }
    }
    return Array.from(byKey.entries())
      .map(([key, sample]) => ({ key, sample }))
      .sort((a, b) => a.sample.metric.localeCompare(b.sample.metric) || a.sample.scope.localeCompare(b.sample.scope))
  }, [query.data])

  if (ids.length < 2) {
    return <p className="py-6 text-center text-sm text-muted-foreground">{t('metadata.compareMin')}</p>
  }
  if (query.isError) {
    return <p role="alert" className="text-sm text-destructive">{query.error.message}</p>
  }
  if (query.isPending) {
    return <p className="py-6 text-center text-sm text-muted-foreground">{t('metadata.loading')}</p>
  }

  return (
    <div className="overflow-x-auto rounded-lg border">
      <table className="w-full min-w-[32rem] text-left text-xs">
        <thead className="bg-muted/40 text-[11px] uppercase tracking-wide text-muted-foreground">
          <tr>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.metric')}</th>
            {items.map((item) => (
              <th scope="col" key={item.id} className="px-3 py-2 font-medium normal-case tracking-normal text-foreground">
                <span className="block max-w-[14rem] truncate font-mono" title={item.label}>{item.label}</span>
                <span className="font-mono text-[11px] font-normal text-muted-foreground">#{item.id}</span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y">
          {rows.map(({ key, sample }) => (
            <tr key={key} className="align-top">
              <td className="px-3 py-2.5">
                <code className="font-mono text-[11px]">{sample.metric}</code>
                <div className="mt-0.5 text-[11px] text-muted-foreground">
                  {t(`metadata.${sample.scope}`)} · {sample.source}
                  {sample.benchmarkVersion && ` · v${sample.benchmarkVersion}`}
                </div>
              </td>
              {items.map((item) => {
                const metric = query.data?.data
                  .find((d) => d.id === item.id)
                  ?.metrics.find((m) => rowKey(m) === key)
                return (
                  <td key={item.id} className="px-3 py-2.5">
                    {metric ? (
                      <MetricValue metric={metric} />
                    ) : (
                      <span className="text-muted-foreground">N/A</span>
                    )}
                  </td>
                )
              })}
            </tr>
          ))}
          {rows.length === 0 && (
            <tr>
              <td colSpan={items.length + 1} className="px-3 py-6 text-center text-muted-foreground">
                {t('metadata.unknown')}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
