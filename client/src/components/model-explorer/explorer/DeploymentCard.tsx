import type { ReactNode } from 'react'
import type { MetadataDeployment } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { summarize } from '../metrics'
import { clampRatio, formatScore, formatTokens, formatTps, formatUsd } from '../format'
import { LazyModelDetail } from '../ModelDetail'
import { MAX_COMPARE } from './filter-state'

/**
 * One provider deployment: identity, the four decision metrics, its capabilities
 * and the evidence drawer. Missing values say why they are missing instead of
 * showing an empty bar.
 */
export function DeploymentCard({
  deployment,
  selected,
  disabled,
  maxSpeed,
  onToggleCompare,
}: {
  deployment: MetadataDeployment
  selected: boolean
  disabled: boolean
  maxSpeed: number
  onToggleCompare: (checked: boolean) => void
}) {
  const { t } = useTranslations()
  const s = summarize(deployment)
  const price =
    s.priceInput == null && s.priceOutput == null
      ? null
      : `${formatUsd(s.priceInput) ?? '—'} / ${formatUsd(s.priceOutput) ?? '—'}`

  return (
    <article
      className={cn(
        'overflow-hidden rounded-lg border bg-background transition-shadow',
        selected && 'ring-1 ring-foreground/40',
      )}
    >
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b px-4 py-2.5">
        <Badge variant="secondary">{deployment.provider}</Badge>
        <code className="min-w-0 break-all font-mono text-xs">{deployment.modelId}</code>
        <span className="font-mono text-[11px] text-muted-foreground">#{deployment.id}</span>
        {deployment.freeTier && (
          <Badge variant="outline" className="border-emerald-500/40 text-emerald-600 dark:text-emerald-400">
            {t('fallback.tier.free')}
          </Badge>
        )}
        {!deployment.enabled && <Badge variant="outline">{t('metadata.disabled')}</Badge>}

        <label
          title={disabled ? t('metadata.limitReached', { max: MAX_COMPARE }) : undefined}
          className={cn(
            'ml-auto inline-flex cursor-pointer select-none items-center gap-2 rounded-md border px-2.5 py-1 text-xs font-medium transition-colors',
            selected ? 'border-foreground bg-foreground text-background' : 'hover:bg-muted',
            disabled && 'cursor-not-allowed opacity-50 hover:bg-transparent',
          )}
        >
          <input
            type="checkbox"
            className="sr-only"
            aria-label={`${t('metadata.compare')} ${deployment.provider} ${deployment.modelId}`}
            checked={selected}
            disabled={disabled}
            onChange={(e) => onToggleCompare(e.target.checked)}
          />
          <span aria-hidden className="flex size-3.5 items-center justify-center rounded-sm border border-current">
            {selected && <span className="size-2 rounded-[1px] bg-current" />}
          </span>
          {t('metadata.compare')}
        </label>
      </header>

      <div className="grid grid-cols-2 gap-px bg-border md:grid-cols-4">
        <MetricTile
          label={t('metadata.quality')}
          value={s.quality != null ? formatScore(s.quality) : null}
          hint={t('metadata.hints.quality')}
          ratio={s.quality != null ? clampRatio(s.quality / 100) : undefined}
          tone="quality"
        />
        <MetricTile
          label={t('metadata.speed')}
          value={s.speed != null ? formatTps(s.speed) : null}
          hint={t('metadata.hints.speed')}
          ratio={s.speed != null && maxSpeed > 0 ? clampRatio(s.speed / maxSpeed) : undefined}
          tone="speed"
        />
        <MetricTile
          label={t('metadata.context')}
          value={s.context != null ? formatTokens(s.context) : null}
          hint={t('metadata.hints.context')}
        />
        <MetricTile
          label={t('metadata.price')}
          value={price}
          hint={t('metadata.unknownPrice')}
        />
      </div>

      <div className="flex flex-wrap gap-1.5 px-4 py-3">
        {s.capabilities.length > 0 ? (
          s.capabilities.map((capability) => (
            <Badge key={capability} variant="outline">
              {t(`fallback.capability.${capability}`, { defaultValue: capability })}
            </Badge>
          ))
        ) : (
          <span className="text-xs text-muted-foreground">{t('metadata.noCapabilityData')}</span>
        )}
      </div>

      <div className="border-t px-4">
        <LazyModelDetail id={deployment.id} />
      </div>
    </article>
  )
}

const TONE_BAR = {
  quality: 'bg-violet-500',
  speed: 'bg-emerald-500',
} as const

function MetricTile({
  label,
  value,
  hint,
  ratio,
  tone,
}: {
  label: string
  value: string | null
  hint: ReactNode
  ratio?: number
  tone?: keyof typeof TONE_BAR
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2 bg-background p-4">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{label}</div>
      {value != null ? (
        <div className="truncate font-mono text-base font-semibold tabular-nums">{value}</div>
      ) : (
        <div className="text-xs leading-snug text-muted-foreground">{hint}</div>
      )}
      {value != null && ratio != null && tone && (
        <div className="h-1 overflow-hidden rounded-full bg-muted" aria-hidden>
          <div className={cn('h-full rounded-full transition-[width] duration-300', TONE_BAR[tone])} style={{ width: `${Math.round(ratio * 100)}%` }} />
        </div>
      )}
    </div>
  )
}
