import { useState } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { StatusBadge } from '@/components/status-badge'
import { Disclosure } from '@/components/disclosure'
import { formatDate } from '@/components/fallback-format'
import { formatInterval, sourceErrorKey, sourceStatusView } from '@/lib/source-status'
import type { MetadataSource } from '@freellmapi/shared'

const MIN_TTL = 300
const MAX_TTL = 604800

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0 space-y-0.5">
      <dt className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="text-sm">{children}</dd>
    </div>
  )
}

/** One metadata source: status, switch, facts, and its refresh interval. */
export function SourceCard({
  source,
  busy,
  onToggle,
  onInterval,
  onAddKey,
}: {
  source: MetadataSource
  busy: boolean
  onToggle: (enabled: boolean) => void
  onInterval: (seconds: number) => void
  onAddKey?: () => void
}) {
  const { t } = useTranslations()
  const view = sourceStatusView(source.status)
  const [interval, setInterval] = useState(String(source.ttlSeconds))
  const intervalValue = Number(interval)
  const intervalValid = Number.isInteger(intervalValue) && intervalValue >= MIN_TTL && intervalValue <= MAX_TTL
  const errorKey = source.error ? sourceErrorKey(source.error) : null

  return (
    <article className="flex flex-col rounded-lg border bg-card">
      <div className="flex items-start gap-3 p-4">
        <div className="min-w-0 flex-1 space-y-1">
          <h3 className="truncate text-sm font-medium">{source.attribution}</h3>
          <p className="truncate text-xs text-muted-foreground">{source.domains.join(' · ')}</p>
        </div>
        <Switch
          checked={source.enabled}
          disabled={busy}
          onCheckedChange={onToggle}
          aria-label={`${t('sources.enable')}: ${source.attribution}`}
        />
      </div>

      <div className="flex flex-wrap items-center gap-2 px-4 pb-3">
        <StatusBadge tone={view.tone}>{t(view.labelKey)}</StatusBadge>
        <StatusBadge tone={source.configured ? 'success' : 'warning'}>
          {source.configured ? t('sources.keyConfigured') : t('sources.keyMissing')}
        </StatusBadge>
        {!source.configured && onAddKey && (
          <Button variant="outline" size="sm" className="h-6 text-xs" onClick={onAddKey}>
            {t('metadata.addBenchmarkKey')}
          </Button>
        )}
      </div>

      <dl className="grid grid-cols-2 gap-x-4 gap-y-3 border-t px-4 py-3">
        <Fact label={t('sources.lastSuccess')}>
          {formatDate(source.lastSuccess) ?? t('metadata.unknown')}
        </Fact>
        <Fact label={t('sources.refreshEvery')}>{formatInterval(source.ttlSeconds)}</Fact>
        <Fact label={t('sources.license')}>
          <span className="text-muted-foreground">{source.license}</span>
        </Fact>
        {source.lastFailure && (
          <Fact label={t('sources.lastFailure')}>{formatDate(source.lastFailure) ?? '—'}</Fact>
        )}
      </dl>

      {source.error && (
        <p role="status" className="border-t bg-destructive/5 px-4 py-2 text-xs text-destructive">
          {errorKey ? t(errorKey) : source.error}
        </p>
      )}

      <Disclosure
        className="border-t"
        summaryClassName="px-4 py-2"
        summary={<span className="text-xs text-muted-foreground">{t('sources.intervalSettings')}</span>}
      >
        <form
          className="flex flex-wrap items-end gap-3 px-4 py-3"
          onSubmit={(event) => {
            event.preventDefault()
            if (intervalValid && intervalValue !== source.ttlSeconds) onInterval(intervalValue)
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor={`ttl-${source.id}`} className="text-xs">
              {t('sources.intervalSeconds')}
            </Label>
            <Input
              id={`ttl-${source.id}`}
              type="number"
              inputMode="numeric"
              min={MIN_TTL}
              max={MAX_TTL}
              value={interval}
              onChange={(event) => setInterval(event.target.value)}
              className="h-8 w-32 font-mono text-xs tabular-nums"
              aria-invalid={!intervalValid}
            />
          </div>
          <Button
            type="submit"
            size="sm"
            disabled={!intervalValid || intervalValue === source.ttlSeconds || busy}
          >
            {t('sources.save')}
          </Button>
          {!intervalValid && (
            <p className="basis-full text-xs text-destructive">
              {t('sources.intervalRange', { min: MIN_TTL, max: MAX_TTL })}
            </p>
          )}
        </form>
      </Disclosure>
    </article>
  )
}
