import type { LucideIcon } from 'lucide-react'
import { Check } from 'lucide-react'
import { cn } from '@/lib/utils'

export interface StepperItem {
  id: string
  label: string
  icon: LucideIcon
  state: 'done' | 'active' | 'todo'
  disabled?: boolean
}

/** Horizontal steps with a connector line. Each step shows its icon. */
export function Stepper({
  items,
  onSelect,
  label,
}: {
  items: StepperItem[]
  onSelect: (id: string) => void
  label: string
}) {
  return (
    <nav aria-label={label}>
      <ol className="flex items-center gap-2">
        {items.map((item, index) => {
          const Icon = item.icon
          return (
            <li key={item.id} className="flex min-w-0 flex-1 items-center gap-2 last:flex-none">
              <button
                type="button"
                onClick={() => onSelect(item.id)}
                disabled={item.disabled}
                aria-current={item.state === 'active' ? 'step' : undefined}
                className="flex min-w-0 items-center gap-2 rounded-full text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:opacity-40"
              >
                <span
                  className={cn(
                    'flex size-7 shrink-0 items-center justify-center rounded-full border transition-colors',
                    item.state === 'active' && 'border-foreground bg-foreground text-background',
                    item.state === 'done' && 'border-emerald-500/60 text-emerald-600 dark:text-emerald-400',
                    item.state === 'todo' && 'border-border text-muted-foreground',
                  )}
                >
                  {item.state === 'done' ? <Check className="size-3.5" aria-hidden /> : <Icon className="size-3.5" aria-hidden />}
                </span>
                <span
                  className={cn(
                    'truncate',
                    item.state === 'active' ? 'font-medium text-foreground' : 'text-muted-foreground',
                  )}
                >
                  {item.label}
                </span>
              </button>
              {index < items.length - 1 && <span className="h-px flex-1 bg-border" aria-hidden />}
            </li>
          )
        })}
      </ol>
    </nav>
  )
}
