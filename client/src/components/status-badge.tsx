import type { LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '@/lib/utils'
import { Tooltip } from './tooltip'

export type Tone = 'success' | 'warning' | 'danger' | 'info' | 'muted'

const toneClass: Record<Tone, string> = {
  success: 'border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  warning: 'border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  danger: 'border-red-500/40 bg-red-500/10 text-red-700 dark:text-red-300',
  info: 'border-sky-500/40 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  muted: 'border-border bg-muted/60 text-muted-foreground',
}

const dotClass: Record<Tone, string> = {
  success: 'bg-emerald-500',
  warning: 'bg-amber-500',
  danger: 'bg-red-500',
  info: 'bg-sky-500',
  muted: 'bg-muted-foreground/50',
}

/** One status label everywhere: a tone, a dot, and a short text. */
export function StatusBadge({
  tone = 'muted',
  children,
  className,
}: {
  tone?: Tone
  children: ReactNode
  className?: string
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border px-2 py-0.5 text-xs font-medium',
        toneClass[tone],
        className,
      )}
    >
      <span className={cn('size-1.5 rounded-full', dotClass[tone])} aria-hidden />
      {children}
    </span>
  )
}

/** Icon-only status: the label appears on hover and is the accessible name. */
export function StatusIcon({
  tone = 'muted',
  icon: Icon,
  label,
}: {
  tone?: Tone
  icon: LucideIcon
  label: string
}) {
  return (
    <Tooltip label={label}>
      <span
        role="img"
        aria-label={label}
        className={cn('inline-flex size-6 items-center justify-center rounded-full border', toneClass[tone])}
      >
        <Icon className="size-3.5" aria-hidden />
      </span>
    </Tooltip>
  )
}
