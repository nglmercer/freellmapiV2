import type { ReactNode } from 'react'
import type { FallbackEntry } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Badge } from '@/components/ui/badge'
import { formatDate } from '@/components/fallback-format'
import { formatTokens, formatUsd } from '@/lib/format'

function DetailRow({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="flex items-baseline justify-between gap-4 text-xs">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="m-0 text-right font-mono tabular-nums">{value ?? '—'}</dd>
    </div>
  )
}

function DetailGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-2">
      <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{title}</h3>
      <dl className="space-y-1.5">{children}</dl>
    </section>
  )
}

function ms(value: number | null | undefined): string | null {
  return value != null ? `${Math.round(value)} ms` : null
}

/** Everything the registry knows about one entry, grouped by what it tells you. */
export function ChainRowDetails({ entry }: { entry: FallbackEntry }) {
  const { t } = useTranslations()
  const metrics = entry.metrics
  const price =
    metrics?.priceInput != null || metrics?.priceOutput != null
      ? `${formatUsd(metrics?.priceInput) ?? '—'} in · ${formatUsd(metrics?.priceOutput) ?? '—'} out`
      : null
  const limits = [
    entry.rpmLimit ? t('fallback.rpm', { count: entry.rpmLimit }) : null,
    entry.rpdLimit ? t('fallback.rpd', { count: entry.rpdLimit }) : null,
  ].filter(Boolean)
  const capabilities = metrics?.capabilities ?? []
  const measured = entry.speed?.sampleCount ?? entry.reliability?.sampleCount ?? 0

  return (
    <div className="grid gap-6 px-4 pb-4 pl-[4.25rem] md:grid-cols-3">
      <DetailGroup title={t('fallback.details.performance')}>
        <DetailRow label={t('fallback.metrics.ttft')} value={ms(metrics?.ttftMs)} />
        <DetailRow label={t('fallback.metrics.latency')} value={ms(metrics?.latencyMs)} />
        <DetailRow
          label={t('fallback.metrics.coding')}
          value={metrics?.codingScore != null ? metrics.codingScore.toFixed(1) : null}
        />
        <DetailRow label={t('fallback.details.sampleCount')} value={String(measured)} />
      </DetailGroup>

      <DetailGroup title={t('fallback.details.pricingContext')}>
        <DetailRow label={t('fallback.metrics.price')} value={price} />
        <DetailRow
          label={t('fallback.metrics.context')}
          value={metrics?.contextWindow != null ? `${formatTokens(metrics.contextWindow)} tokens` : null}
        />
        <DetailRow label={t('fallback.details.size')} value={entry.sizeLabel || null} />
        <DetailRow label={t('fallback.details.budget')} value={entry.monthlyTokenBudget || null} />
        <DetailRow label={t('fallback.details.rateLimits')} value={limits.length ? limits.join(' · ') : null} />
      </DetailGroup>

      <DetailGroup title={t('fallback.details.sources')}>
        <DetailRow label={t('fallback.metrics.quality')} value={entry.quality?.source ?? null} />
        <DetailRow
          label={t('fallback.details.confidence')}
          value={entry.quality?.confidence != null && entry.quality.confidence > 0
            ? `${Math.round(entry.quality.confidence * 100)}%`
            : null}
        />
        <DetailRow label={t('fallback.details.updated')} value={formatDate(entry.quality?.updatedAt)} />
        <DetailRow label={t('fallback.metrics.speed')} value={entry.speed?.source ?? null} />
        <DetailRow
          label={t('fallback.metrics.reliability')}
          value={entry.reliability?.successRate != null ? `${(entry.reliability.successRate * 100).toFixed(1)}%` : null}
        />
        <DetailRow label={t('fallback.details.rateLimitHits')} value={String(entry.rateLimitHits ?? 0)} />
      </DetailGroup>

      <section className="space-y-2 md:col-span-3">
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
      </section>
    </div>
  )
}
