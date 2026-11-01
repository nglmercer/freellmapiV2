import { useId, useState } from 'react'
import { ChevronDown } from 'lucide-react'
import type { MetadataModel } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { summarizeModel } from '../metrics'
import { formatScore, formatTokens, formatTps, formatUsd } from '../format'
import { DeploymentCard } from './DeploymentCard'

/**
 * One canonical model. The header is a button that expands its deployments;
 * the columns show the best value across them, so the list scans by metric.
 */
export function ModelGroup({
  model,
  selectedIds,
  canSelectMore,
  maxSpeed,
  onToggleCompare,
}: {
  model: MetadataModel
  selectedIds: Set<number>
  canSelectMore: boolean
  maxSpeed: number
  onToggleCompare: (deployment: { id: number; label: string }, checked: boolean) => void
}) {
  const { t } = useTranslations()
  const [open, setOpen] = useState(false)
  const panelId = useId()
  const best = summarizeModel(model)
  const price =
    best.priceInput == null && best.priceOutput == null
      ? null
      : `${formatUsd(best.priceInput) ?? '—'} / ${formatUsd(best.priceOutput) ?? '—'}`
  const limitReached = !canSelectMore

  return (
    <article
      className={cn(
        'overflow-hidden rounded-xl border bg-card transition-shadow',
        open && 'shadow-sm',
      )}
    >
      <button
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-4 p-4 text-left transition-colors hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none"
      >
        <span className="min-w-0 flex-1 space-y-1">
          <span className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-semibold">{model.name}</span>
            <Badge variant="outline">
              {model.deployments.length} {t('metadata.deployments')}
            </Badge>
            {best.freeDeployments > 0 && (
              <Badge variant="secondary">
                {t('metadata.stats.free', { count: best.freeDeployments })}
              </Badge>
            )}
          </span>
          <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
            <span className="font-mono">{model.key}</span>
            <span aria-hidden>·</span>
            <span className="truncate">{best.providers.join(', ')}</span>
          </span>
        </span>

        <span className="hidden shrink-0 items-center gap-6 md:flex">
          <HeaderMetric label={t('metadata.quality')} value={best.quality != null ? formatScore(best.quality) : null} />
          <HeaderMetric label={t('metadata.speed')} value={best.speed != null ? formatTps(best.speed) : null} />
          <HeaderMetric label={t('metadata.context')} value={best.context != null ? formatTokens(best.context) : null} />
          <HeaderMetric label={t('metadata.priceShort')} value={price} wide />
        </span>

        <ChevronDown
          aria-hidden
          className={cn('size-4 shrink-0 text-muted-foreground transition-transform', open && 'rotate-180')}
        />
      </button>

      {open && (
        <div id={panelId} className="space-y-3 border-t bg-muted/20 p-3 sm:p-4">
          {model.deployments.map((deployment) => {
            const selected = selectedIds.has(deployment.id)
            return (
              <DeploymentCard
                key={deployment.id}
                deployment={deployment}
                selected={selected}
                disabled={limitReached && !selected}
                maxSpeed={maxSpeed}
                onToggleCompare={(checked) =>
                  onToggleCompare(
                    { id: deployment.id, label: `${deployment.provider} · ${deployment.modelId}` },
                    checked,
                  )
                }
              />
            )
          })}
          {model.deployments.length === 0 && (
            <p className="text-xs text-muted-foreground">{t('metadata.empty')}</p>
          )}
        </div>
      )}
    </article>
  )
}

function HeaderMetric({ label, value, wide }: { label: string; value: string | null; wide?: boolean }) {
  return (
    <span className={cn('flex flex-col items-end gap-0.5 text-right', wide ? 'min-w-[9.5rem]' : 'min-w-[4.5rem]')}>
      <span className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</span>
      <span className={cn('font-mono text-sm tabular-nums', value == null && 'text-muted-foreground')}>
        {value ?? '—'}
      </span>
    </span>
  )
}
