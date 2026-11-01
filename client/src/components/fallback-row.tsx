import { useState } from 'react'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import type { FallbackEntry } from '../../../shared/types'
import { MetricMeter } from './metric-meter'
import {
  compactNumber,
  formatDate,
  missingReason,
  qualityOf,
  speedOf,
  usdPerMillion,
  type Translate,
} from './fallback-format'

function DetailRow({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="flex items-baseline justify-between gap-4 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono tabular-nums text-right">{value ?? '—'}</span>
    </div>
  )
}

function DetailGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="space-y-2">
      <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{title}</h3>
      <div className="space-y-1.5">{children}</div>
    </div>
  )
}

function confidenceText(value: number | null | undefined): string | null {
  return value != null && value > 0 ? `${Math.round(value * 100)}%` : null
}

/** Everything the registry knows about this model, grouped by what it tells you. */
function FallbackRowDetails({ entry, t }: { entry: FallbackEntry; t: Translate }) {
  const metrics = entry.metrics
  const price =
    metrics?.priceInput != null || metrics?.priceOutput != null
      ? `${usdPerMillion(metrics?.priceInput) ?? '—'} in · ${usdPerMillion(metrics?.priceOutput) ?? '—'} out`
      : null
  const capabilities = metrics?.capabilities ?? []

  return (
    <div className="grid gap-6 border-t bg-muted/30 px-4 py-4 md:grid-cols-3 md:pl-[4.5rem]">
      <DetailGroup title={t('fallback.details.performance')}>
        <DetailRow
          label={t('fallback.metrics.ttft')}
          value={metrics?.ttftMs != null ? `${Math.round(metrics.ttftMs)} ms` : null}
        />
        <DetailRow
          label={t('fallback.metrics.latency')}
          value={metrics?.latencyMs != null ? `${Math.round(metrics.latencyMs)} ms` : null}
        />
        <DetailRow
          label={t('fallback.metrics.coding')}
          value={metrics?.codingScore != null ? metrics.codingScore.toFixed(1) : null}
        />
        <DetailRow
          label={t('fallback.details.sampleCount')}
          value={String(entry.speed?.sampleCount ?? entry.reliability?.sampleCount ?? 0)}
        />
      </DetailGroup>

      <DetailGroup title={t('fallback.details.pricingContext')}>
        <DetailRow label={t('fallback.metrics.price')} value={price} />
        <DetailRow
          label={t('fallback.metrics.context')}
          value={metrics?.contextWindow != null ? `${compactNumber(metrics.contextWindow)} tokens` : null}
        />
        <DetailRow label={t('fallback.details.size')} value={entry.sizeLabel || null} />
        <DetailRow label={t('fallback.details.budget')} value={entry.monthlyTokenBudget || null} />
      </DetailGroup>

      <DetailGroup title={t('fallback.details.sources')}>
        <DetailRow label={t('fallback.metrics.quality')} value={entry.quality?.source ?? null} />
        <DetailRow label={t('fallback.details.confidence')} value={confidenceText(entry.quality?.confidence)} />
        <DetailRow label={t('fallback.details.updated')} value={formatDate(entry.quality?.updatedAt)} />
        <DetailRow label={t('fallback.metrics.speed')} value={entry.speed?.source ?? null} />
        <DetailRow
          label={t('fallback.metrics.reliability')}
          value={
            entry.reliability?.successRate != null
              ? `${(entry.reliability.successRate * 100).toFixed(1)}%`
              : null
          }
        />
        <DetailRow
          label={t('fallback.details.rateLimitHits')}
          value={String(entry.rateLimitHits ?? 0)}
        />
      </DetailGroup>

      <div className="space-y-2 md:col-span-3">
        <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {t('fallback.metrics.capabilities')}
        </h3>
        {capabilities.length === 0 ? (
          <p className="text-xs text-muted-foreground">{t('fallback.details.noCapabilities')}</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {capabilities.map((capability) => (
              <Badge key={capability} variant="outline">
                {t(`fallback.capability.${capability}`, { defaultValue: capability })}
              </Badge>
            ))}
          </div>
        )}
        {(entry.rpmLimit || entry.rpdLimit) && (
          <p className="text-xs text-muted-foreground">
            {[
              entry.rpmLimit ? t('fallback.rpm', { count: entry.rpmLimit }) : null,
              entry.rpdLimit ? t('fallback.rpd', { count: entry.rpdLimit }) : null,
            ]
              .filter(Boolean)
              .join(' · ')}
          </p>
        )}
      </div>
    </div>
  )
}

