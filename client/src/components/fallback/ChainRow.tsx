import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'
import type { Platform } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Disclosure } from '@/components/disclosure'
import { useProviderWizard } from '@/lib/provider-wizard-context'
import { cn } from '@/lib/utils'
import { clamp01 } from '@/components/fallback-format'
import { formatTokens } from '@/lib/format'
import { ChainRowDetails } from './ChainRowDetails'
import { providerLabel, qualityOf, speedOf, type FallbackEntry } from './chain-model'

type Tone = 'quality' | 'speed' | 'reliability'

const TONE_BAR: Record<Tone, string> = {
  quality: 'bg-violet-500',
  speed: 'bg-emerald-500',
  reliability: 'bg-sky-500',
}

/**
 * One chain entry. The four decision metrics sit on a single line; a missing
 * value shows a short reason, and the full explanation is in its tooltip.
 */
export function ChainRow({
  entry,
  rank,
  maxSpeed,
  onToggle,
}: {
  entry: FallbackEntry
  rank: number
  maxSpeed: number
  onToggle: (modelDbId: number, enabled: boolean) => void
}) {
  const { t } = useTranslations()
  const wizard = useProviderWizard()
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: entry.modelDbId,
  })

  // A provider without an enabled key cannot serve requests.
  const unconfigured = entry.configured === false
  const provider = providerLabel(entry.platform)
  const quality = qualityOf(entry)
  const speed = speedOf(entry)
  const reliability = entry.reliability?.successRate ?? null
  const context = entry.metrics?.contextWindow ?? null

  const qualityMissing = entry.qualityState === 'needs_source'
    ? { short: t('fallback.short.needsKey'), full: t('fallback.state.needsSource') }
    : { short: t('fallback.state.noData'), full: t('fallback.state.noData') }
  const speedMissing = entry.speedState === 'awaiting_requests'
    ? { short: t('fallback.short.awaiting'), full: t('fallback.state.awaitingRequests') }
    : { short: t('fallback.state.noData'), full: t('fallback.state.noData') }

  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn('bg-card', isDragging && 'relative z-10 opacity-90 shadow-lg')}
    >
      <div className="flex items-start gap-3 px-4 py-3">
        <button
          type="button"
          {...attributes}
          {...listeners}
          aria-label={t('fallback.dragToReorder')}
          className="mt-1 cursor-grab touch-none rounded-sm text-muted-foreground/60 transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
        >
          <GripVertical className="size-4" aria-hidden />
        </button>

        <span className="mt-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-muted font-mono text-xs tabular-nums">
          {rank}
        </span>

        <div className={cn('min-w-0 flex-1 space-y-2.5', !entry.enabled && !unconfigured && 'opacity-60')}>
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium">{entry.displayName}</span>
            <Badge variant="secondary">{provider}</Badge>
            <Badge variant={entry.freeTier ? 'outline' : 'secondary'}>
              {entry.freeTier ? t('fallback.tier.free') : t('fallback.tier.paid')}
            </Badge>
            {unconfigured && <Badge variant="outline">{t('fallback.notConfigured')}</Badge>}
            {entry.penalty > 0 && (
              <Badge variant="destructive">{t('fallback.penalty', { count: entry.penalty })}</Badge>
            )}
          </div>

          <dl className="grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
            <Metric
              label={t('fallback.metrics.quality')}
              value={quality != null ? quality.toFixed(1) : null}
              missing={qualityMissing}
              ratio={quality != null ? clamp01(quality / 100) : null}
              tone="quality"
            />
            <Metric
              label={t('fallback.metrics.speed')}
              value={speed != null ? `${speed.toFixed(1)} tok/s` : null}
              missing={speedMissing}
              ratio={speed != null && maxSpeed > 0 ? clamp01(speed / maxSpeed) : null}
              tone="speed"
            />
            <Metric
              label={t('fallback.metrics.reliability')}
              value={reliability != null ? `${(reliability * 100).toFixed(1)}%` : null}
              missing={{ short: t('fallback.state.noSamples'), full: t('fallback.state.noSamples') }}
              ratio={reliability != null ? clamp01(reliability) : null}
              tone="reliability"
            />
            <Metric
              label={t('fallback.metrics.context')}
              value={context != null ? formatTokens(context) : null}
              missing={{ short: t('fallback.state.noData'), full: t('fallback.state.noData') }}
            />
          </dl>
        </div>

        <div className="shrink-0 pt-0.5">
          {unconfigured ? (
            <Button variant="outline" size="sm" onClick={() => wizard.open({ platform: entry.platform as Platform })}>
              {t('fallback.configure', { provider })}
            </Button>
          ) : (
            <Switch
              checked={entry.enabled}
              onCheckedChange={(checked) => onToggle(entry.modelDbId, checked)}
              aria-label={entry.enabled ? t('fallback.disable') : t('fallback.enable')}
            />
          )}
        </div>
      </div>

      <Disclosure
        summaryClassName="px-4 py-2 pl-[4.25rem]"
        summary={<span className="text-xs font-medium text-muted-foreground">{t('fallback.details.summary')}</span>}
      >
        <ChainRowDetails entry={entry} />
      </Disclosure>
    </li>
  )
}

function Metric({
  label,
  value,
  missing,
  ratio,
  tone,
}: {
  label: string
  value: string | null
  missing: { short: string; full: string }
  ratio?: number | null
  tone?: Tone
}) {
  return (
    <div className="min-w-0 space-y-1">
      <dt className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{label}</dt>
      <dd className="m-0 min-h-5">
        {value != null ? (
          <span className="block truncate font-mono text-sm tabular-nums">{value}</span>
        ) : (
          <span className="block truncate text-xs text-muted-foreground" title={missing.full}>
            {missing.short}
          </span>
        )}
      </dd>
      {value != null && ratio != null && tone && (
        <Bar ratio={ratio} tone={tone} />
      )}
    </div>
  )
}

function Bar({ ratio, tone }: { ratio: number; tone: Tone }) {
  return (
    <div className="h-1 overflow-hidden rounded-full bg-muted" aria-hidden>
      <div
        className={cn('h-full rounded-full transition-[width] duration-300', TONE_BAR[tone])}
        style={{ width: `${Math.round(ratio * 100)}%` }}
      />
    </div>
  )
}
