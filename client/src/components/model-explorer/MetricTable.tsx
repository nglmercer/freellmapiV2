import type { MetadataMetric } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { formatDateTime, formatPercent } from './format'

const AMBER = 'border-amber-500/40 text-amber-600 dark:text-amber-400'

/** A metric's value with its unit. Booleans read as supported / unsupported. */
export function MetricValue({ metric }: { metric: MetadataMetric }) {
  const { t } = useTranslations()
  const value = metric.value.value

  if (typeof value === 'boolean') {
    return (
      <span className={cn('font-medium', value ? 'text-emerald-600 dark:text-emerald-400' : 'text-muted-foreground')}>
        {t(value ? 'metadata.supported' : 'metadata.unsupported')}
      </span>
    )
  }

  const text = Array.isArray(value)
    ? value.join(', ')
    : typeof value === 'number'
      ? value.toLocaleString('en-US', { maximumFractionDigits: 4 })
      : String(value)
  const showUnit = !['text', 'boolean', 'json'].includes(metric.unit)

  return (
    <span className="font-mono tabular-nums">
      {text}
      {showUnit && <span className="ml-1 font-sans text-muted-foreground">{metric.unit}</span>}
    </span>
  )
}

/** Every observation behind a deployment or model: value, source, scope, freshness. */
export function MetricTable({ metrics }: { metrics: MetadataMetric[] }) {
  const { t } = useTranslations()

  return (
    <div className="overflow-x-auto rounded-lg border bg-background">
      <table className="w-full min-w-[40rem] text-left text-xs">
        <thead className="bg-muted/40 text-[11px] uppercase tracking-wide text-muted-foreground">
          <tr>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.metric')}</th>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.value')}</th>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.source')}</th>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.scope')}</th>
            <th scope="col" className="px-3 py-2 font-medium">{t('metadata.updated')}</th>
          </tr>
        </thead>
        <tbody className="divide-y">
          {metrics.map((m) => (
            <tr key={`${m.id}-${m.metric}-${m.scope}`} className="align-top">
              <td className="max-w-[16rem] px-3 py-2.5">
                <code className="break-all font-mono text-[11px]">{m.metric}</code>
                {m.selectionReason && (
                  <p className="mt-1 line-clamp-2 text-[11px] text-muted-foreground" title={m.selectionReason}>
                    {m.selectionReason}
                  </p>
                )}
              </td>
              <td className="px-3 py-2.5">
                <MetricValue metric={m} />
              </td>
              <td className="px-3 py-2.5">
                <div className="font-medium">{m.source}</div>
                <div className="mt-0.5 flex flex-wrap items-center gap-1 text-muted-foreground">
                  {m.benchmarkVersion && <span>v{m.benchmarkVersion}</span>}
                  {m.conflict && <Badge variant="outline" className={AMBER}>{t('metadata.conflict')}</Badge>}
                </div>
              </td>
              <td className="px-3 py-2.5">
                <div>{t(`metadata.${m.scope}`)}</div>
                <div className="mt-0.5 text-muted-foreground">
                  {m.verified ? (
                    <span className="font-medium text-foreground">{t('metadata.verified')}</span>
                  ) : (
                    formatPercent(m.confidence)
                  )}
                </div>
              </td>
              <td className="whitespace-nowrap px-3 py-2.5">
                <div>{formatDateTime(m.fetchedAt)}</div>
                {m.freshness === 'stale' && (
                  <Badge variant="outline" className={cn('mt-1', AMBER)}>{t('metadata.stale')}</Badge>
                )}
              </td>
            </tr>
          ))}
          {metrics.length === 0 && (
            <tr>
              <td colSpan={5} className="px-3 py-6 text-center text-muted-foreground">
                {t('metadata.unknown')}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
