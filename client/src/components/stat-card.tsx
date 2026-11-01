import { cn } from '@/lib/utils'
import { clamp01 } from './fallback-format'
import type { MeterTone } from './metric-meter'

export function StatCard({
  label,
  value,
  detail,
  ratio,
  tone = 'neutral',
}: {
  label: string
  value: number | string
  detail?: string
  ratio?: number
  tone?: MeterTone
}) {
  const barClass: Record<MeterTone, string> = {
    quality: 'bg-violet-500',
    speed: 'bg-emerald-500',
    reliability: 'bg-sky-500',
    neutral: 'bg-foreground/60',
  }
  return (
    <div className="rounded-lg border bg-card p-4">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 flex items-baseline gap-2">
        <span className="text-2xl font-semibold tabular-nums">{value}</span>
        <span className="text-xs text-muted-foreground tabular-nums">{detail}</span>
      </div>
      {ratio !== undefined && (
        <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden>
          <div
            className={cn('h-full rounded-full transition-[width] duration-300', barClass[tone])}
            style={{ width: `${Math.round(clamp01(ratio) * 100)}%` }}
          />
        </div>
      )}
    </div>
  )
}
