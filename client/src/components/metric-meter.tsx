import { cn } from '@/lib/utils'
import { clamp01 } from './fallback-format'

export type MeterTone = 'quality' | 'speed' | 'reliability' | 'neutral'

const barClass: Record<MeterTone, string> = {
  quality: 'bg-violet-500',
  speed: 'bg-emerald-500',
  reliability: 'bg-sky-500',
  neutral: 'bg-muted-foreground/50',
}

/**
 * One metric: a label, its value, and a bar. When there is no value, the hint
 * says what would fill it, so the cell never shows a bare dash.
 */
export function MetricMeter({
  label,
  value,
  ratio,
  hint,
  tone = 'neutral',
  title,
}: {
  label: string
  value: string | null
  ratio?: number | null
  hint?: string
  tone?: MeterTone
  title?: string
}) {
  const empty = value == null
  return (
    <div className="min-w-0 space-y-1" title={title}>
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{label}</div>
      {empty ? (
        <div className="text-xs leading-snug text-muted-foreground">{hint}</div>
      ) : (
        <div className="font-mono text-sm tabular-nums truncate">{value}</div>
      )}
      <div className="h-1.5 overflow-hidden rounded-full bg-muted" aria-hidden>
        <div
          className={cn('h-full rounded-full transition-[width] duration-300', barClass[tone], empty && 'opacity-0')}
          style={{ width: `${Math.round(clamp01(ratio ?? 0) * 100)}%` }}
        />
      </div>
    </div>
  )
}
