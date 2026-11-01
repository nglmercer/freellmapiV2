import type { ReactNode } from 'react'
import type { MetadataModel } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { Disclosure } from './disclosure'
import { KeyValueSummary } from './key-value-summary'
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
  const summaries = model.deployments.map(summarize)
  const best = (values: (number | null)[]) => {
    const present = values.filter((v): v is number => v != null)
    return present.length ? Math.max(...present) : null
  }
  const bestQuality = best(summaries.map((s) => s.quality))
  const bestSpeed = best(summaries.map((s) => s.speed))
  const bestContext = best(summaries.map((s) => s.context))
  const lowestPrice = (() => {
    const present = summaries.map((s) => s.priceInput).filter((v): v is number => v != null)
    return present.length ? Math.min(...present) : null
  })()

  return (
    <article className="rounded-lg border bg-card">
      <Disclosure
        summaryClassName="p-4"
        summary={
          <div className="flex items-center gap-4">
            <div className="min-w-0 flex-1 space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="truncate text-sm font-medium">{model.name}</h3>
                <Badge variant="outline">
                  {model.deployments.length} {t('metadata.deployments')}
                </Badge>
              </div>
              <p className="truncate font-mono text-xs text-muted-foreground">{model.key}</p>
            </div>
            <div className="hidden shrink-0 md:block">
              <KeyValueSummary
                emptyLabel={t('metadata.noMetrics')}
                items={[
                  { label: t('metadata.quality'), value: bestQuality != null ? bestQuality.toFixed(1) : null },
                  { label: t('metadata.speed'), value: bestSpeed != null ? `${bestSpeed.toFixed(0)} tok/s` : null },
                  { label: t('metadata.context'), value: bestContext != null ? compactTokens(bestContext) : null },
                  { label: t('metadata.priceShort'), value: usd(lowestPrice) },
                ]}
              />
            </div>
          </div>
        }
      >
        <div className="space-y-3 p-3">
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
      </Disclosure>
    </article>
  )
}
