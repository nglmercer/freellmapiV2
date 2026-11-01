import { cn } from '@/lib/utils'

export interface SummaryItem {
  label: string
  /** Formatted value. Items with no value are skipped unless `hint` is set. */
  value: string | null
  /** Shown in place of the value when there is none. */
  hint?: string
}

/**
 * A compact row of label/value pairs for summaries in headers and previews.
 * One component for every "quality 71.0  speed 120 tok/s" style line.
 */
export function KeyValueSummary({
  items,
  className,
  emptyLabel,
}: {
  items: SummaryItem[]
  className?: string
  /** Text shown when no item has a value. */
  emptyLabel?: string
}) {
  const visible = items.filter((item) => item.value != null || item.hint)
  if (visible.length === 0) {
    return emptyLabel ? <span className="text-xs text-muted-foreground">{emptyLabel}</span> : null
  }
  return (
    <dl className={cn('flex flex-wrap gap-x-5 gap-y-1 text-xs tabular-nums', className)}>
      {visible.map((item) => (
        <div key={item.label} className="flex items-baseline gap-1.5">
          <dt className="text-muted-foreground">{item.label}</dt>
          {item.value != null ? (
            <dd className="font-mono">{item.value}</dd>
          ) : (
            <dd className="text-muted-foreground">{item.hint}</dd>
          )}
        </div>
      ))}
    </dl>
  )
}
