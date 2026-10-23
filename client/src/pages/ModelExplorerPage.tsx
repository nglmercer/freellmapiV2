import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type {
  MetadataMetric,
  MetadataModel,
  MetadataSource,
} from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { SimpleLineChart } from '@/components/charts/SimpleLineChart'

type Page = { data: MetadataModel[]; total: number }
type Match = {
  id: number
  source: string
  sourceModelId: string
  modelId: string
  canonicalHint: string | null
}
type Decision = {
  modelDbId: number
  eligible: boolean
  rejectionReasons: string[]
  score: number | null
  estimatedCostUsd: number | null
}
type HistoryPoint = {
  metric: string
  source: string
  changedAt: string
  benchmarkVersion: string
  value: MetadataMetric['value']
  unit: string
}
const selectClass = 'h-9 rounded-md border bg-background px-2 text-sm min-w-0'

function MetricTable({ metrics }: { metrics: MetadataMetric[] }) {
  const { t } = useTranslations()
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-xs text-left">
        <thead>
          <tr className="border-b">
            <th className="p-2">{t('metadata.metric')}</th>
            <th>{t('metadata.value')}</th>
            <th>{t('metadata.source')}</th>
            <th>{t('metadata.scope')}</th>
            <th>{t('metadata.updated')}</th>
          </tr>
        </thead>
        <tbody>
          {metrics.map((m) => (
            <tr key={`${m.id}-${m.metric}-${m.scope}`} className="border-b">
              <td className="p-2 font-mono">{m.metric}</td>
              <td className="pr-3">
                {typeof m.value.value === 'boolean'
                  ? t(
                      m.value.value
                        ? 'metadata.supported'
                        : 'metadata.unsupported',
                    )
                  : Array.isArray(m.value.value)
                    ? m.value.value.join(', ')
                    : String(m.value.value)}{' '}
                <span className="text-muted-foreground">
                  {!['text', 'boolean', 'json'].includes(m.unit) && m.unit}
                </span>
              </td>
              <td className="pr-3">
                {m.source}
                <div className="text-muted-foreground">
                  {m.benchmarkVersion && `v${m.benchmarkVersion}`}{' '}
                  {m.conflict && t('metadata.conflict')}
                </div>
              </td>
              <td className="pr-3">
                {t(`metadata.${m.scope}`)}
                <div>
                  {m.verified
                    ? t('metadata.verified')
                    : `${Math.round(m.confidence * 100)}%`}
                </div>
              </td>
              <td className="whitespace-nowrap" title={m.selectionReason}>
                {new Date(m.fetchedAt).toLocaleString()}{' '}
                {m.freshness === 'stale' && (
                  <span className="text-amber-600">{t('metadata.stale')}</span>
                )}
              </td>
            </tr>
          ))}
          {!metrics.length && (
            <tr>
              <td className="p-3" colSpan={5}>
                {t('metadata.unknown')}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
function LazyModelDetail({ id }: { id: number }) {
  const { t } = useTranslations()
  const [open, setOpen] = useState(false)
  return (
    <details onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary className="text-xs cursor-pointer py-2">
        {t('metadata.details')}
      </summary>
      {open && <ModelDetail id={id} />}
    </details>
  )
}
function ModelDetail({ id }: { id: number }) {
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
        <details>
          <summary className="text-xs cursor-pointer">
            {t('metadata.candidates')}
          </summary>
          <MetricTable metrics={detail.data.candidates} />
        </details>
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
function Comparison({ ids }: { ids: number[] }) {
  const { t } = useTranslations()
  const query = useQuery<{ data: { id: number; metrics: MetadataMetric[] }[] }>(
    {
      queryKey: ['metadata-compare', ids],
      queryFn: () => apiFetch(`/api/models/compare?ids=${ids.join(',')}`),
      enabled: ids.length >= 2,
    },
  )
  if (ids.length < 2) return null
  const keys = Array.from(
    new Set(
      query.data?.data.flatMap((d) =>
        d.metrics.map(
          (m) => `${m.metric}|${m.scope}|${m.source}|${m.benchmarkVersion}`,
        ),
      ) ?? [],
    ),
  )
  return (
    <section className="border rounded-md p-4 space-y-3">
      <h2 className="font-medium">{t('metadata.compare')}</h2>
      <p className="text-xs text-muted-foreground">
        {t('metadata.compareNote')}
      </p>
      {query.isError && <p role="alert">{query.error.message}</p>}
      <div className="overflow-x-auto">
        <table className="text-xs w-full text-left">
          <thead>
            <tr>
              <th>{t('metadata.metric')}</th>
              {ids.map((id) => (
                <th key={id}>#{id}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {keys.map((key) => (
              <tr key={key} className="border-t">
                <td className="py-2">{key.split('|').join(' · ')}</td>
                {ids.map((id) => {
                  const m = query.data?.data
                    .find((d) => d.id === id)
                    ?.metrics.find(
                      (m) =>
                        `${m.metric}|${m.scope}|${m.source}|${m.benchmarkVersion}` ===
                        key,
                    )
                  return (
                    <td key={id} className="px-2">
                      {m ? `${String(m.value.value)} ${m.unit}` : 'N/A'}
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
function SourceControls() {
  const { t } = useTranslations()
  const cache = useQueryClient()
  const query = useQuery<MetadataSource[]>({
    queryKey: ['metadata-sources'],
    queryFn: () => apiFetch('/api/metadata/sources'),
    refetchInterval: 30_000,
  })
  const update = useMutation({
    mutationFn: ({
      id,
      ...patch
    }: {
      id: string
      enabled?: boolean
      ttlSeconds?: number
    }) =>
      apiFetch(`/api/metadata/sources/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
    onSuccess: () =>
      cache.invalidateQueries({ queryKey: ['metadata-sources'] }),
  })
  const sync = useMutation({
    mutationFn: () => apiFetch('/api/metadata/sync', { method: 'POST' }),
    onSuccess: () =>
      cache.invalidateQueries({
        predicate: (q) => String(q.queryKey[0]).startsWith('metadata'),
      }),
  })
  const runs = useQuery<
    {
      id: number
      source: string
      status: string
      summary: {
        fetched: number
        valid: number
        matched: number
        unmatched: number
        changed: number
      }
    }[]
  >({
    queryKey: ['metadata-runs'],
    queryFn: () => apiFetch('/api/metadata/sync/status'),
  })
  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="font-medium">{t('metadata.sources')}</h2>
        <Button disabled={sync.isPending} onClick={() => sync.mutate()}>
          {sync.isPending ? t('metadata.loading') : t('metadata.refresh')}
        </Button>
      </div>
      {(query.error || update.error || sync.error) && (
        <p role="alert" className="text-destructive">
          {(query.error || update.error || sync.error)?.message}
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        {query.data?.map((s) => (
          <div className="border rounded-md p-4 space-y-2" key={s.id}>
            <label className="flex items-center gap-2 font-medium text-sm">
              <input
                type="checkbox"
                checked={s.enabled}
                disabled={update.isPending}
                onChange={(e) =>
                  update.mutate({ id: s.id, enabled: e.target.checked })
                }
              />
              {s.attribution}
            </label>
            <p className="text-xs">
              {s.status} ·{' '}
              {t(
                s.configured ? 'metadata.configured' : 'metadata.notConfigured',
              )}
            </p>
            <p className="text-xs text-muted-foreground">
              {s.domains.join(', ')} · {s.license}
            </p>
            <p className="text-xs">
              {t('metadata.lastSuccess')}:{' '}
              {s.lastSuccess
                ? new Date(s.lastSuccess).toLocaleString()
                : t('metadata.unknown')}
            </p>
            {s.error && <p className="text-xs text-destructive">{s.error}</p>}
            <label className="flex items-center gap-2 text-xs">
              TTL (s)
              <input
                type="number"
                className="border rounded p-1 w-28 bg-background"
                min={300}
                max={604800}
                defaultValue={s.ttlSeconds}
                key={`${s.id}-${s.ttlSeconds}`}
                onBlur={(e) => {
                  const value = Number(e.target.value)
                  if (
                    Number.isInteger(value) &&
                    value >= 300 &&
                    value <= 604800 &&
                    value !== s.ttlSeconds
                  )
                    update.mutate({ id: s.id, ttlSeconds: value })
                }}
              />
            </label>
          </div>
        ))}
      </div>
      <ul className="text-xs space-y-2">
        {runs.data?.slice(0, 12).map((r) => (
          <li key={r.id}>
            {r.source}: {r.status} · {t('metadata.fetched')}:{' '}
            {r.summary.fetched} · {t('metadata.matched')}: {r.summary.matched} ·{' '}
            {t('metadata.changed')}: {r.summary.changed}
          </li>
        ))}
      </ul>
    </section>
  )
}
function MatchReview() {
  const { t } = useTranslations()
  const cache = useQueryClient()
  const [status, setStatus] = useState('unmatched')
  const [canonical, setCanonical] = useState('')
  const [reason, setReason] = useState('')
  const [offset, setOffset] = useState(0)
  const query = useQuery<{ data: Match[] }>({
    queryKey: ['metadata-matches', status, offset],
    queryFn: () =>
      apiFetch(
        `/api/metadata/matches?status=${status}&limit=25&offset=${offset}`,
      ),
  })
  const mutation = useMutation({
    mutationFn: ({ id, reject }: { id: number; reject: boolean }) =>
      apiFetch(`/api/metadata/matches/${id}/resolve`, {
        method: 'POST',
        body: JSON.stringify({
          ...(reject
            ? { reject: true }
            : { canonicalModelId: Number(canonical) }),
          reason,
        }),
      }),
    onSuccess: () =>
      cache.invalidateQueries({
        predicate: (q) => String(q.queryKey[0]).startsWith('metadata'),
      }),
  })
  return (
    <section className="space-y-4">
      <h2 className="font-medium">{t('metadata.matches')}</h2>
      <p className="text-sm text-muted-foreground">{t('metadata.matchNote')}</p>
      <div className="flex flex-wrap gap-2">
        <select
          aria-label={t('metadata.status')}
          className={selectClass}
          value={status}
          onChange={(e) => {
            setStatus(e.target.value)
            setOffset(0)
          }}
        >
          {[
            'unmatched',
            'ambiguous',
            'matched',
            'rejected',
            'canonicalOnly',
          ].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <Input
          className="w-48"
          placeholder={t('metadata.canonicalId')}
          aria-label={t('metadata.canonicalId')}
          type="number"
          min={1}
          value={canonical}
          onChange={(e) => setCanonical(e.target.value)}
        />
        <Input
          className="sm:max-w-sm"
          placeholder={t('metadata.reason')}
          aria-label={t('metadata.reason')}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </div>
      {(query.error || mutation.error) && (
        <p role="alert">{(query.error || mutation.error)?.message}</p>
      )}
      {query.data?.data.map((m) => (
        <div
          className="flex flex-wrap items-center gap-2 border-b py-2 text-xs"
          key={m.id}
        >
          <span className="flex-1 min-w-48">
            {m.source} · {m.sourceModelId}
            <span className="block text-muted-foreground">
              {m.canonicalHint && `${t('metadata.hint')}: ${m.canonicalHint}`}
            </span>
          </span>
          <Button
            size="sm"
            disabled={
              mutation.isPending ||
              !reason.trim() ||
              !Number.isInteger(Number(canonical)) ||
              Number(canonical) < 1
            }
            onClick={() => mutation.mutate({ id: m.id, reject: false })}
          >
            {t('metadata.link')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={mutation.isPending || !reason.trim()}
            onClick={() => mutation.mutate({ id: m.id, reject: true })}
          >
            {t('metadata.reject')}
          </Button>
          <MatchAudit id={m.id} />
        </div>
      ))}
      <div className="flex gap-2">
        <Button
          variant="outline"
          disabled={!offset}
          onClick={() => setOffset(Math.max(0, offset - 25))}
        >
          {t('metadata.previous')}
        </Button>
        <Button
          variant="outline"
          disabled={(query.data?.data.length ?? 0) < 25}
          onClick={() => setOffset(offset + 25)}
        >
          {t('metadata.next')}
        </Button>
      </div>
    </section>
  )
}
function MatchAudit({ id }: { id: number }) {
  const { t } = useTranslations()
  const [open, setOpen] = useState(false)
  const query = useQuery<{
    data: {
      id: number
      decision: string
      canonicalModelId: number | null
      reason: string
      createdAt: string
    }[]
  }>({
    queryKey: ['metadata-audit', id],
    enabled: open,
    queryFn: () => apiFetch(`/api/metadata/matches/${id}/audit`),
  })
  return (
    <details
      className="text-xs"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary className="cursor-pointer">{t('metadata.audit')}</summary>
      {query.error && <p role="alert">{query.error.message}</p>}
      <ul>
        {query.data?.data.map((a) => (
          <li key={a.id}>
            {new Date(a.createdAt).toLocaleString()} · {a.decision} ·{' '}
            {a.canonicalModelId ?? 'N/A'} · {a.reason}
          </li>
        ))}
      </ul>
    </details>
  )
}
function Simulator() {
  const { t } = useTranslations()
  const [strategy, setStrategy] = useState('balanced')
  const [capability, setCapability] = useState('')
  const [free, setFree] = useState(false)
  const [input, setInput] = useState(1000)
  const [output, setOutput] = useState(500)
  const [maxCost, setMaxCost] = useState('')
  const [providers, setProviders] = useState('')
  const [weights, setWeights] = useState({
    quality: 0.35,
    speed: 0.15,
    reliability: 0.2,
    latency: 0.1,
    price: 0.1,
    availability: 0.1,
  })
  const policy = {
    strategy,
    requiredCapabilities: capability ? [capability] : [],
    freeOnly: free,
    ...(maxCost ? { maxCostUsd: Number(maxCost) } : {}),
    allowedProviders: providers
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    weights,
  }
  const save = useMutation({
    mutationFn: () =>
      apiFetch('/api/routing/policy', {
        method: 'PATCH',
        body: JSON.stringify(policy),
      }),
  })
  const mutation = useMutation<{
    selectedModelId: number | null
    decisions: Decision[]
  }>({
    mutationFn: () =>
      apiFetch('/api/routing/simulate', {
        method: 'POST',
        body: JSON.stringify({
          ...policy,
          inputTokens: input,
          outputTokens: output,
        }),
      }),
  })
  return (
    <section className="space-y-4">
      <h2 className="font-medium">{t('metadata.simulator')}</h2>
      <div className="flex flex-wrap gap-3 items-end">
        <label className="text-xs">
          {t('metadata.strategy')}
          <select
            className={`${selectClass} block`}
            value={strategy}
            onChange={(e) => setStrategy(e.target.value)}
          >
            {[
              'manual',
              'balanced',
              'quality',
              'fastest',
              'most-reliable',
              'lowest-latency',
              'cheapest',
              'best-coding',
              'best-reasoning',
              'free-only',
              'weighted',
            ].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          {t('metadata.capability')}
          <select
            className={`${selectClass} block`}
            value={capability}
            onChange={(e) => setCapability(e.target.value)}
          >
            <option value="">{t('metadata.any')}</option>
            {['tools', 'vision', 'reasoning', 'structured_output'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          {t('metadata.inputTokens')}
          <Input
            className="w-28"
            type="number"
            min={0}
            value={input}
            onChange={(e) => setInput(Number(e.target.value))}
          />
        </label>
        <label className="text-xs">
          {t('metadata.outputTokens')}
          <Input
            className="w-28"
            type="number"
            min={0}
            value={output}
            onChange={(e) => setOutput(Number(e.target.value))}
          />
        </label>
        <label className="text-sm flex gap-2">
          <input
            type="checkbox"
            checked={free}
            onChange={(e) => setFree(e.target.checked)}
          />
          {t('metadata.free')}
        </label>
        <Button disabled={mutation.isPending} onClick={() => mutation.mutate()}>
          {t('metadata.simulate')}
        </Button>
      </div>
      <details className="border rounded p-3">
        <summary className="text-sm cursor-pointer">
          {t('metadata.advanced')}
        </summary>
        <div className="flex flex-wrap gap-3 py-3">
          <label className="text-xs">
            {t('metadata.maxCost')}
            <Input
              className="w-40"
              min={0}
              type="number"
              step="any"
              value={maxCost}
              onChange={(e) => setMaxCost(e.target.value)}
            />
          </label>
          <label className="text-xs">
            {t('metadata.allowedProviders')}
            <Input
              className="w-64"
              value={providers}
              onChange={(e) => setProviders(e.target.value)}
            />
          </label>
          {Object.entries(weights).map(([key, value]) => (
            <label key={key} className="text-xs">
              {key}
              <Input
                className="w-24"
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={value}
                onChange={(e) =>
                  setWeights((w) => ({ ...w, [key]: Number(e.target.value) }))
                }
              />
            </label>
          ))}
        </div>
        <p className="text-xs text-muted-foreground mb-3">
          {t('metadata.policyNote')}
        </p>
        <Button
          disabled={save.isPending}
          variant="outline"
          onClick={() => save.mutate()}
        >
          {t('metadata.savePolicy')}
        </Button>
        {save.isSuccess && (
          <span className="text-xs ml-3">{t('metadata.saved')}</span>
        )}
        {save.error && <p role="alert">{save.error.message}</p>}
      </details>
      {mutation.error && <p role="alert">{mutation.error.message}</p>}
      {mutation.data && (
        <p className="text-sm">
          {t('metadata.selected')}:{' '}
          {mutation.data.selectedModelId ?? t('metadata.noCandidate')}
        </p>
      )}
      <ul className="text-xs space-y-2">
        {mutation.data?.decisions.map((d) => (
          <li key={d.modelDbId} className="border-b py-2">
            #{d.modelDbId} ·{' '}
            {t(d.eligible ? 'metadata.eligible' : 'metadata.excluded')} ·{' '}
            {d.score?.toFixed(3) ?? t('metadata.unknown')} ·{' '}
            {d.estimatedCostUsd === null
              ? t('metadata.unknownPrice')
              : `$${d.estimatedCostUsd}`}
            <p className="text-muted-foreground">
              {d.rejectionReasons.join(', ')}
            </p>
          </li>
        ))}
      </ul>
    </section>
  )
}
export default function ModelExplorerPage() {
  const { t } = useTranslations()
  const [tab, setTab] = useState('explorer')
  const [query, setQuery] = useState('')
  const [provider, setProvider] = useState('')
  const [capability, setCapability] = useState('')
  const [free, setFree] = useState(false)
  const [context, setContext] = useState('')
  const [sort, setSort] = useState('name')
  const [offset, setOffset] = useState(0)
  const [selected, setSelected] = useState<number[]>([])
  const params = new URLSearchParams({
    query,
    provider,
    capability,
    sort,
    limit: '25',
    offset: String(offset),
    ...(free ? { free: 'true' } : {}),
    ...(context ? { minContext: context } : {}),
  })
  const models = useQuery<Page>({
    queryKey: ['metadata-explorer', params.toString()],
    queryFn: () => apiFetch(`/api/metadata/explorer?${params}`),
    enabled: tab === 'explorer',
  })
  function change(action: () => void) {
    action()
    setOffset(0)
  }
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold">{t('metadata.title')}</h1>
        <p className="text-sm text-muted-foreground mt-1">
          {t('metadata.description')}
        </p>
      </div>
      <nav aria-label={t('metadata.title')} className="flex gap-2 flex-wrap">
        {['explorer', 'sources', 'matches', 'simulator'].map((s) => (
          <Button
            key={s}
            variant={tab === s ? 'default' : 'outline'}
            onClick={() => setTab(s)}
          >
            {t(`metadata.${s}`)}
          </Button>
        ))}
      </nav>
      {tab === 'sources' && <SourceControls />}
      {tab === 'matches' && <MatchReview />}
      {tab === 'simulator' && <Simulator />}
      {tab === 'explorer' && (
        <>
          <div className="grid gap-2 grid-cols-2 md:grid-cols-3">
            <Input
              className="col-span-2 md:col-span-1"
              aria-label={t('metadata.search')}
              placeholder={t('metadata.search')}
              value={query}
              onChange={(e) => change(() => setQuery(e.target.value))}
            />
            <Input
              aria-label={t('metadata.provider')}
              placeholder={t('metadata.provider')}
              value={provider}
              onChange={(e) => change(() => setProvider(e.target.value))}
            />
            <select
              aria-label={t('metadata.capability')}
              className={selectClass}
              value={capability}
              onChange={(e) => change(() => setCapability(e.target.value))}
            >
              <option value="">{t('metadata.anyCapability')}</option>
              {['tools', 'vision', 'reasoning', 'structured_output'].map(
                (s) => (
                  <option key={s}>{s}</option>
                ),
              )}
            </select>
            <Input
              type="number"
              min={0}
              aria-label={t('metadata.minContext')}
              placeholder={t('metadata.minContext')}
              value={context}
              onChange={(e) => change(() => setContext(e.target.value))}
            />
            <select
              aria-label={t('metadata.sort')}
              className={selectClass}
              value={sort}
              onChange={(e) => change(() => setSort(e.target.value))}
            >
              {[
                'name',
                'quality',
                'coding',
                'speed',
                'ttft',
                'price',
                'reliability',
                'context',
                'recency',
              ].map((s) => (
                <option key={s} value={s}>
                  {t(`metadata.${s}`)}
                </option>
              ))}
            </select>
            <label className="text-sm flex items-center gap-2">
              <input
                type="checkbox"
                checked={free}
                onChange={(e) => change(() => setFree(e.target.checked))}
              />
              {t('metadata.free')}
            </label>
          </div>
          {models.isLoading && <p>{t('metadata.loading')}</p>}
          {models.error && <p role="alert">{models.error.message}</p>}
          {models.data?.data.length === 0 && (
            <p className="text-muted-foreground">{t('metadata.empty')}</p>
          )}
          <Comparison ids={selected} />
          <div className="space-y-3">
            {models.data?.data.map((m) => (
              <details className="border rounded-md p-4" key={m.key}>
                <summary className="cursor-pointer font-medium text-sm">
                  {m.name}{' '}
                  <span className="text-muted-foreground font-normal">
                    · {m.deployments.length} {t('metadata.deployments')} ·{' '}
                    {m.key}
                  </span>
                </summary>
                <div className="mt-3 space-y-3">
                  {m.deployments.map((d) => (
                    <div key={d.id} className="border-t pt-3">
                      <div className="flex items-center gap-2 text-sm">
                        <input
                          aria-label={`${t('metadata.compare')} ${d.modelId}`}
                          type="checkbox"
                          checked={selected.includes(d.id)}
                          disabled={
                            selected.length >= 4 && !selected.includes(d.id)
                          }
                          onChange={(e) =>
                            setSelected((s) =>
                              e.target.checked
                                ? [...s, d.id]
                                : s.filter((id) => id !== d.id),
                            )
                          }
                        />
                        <span className="font-medium">{d.provider}</span>
                        <span className="text-muted-foreground break-all">
                          {d.modelId} · #{d.id}
                        </span>
                      </div>
                      <LazyModelDetail id={d.id} />
                    </div>
                  ))}
                </div>
              </details>
            ))}
          </div>
          <div className="flex justify-between items-center gap-3">
            <Button
              variant="outline"
              disabled={!offset}
              onClick={() => setOffset(Math.max(0, offset - 25))}
            >
              {t('metadata.previous')}
            </Button>
            <span className="text-xs">
              {models.data?.total ?? 0} {t('metadata.models')}
            </span>
            <Button
              variant="outline"
              disabled={offset + 25 >= (models.data?.total ?? 0)}
              onClick={() => setOffset(offset + 25)}
            >
              {t('metadata.next')}
            </Button>
          </div>
        </>
      )}
    </div>
  )
}
