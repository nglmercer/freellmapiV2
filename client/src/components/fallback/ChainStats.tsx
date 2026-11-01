import { useTranslations } from '@/hooks/useTranslations'
import { SummaryStats } from '@/components/summary-stats'
import { hasQuality, hasSpeed, type FallbackEntry } from './chain-model'

function percent(part: number, total: number): string {
  return `${total > 0 ? Math.round((part / total) * 100) : 0}%`
}

/** Headline numbers for the chain. Empty scores say what would fill them. */
export function ChainStats({ entries }: { entries: FallbackEntry[] }) {
  const { t } = useTranslations()
  const total = entries.length
  const enabled = entries.filter((e) => e.enabled).length
  const free = entries.filter((e) => e.freeTier).length
  const quality = entries.filter(hasQuality).length
  const speed = entries.filter(hasSpeed).length
  const ratio = (n: number) => (total > 0 ? n / total : 0)

  return (
    <SummaryStats
      items={[
        {
          label: t('fallback.stats.inChain'),
          value: total,
          detail: t('fallback.stats.freeCount', { count: free }),
        },
        {
          label: t('fallback.stats.enabled'),
          value: enabled,
          detail: percent(enabled, total),
          ratio: ratio(enabled),
        },
        {
          label: t('fallback.stats.qualityScored'),
          value: quality,
          detail: quality === 0 ? t('fallback.stats.addKey') : percent(quality, total),
          ratio: ratio(quality),
          tone: 'quality',
        },
        {
          label: t('fallback.stats.speedMeasured'),
          value: speed,
          detail: speed === 0 ? t('fallback.short.awaiting') : percent(speed, total),
          ratio: ratio(speed),
          tone: 'speed',
        },
      ]}
    />
  )
}
