import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import type { MetadataMetric } from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { SimpleLineChart } from '@/components/charts/SimpleLineChart'
import { Disclosure } from '@/components/disclosure'
import { MetricTable } from './MetricTable'
import { selectClass } from './styles'
import type { HistoryPoint } from './types'

/** Mounts the detail (and its requests) only when the disclosure opens. */
export function LazyModelDetail({ id }: { id: number }) {
  const { t } = useTranslations()
  return (
    <Disclosure
      summary={<span className="text-xs text-muted-foreground">{t('metadata.details')}</span>}
      summaryClassName="py-2"
    >
      <ModelDetail id={id} />
    </Disclosure>
  )
}
export function ModelDetail({ id }: { id: number }) {
  const { t } = useTranslations()
  const [metric, setMetric] = useState('performance.output_tps')
  const detail = useQuery<{
    metrics: MetadataMetric[]
    candidates: (MetadataMetric & {
      sourceModelId: string
      selected: boolean
    })[]
    windows: {
      minutes: number
      sampleCount: number
      p50LatencyMs: number | null
      p95LatencyMs: number | null
    }[]
  }>({
    queryKey: ['metadata-detail', id],
    queryFn: () => apiFetch(`/api/models/${id}/metadata`),
  })
  const history = useQuery<{ data: HistoryPoint[] }>({
    queryKey: ['metadata-history', id, metric],
    queryFn: () =>
      apiFetch(
        `/api/models/${id}/history?metric=${encodeURIComponent(metric)}&window=30`,
      ),
  })
  if (detail.isError) return <p role="alert">{detail.error.message}</p>
  if (!detail.data) return <p>{t('metadata.loading')}</p>
  const numeric = [...(history.data?.data ?? [])]
    .reverse()
    .filter((h) => h.value.type === 'number')
  const comparable =
    numeric.length >= 2 &&
    numeric.every(
      (h) =>
        h.source === numeric[0].source &&
        h.benchmarkVersion === numeric[0].benchmarkVersion,
    ) &&
    numeric.some((h) => Number(h.value.value) > 0)
  return (
    <div className="space-y-4 py-3">
      <div className="flex flex-wrap gap-3 text-xs">
        {[
          'tools',
          'vision',
          'reasoning',
          'structured_output',
          'audio_input',
          'video_input',
        ].map((cap) => {
          const evidence = detail.data.metrics.find(
            (m) =>
              m.metric === `capability.${cap}` &&
              m.scope === 'providerDeployment',
          )
          return (
            <span key={cap}>
              {cap}:{' '}
              {evidence
                ? t(
                    evidence.value.value === true
                      ? 'metadata.supported'
                      : 'metadata.unsupported',
                  )
                : t('metadata.unknown')}
            </span>
          )
        })}
      </div>
      <MetricTable metrics={detail.data.metrics} />
      {detail.data.candidates?.some((c) => !c.selected) && (
        <Disclosure
          summary={<span className="text-xs text-muted-foreground">{t('metadata.candidates')}</span>}
          summaryClassName="py-2"
        >
          <MetricTable metrics={detail.data.candidates} />
        </Disclosure>
      )}
      <div className="grid gap-2 sm:grid-cols-3">
        {detail.data.windows.map((w) => (
          <div key={w.minutes} className="rounded border p-3 text-xs">
            {w.minutes} min · {w.sampleCount} {t('metadata.samples')}
            <p>
              p50: {w.p50LatencyMs ?? t('metadata.unknown')} ms · p95:{' '}
              {w.p95LatencyMs ?? t('metadata.unknown')} ms
            </p>
          </div>
        ))}
      </div>
      <label className="flex gap-2 items-center text-sm">
        {t('metadata.history')}
        <select
          className={selectClass}
          value={metric}
          onChange={(e) => setMetric(e.target.value)}
        >
          {Array.from(
            new Set([
              'performance.output_tps',
              ...detail.data.metrics.map((m) => m.metric),
            ]),
          ).map((k) => (
            <option key={k}>{k}</option>
          ))}
        </select>
      </label>
      {history.isError && <p role="alert">{history.error.message}</p>}
      {comparable && (
        <SimpleLineChart
          data={numeric.map((h) => ({
            date: new Date(h.changedAt).toLocaleDateString(),
            value: h.value.value,
          }))}
          xKey="date"
          lines={[
            {
              dataKey: 'value',
              name: `${metric} · ${numeric[0].source}`,
              color: 'currentColor',
            },
          ]}
          height={180}
        />
      )}
      <ul className="text-xs space-y-1 max-h-48 overflow-y-auto">
        {history.data?.data.map((h, i) => (
          <li key={i}>
            {new Date(h.changedAt).toLocaleString()} · {String(h.value.value)}{' '}
            {h.unit} · {h.source}{' '}
            {h.benchmarkVersion && `v${h.benchmarkVersion}`}
          </li>
        ))}
      </ul>
      {history.data?.data.length === 0 && (
        <p className="text-xs text-muted-foreground">
          {t('metadata.noHistory')}
        </p>
      )}
    </div>
  )
}
