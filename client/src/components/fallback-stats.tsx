import { useTranslations } from '@/hooks/useTranslations'
import type { FallbackEntry } from '../../../shared/types'
import { hasQuality, hasSpeed } from './fallback-filters'
import { StatCard } from './stat-card'

/** Totals for the chain: how many entries exist, are enabled, and have data. */
export function FallbackStats({ entries }: { entries: FallbackEntry[] }) {
  const { t } = useTranslations()
  const total = entries.length
  const enabled = entries.filter((e) => e.enabled).length
  const qualityScored = entries.filter(hasQuality).length
  const speedMeasured = entries.filter(hasSpeed).length
  const free = entries.filter((e) => e.freeTier).length
  const share = (count: number) => (total > 0 ? Math.round((count / total) * 100) : 0)

  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <StatCard
        label={t('fallback.stats.inChain')}
        value={total}
        detail={t('fallback.stats.freeCount', { count: free })}
      />
      <StatCard
        label={t('fallback.stats.enabled')}
        value={enabled}
        detail={`${share(enabled)}%`}
        ratio={total > 0 ? enabled / total : 0}
      />
      <StatCard
        label={t('fallback.stats.qualityScored')}
        value={qualityScored}
        detail={`${share(qualityScored)}%`}
        ratio={total > 0 ? qualityScored / total : 0}
        tone="quality"
      />
      <StatCard
        label={t('fallback.stats.speedMeasured')}
        value={speedMeasured}
        detail={`${share(speedMeasured)}%`}
        ratio={total > 0 ? speedMeasured / total : 0}
        tone="speed"
      />
    </div>
  )
}
