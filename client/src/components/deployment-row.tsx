import type { ReactNode } from 'react'
import type { MetadataDeployment } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { MetricMeter } from './metric-meter'
import { compactTokens, summarize, usd } from './catalog-metrics'

/** One provider deployment of a model, with its key numbers and a details slot. */
export function DeploymentRow({
  deployment,
  selected,
  disabled,
  maxSpeed,
  onSelect,
  details,
}: {
  deployment: MetadataDeployment
  selected: boolean
  disabled: boolean
  maxSpeed: number
  onSelect: (checked: boolean) => void
  details: ReactNode
}) {
  const { t } = useTranslations()
  const s = summarize(deployment)
  const priceText =
    s.priceInput != null || s.priceOutput != null
      ? `${usd(s.priceInput) ?? '—'} / ${usd(s.priceOutput) ?? '—'}`
      : null

  return (
    <div className="rounded-md border bg-background">
      <div className="flex items-start gap-3 p-3">
        <input
          type="checkbox"
          className="mt-1"
          aria-label={`${t('metadata.compare')} ${deployment.modelId}`}
          checked={selected}
          disabled={disabled}
          onChange={(e) => onSelect(e.target.checked)}
        />
        <div className="min-w-0 flex-1 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="secondary">{deployment.provider}</Badge>
            <span className="min-w-0 break-all font-mono text-xs">{deployment.modelId}</span>
            <span className="text-xs text-muted-foreground">#{deployment.id}</span>
            {deployment.freeTier && <Badge variant="outline">{t('fallback.tier.free')}</Badge>}
            {!deployment.enabled && <Badge variant="outline">{t('metadata.disabled')}</Badge>}
          </div>
          <div className="grid grid-cols-2 gap-x-6 gap-y-3 md:grid-cols-4">
            <MetricMeter
              tone="quality"
              label={t('metadata.quality')}
              value={s.quality != null ? s.quality.toFixed(1) : null}
              ratio={s.quality != null ? s.quality / 100 : 0}
              hint={t('metadata.hints.quality')}
            />
            <MetricMeter
              tone="speed"
              label={t('metadata.speed')}
              value={s.speed != null ? `${s.speed.toFixed(1)} tok/s` : null}
              ratio={s.speed != null && maxSpeed > 0 ? s.speed / maxSpeed : 0}
              hint={t('metadata.hints.speed')}
            />
            <MetricMeter
              label={t('metadata.context')}
              value={s.context != null ? compactTokens(s.context) : null}
              hint={t('metadata.hints.context')}
            />
            <MetricMeter
              label={t('metadata.price')}
              value={priceText}
              hint={t('metadata.unknownPrice')}
            />
          </div>
          {s.capabilities.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {s.capabilities.map((capability) => (
                <Badge key={capability} variant="outline">
                  {t(`fallback.capability.${capability}`, { defaultValue: capability })}
                </Badge>
              ))}
            </div>
          )}
        </div>
      </div>
      <div className="border-t px-3">{details}</div>
    </div>
  )
}
