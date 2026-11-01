import { StatCard } from './stat-card'
import type { MeterTone } from './metric-meter'

export interface SummaryStatItem {
  label: string
  value: number | string
  detail?: string
  /** 0–1; draws a bar under the value when set. */
  ratio?: number
  tone?: MeterTone
}

/** The one stat grid used by the chain and the model catalog. */
export function SummaryStats({ items }: { items: SummaryStatItem[] }) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {items.map((item) => (
        <StatCard
          key={item.label}
          label={item.label}
          value={item.value}
          detail={item.detail}
          ratio={item.ratio}
          tone={item.tone}
        />
      ))}
    </div>
  )
}
