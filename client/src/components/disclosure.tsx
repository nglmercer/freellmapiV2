import { useId, useState, type ReactNode } from 'react'
import { ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'

/**
 * The one expandable block used across the dashboard: a summary row that
 * toggles the detail body. Replaces native `<details>` and per-page toggles.
 */
export function Disclosure({
  summary,
  children,
  defaultOpen = false,
  onToggle,
  className,
  summaryClassName,
  toggleLabel,
}: {
  summary: ReactNode
  children?: ReactNode
  defaultOpen?: boolean
  onToggle?: (open: boolean) => void
  className?: string
  summaryClassName?: string
  /** Optional text for the trailing affordance; defaults to a chevron only. */
  toggleLabel?: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)
  const bodyId = useId()

  function toggle() {
    setOpen((current) => {
      onToggle?.(!current)
      return !current
    })
  }

  return (
    <div className={cn('min-w-0', className)}>
      {/* A div with role="button": summaries may hold block content, which a
          <button> cannot. Enter and Space toggle it, as a button would. */}
      <div
        role="button"
        tabIndex={0}
        onClick={toggle}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            toggle()
          }
        }}
        aria-expanded={open}
        aria-controls={bodyId}
        className={cn(
          'flex w-full cursor-pointer items-center gap-3 text-left transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring',
          summaryClassName,
        )}
      >
        <div className="min-w-0 flex-1">{summary}</div>
        {toggleLabel && (
          <span className="shrink-0 text-xs text-muted-foreground">{toggleLabel}</span>
        )}
        <ChevronDown
          aria-hidden
          className={cn('size-4 shrink-0 text-muted-foreground transition-transform', open && 'rotate-180')}
        />
      </div>
      {open && (
        <div id={bodyId} className="border-t">
          {children}
        </div>
      )}
    </div>
  )
}
