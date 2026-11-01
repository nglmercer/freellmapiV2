import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Check, Minus, X } from 'lucide-react'
import type { MetadataMetric } from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { SimpleLineChart } from '@/components/charts/SimpleLineChart'
import { Disclosure } from '@/components/disclosure'
import { cn } from '@/lib/utils'
import { MetricTable } from './MetricTable'
import { selectClass } from './styles'
import { CAPABILITIES, capabilityEvidence, SPEED_METRIC } from './metrics'
import { formatDateTime, formatMs } from './format'
import type { HistoryPoint } from './types'

interface DeploymentDetail {
  metrics: MetadataMetric[]
  candidates: (MetadataMetric & { sourceModelId: string; selected: boolean })[]
  windows: {
    minutes: number
    sampleCount: number
    p50LatencyMs: number | null
    p95LatencyMs: number | null
  }[]
}

/** Mounts the detail (and its requests) only when the disclosure opens. */
export function LazyModelDetail({ id }: { id: number }) {
  const { t } = useTranslations()
  return (
    <Disclosure
      summary={<span className="text-xs font-medium text-muted-foreground">{t('metadata.details')}</span>}
      summaryClassName="py-2.5"
    >
      <ModelDetail id={id} />
    </Disclosure>
  )
}

export function ModelDetail({ id }: { id: number }) {
  const { t } = useTranslations()
  const [metric, setMetric] = useState(SPEED_METRIC)
  const detail = useQuery<DeploymentDetail>({
    queryKey: ['metadata-detail', id],
    queryFn: () => apiFetch(`/api/models/${id}/metadata`),
  })

  if (detail.isError) {
    return <p role="alert" className="py-3 text-xs text-destructive">{detail.error.message}</p>
  }
  if (!detail.data) {
    return <p className="py-3 text-xs text-muted-foreground">{t('metadata.loading')}</p>
  }

  const { metrics, candidates = [], windows } = detail.data
  const hasOtherCandidates = candidates.some((c) => !c.selected)

  return (
    <div className="space-y-6 py-4">
      <section className="space-y-2">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {t('metadata.capabilities')}
        </h3>
        <CapabilityGrid metrics={metrics} />
      </section>

      <section className="space-y-2">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {t('metadata.evidence')}
        </h3>
        <MetricTable metrics={metrics} />
        {hasOtherCandidates && (
          <Disclosure
            summary={<span className="text-xs text-muted-foreground">{t('metadata.candidates')}</span>}
            summaryClassName="py-2"
          >
            <div className="pt-2">
              <MetricTable metrics={candidates} />
            </div>
          </Disclosure>
        )}
      </section>

      <section className="space-y-2">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {t('metadata.localPerformance')}
        </h3>
        <div className="grid gap-2 sm:grid-cols-3">
          {windows.map((w) => (
            <div key={w.minutes} className="rounded-lg border bg-background p-3">
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-xs font-medium">{t('metadata.windowLabel', { minutes: w.minutes })}</span>
                <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
                  {w.sampleCount} {t('metadata.samples')}
                </span>
              </div>
              <dl className="mt-3 grid grid-cols-2 gap-2 text-xs">
                <LatencyCell label="p50" value={w.p50LatencyMs} unknown={t('metadata.unknown')} />
                <LatencyCell label="p95" value={w.p95LatencyMs} unknown={t('metadata.unknown')} />
              </dl>
            </div>
          ))}
        </div>
      </section>

      <HistorySection id={id} metric={metric} onMetricChange={setMetric} metrics={metrics} />
    </div>
  )
}

function CapabilityGrid({ metrics }: { metrics: MetadataMetric[] }) {
  const { t } = useTranslations()
  return (
    <ul className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
      {CAPABILITIES.map((capability) => {
        const state = capabilityEvidence(metrics, capability)
        return (
          <li
            key={capability}
            className="flex items-center justify-between gap-3 rounded-lg border bg-background px-3 py-2 text-xs"
          >
            <span className="font-medium">{t(`fallback.capability.${capability}`, { defaultValue: capability })}</span>
            <span
              className={cn(
                'inline-flex items-center gap-1 whitespace-nowrap',
                state === true && 'text-emerald-600 dark:text-emerald-400',
                state !== true && 'text-muted-foreground',
              )}
            >
              {state === true && <Check className="size-3.5" aria-hidden />}
              {state === false && <X className="size-3.5" aria-hidden />}
              {state === null && <Minus className="size-3.5" aria-hidden />}
              {t(state === true ? 'metadata.supported' : state === false ? 'metadata.unsupported' : 'metadata.unknown')}
            </span>
          </li>
        )
      })}
    </ul>
  )
}

function LatencyCell({ label, value, unknown }: { label: string; value: number | null; unknown: string }) {
  return (
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono tabular-nums">{value != null ? formatMs(value) : unknown}</dd>
    </div>
  )
}

function HistorySection({
  id,
  metric,
  onMetricChange,
  metrics,
}: {
  id: number
  metric: string
  onMetricChange: (metric: string) => void
  metrics: MetadataMetric[]
}) {
  const { t } = useTranslations()
  const history = useQuery<{ data: HistoryPoint[] }>({
    queryKey: ['metadata-history', id, metric],
    queryFn: () => apiFetch(`/api/models/${id}/history?metric=${encodeURIComponent(metric)}&window=30`),
  })
  const points = history.data?.data ?? []
  const numeric = [...points].reverse().filter((h) => h.value.type === 'number')
  const comparable =
    numeric.length >= 2 &&
    numeric.every((h) => h.source === numeric[0].source && h.benchmarkVersion === numeric[0].benchmarkVersion) &&
    numeric.some((h) => Number(h.value.value) > 0)
  const metricNames = Array.from(new Set([SPEED_METRIC, ...metrics.map((m) => m.metric)]))

  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{t('metadata.history')}</h3>
        <label className="flex items-center gap-2 text-xs">
          <span className="text-muted-foreground">{t('metadata.metric')}</span>
          <select className={selectClass} value={metric} onChange={(e) => onMetricChange(e.target.value)}>
            {metricNames.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
      </div>

      {history.isError && (
        <p role="alert" className="text-xs text-destructive">{history.error.message}</p>
      )}

      {comparable && (
        <div className="rounded-lg border bg-background p-3">
          <SimpleLineChart
            data={numeric.map((h) => ({
              date: new Date(h.changedAt).toLocaleDateString(),
              value: h.value.value,
            }))}
            xKey="date"
            lines={[{ dataKey: 'value', name: `${metric} · ${numeric[0].source}`, color: 'currentColor' }]}
            height={180}
          />
        </div>
      )}

      {points.length > 0 && (
        <ul className="max-h-48 divide-y overflow-y-auto rounded-lg border bg-background text-xs">
          {points.map((h) => (
            <li key={`${h.changedAt}-${h.source}-${h.benchmarkVersion}`} className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5 px-3 py-2">
              <span className="text-muted-foreground">{formatDateTime(h.changedAt)}</span>
              <span className="font-mono tabular-nums">
                {String(h.value.value)} {h.unit && !['text', 'boolean', 'json'].includes(h.unit) && h.unit}
              </span>
              <span>
                {h.source}
                {h.benchmarkVersion && <span className="text-muted-foreground"> · v{h.benchmarkVersion}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}

      {!history.isPending && !history.isError && points.length === 0 && (
        <p className="text-xs text-muted-foreground">{t('metadata.noHistory')}</p>
      )}
    </section>
  )
}