export function SortableFallbackRow({
  entry,
  index,
  maxSpeed,
  onToggle,
}: {
  entry: FallbackEntry
  index: number
  maxSpeed: number
  onToggle: (modelDbId: number, enabled: boolean) => void
}) {
  const { t } = useTranslations()
  const [expanded, setExpanded] = useState(false)
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: entry.modelDbId,
  })

  const quality = qualityOf(entry)
  const speed = speedOf(entry)
  const reliability = entry.reliability?.successRate ?? null
  const context = entry.metrics?.contextWindow ?? null

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(
        'bg-card transition-opacity',
        isDragging && 'opacity-50',
        !entry.enabled && 'opacity-60',
      )}
    >
      <div className="flex items-start gap-3 px-4 py-3">
        <button
          {...attributes}
          {...listeners}
          className="mt-1 cursor-grab text-muted-foreground/50 transition-colors hover:text-foreground active:cursor-grabbing"
          aria-label={t('fallback.dragToReorder')}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <circle cx="9" cy="6" r="1.5" /><circle cx="15" cy="6" r="1.5" />
            <circle cx="9" cy="12" r="1.5" /><circle cx="15" cy="12" r="1.5" />
            <circle cx="9" cy="18" r="1.5" /><circle cx="15" cy="18" r="1.5" />
          </svg>
        </button>

        <span className="mt-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-md bg-muted font-mono text-xs tabular-nums">
          {index + 1}
        </span>

        <div className="min-w-0 flex-1 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium">{entry.displayName}</span>
            <Badge variant="secondary">{entry.platform}</Badge>
            <Badge variant={entry.freeTier ? 'outline' : 'secondary'}>
              {entry.freeTier ? t('fallback.tier.free') : t('fallback.tier.paid')}
            </Badge>
            {entry.penalty > 0 && (
              <Badge variant="destructive">{t('fallback.penalty', { count: entry.penalty })}</Badge>
            )}
          </div>

          <div className="grid grid-cols-2 gap-x-6 gap-y-3 md:grid-cols-4">
            <MetricMeter
              tone="quality"
              label={t('fallback.metrics.quality')}
              value={quality != null ? quality.toFixed(1) : null}
              ratio={quality != null ? quality / 100 : 0}
              hint={missingReason(entry, 'quality', t)}
            />
            <MetricMeter
              tone="speed"
              label={t('fallback.metrics.speed')}
              value={speed != null ? `${speed.toFixed(1)} tok/s` : null}
              ratio={speed != null && maxSpeed > 0 ? speed / maxSpeed : 0}
              hint={missingReason(entry, 'speed', t)}
            />
            <MetricMeter
              tone="reliability"
              label={t('fallback.metrics.reliability')}
              value={reliability != null ? `${(reliability * 100).toFixed(1)}%` : null}
              ratio={reliability ?? 0}
              hint={t('fallback.state.noSamples')}
            />
            <MetricMeter
              label={t('fallback.metrics.context')}
              value={context != null ? compactNumber(context) : null}
              hint={t('fallback.state.noData')}
            />
          </div>
        </div>

        <div className="flex shrink-0 flex-col items-end gap-3">
          <Switch
            checked={entry.enabled}
            onCheckedChange={(checked) => onToggle(entry.modelDbId, checked)}
            aria-label={entry.enabled ? t('fallback.disable') : t('fallback.enable')}
          />
          <button
            type="button"
            onClick={() => setExpanded((open) => !open)}
            aria-expanded={expanded}
            className="text-xs text-muted-foreground transition-colors hover:text-foreground"
          >
            {expanded ? t('fallback.details.hide') : t('fallback.details.show')}
          </button>
        </div>
      </div>

      {expanded && <FallbackRowDetails entry={entry} t={t} />}
    </div>
  )
}
