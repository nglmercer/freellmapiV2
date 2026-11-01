import { useState, type ReactNode } from 'react'
import { ChevronDown } from 'lucide-react'
import type { MetadataModel } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { DeploymentRow } from './deployment-row'
import { compactTokens, summarize, usd } from './catalog-metrics'

/** A model with its provider deployments, collapsed to one line by default. */
export function ModelCard({
  model,
  selected,
  limitReached,
  maxSpeed,
  onSelect,
  renderDetails,
}: {
  model: MetadataModel
  selected: number[]
  limitReached: boolean
  maxSpeed: number
  onSelect: (id: number, checked: boolean) => void
  renderDetails: (id: number) => ReactNode
}) {
  const { t } = useTranslations()
  const [open, setOpen] = useState(false)
  const summaries = model.deployments.map(summarize)
  const best = {
    quality: Math.max(-1, ...summaries.map((s) => s.quality ?? -1)),
    speed: Math.max(-1, ...summaries.map((s) => s.speed ?? -1)),
    context: Math.max(-1, ...summaries.map((s) => s.context ?? -1)),
  }
  const cheapest = Math.min(
    ...summaries.map((s) => s.priceInput ?? Infinity),
  )
  const hasAnything = best.quality >= 0 || best.speed >= 0 || best.context >= 0

  return (
    <article className="rounded-lg border bg-card">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="flex w-full items-center gap-4 p-4 text-left"
      >
        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-sm font-medium">{model.name}</h3>
            <Badge variant="outline">
              {model.deployments.length} {t('metadata.deployments')}
            </Badge>
          </div>
          <p className="truncate font-mono text-xs text-muted-foreground">{model.key}</p>
        </div>
        <div className="hidden shrink-0 gap-5 text-xs tabular-nums md:flex">
          {best.quality >= 0 && (
            <span>
              <span className="text-muted-foreground">{t('metadata.quality')} </span>
              <span className="font-mono">{best.quality.toFixed(1)}</span>
            </span>
          )}
          {best.speed >= 0 && (
            <span>
              <span className="text-muted-foreground">{t('metadata.speed')} </span>
              <span className="font-mono">{best.speed.toFixed(0)} tok/s</span>
            </span>
          )}
          {best.context >= 0 && (
            <span>
              <span className="text-muted-foreground">{t('metadata.context')} </span>
              <span className="font-mono">{compactTokens(best.context)}</span>
            </span>
          )}
          {Number.isFinite(cheapest) && (
            <span>
              <span className="text-muted-foreground">{t('metadata.price')} </span>
              <span className="font-mono">{usd(cheapest)}</span>
            </span>
          )}
          {!hasAnything && <span className="text-muted-foreground">{t('metadata.noMetrics')}</span>}
        </div>
        <ChevronDown className={cn('size-4 shrink-0 text-muted-foreground transition-transform', open && 'rotate-180')} aria-hidden />
      </button>

      {open && (
        <div className="space-y-3 border-t p-3">
          {model.deployments.map((deployment) => (
            <DeploymentRow
              key={deployment.id}
              deployment={deployment}
              selected={selected.includes(deployment.id)}
              disabled={limitReached && !selected.includes(deployment.id)}
              maxSpeed={maxSpeed}
              onSelect={(checked) => onSelect(deployment.id, checked)}
              details={renderDetails(deployment.id)}
            />
          ))}
          {summaries.length === 0 && <p className="text-xs text-muted-foreground">{t('metadata.empty')}</p>}
        </div>
      )}
    </article>
  )
}
