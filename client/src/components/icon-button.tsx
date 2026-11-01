import type { LucideIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { Tooltip } from './tooltip'

/**
 * An icon-only action. The label is both the accessible name and the hover
 * tooltip, so an icon never appears without a description.
 */
export function IconButton({
  label,
  icon: Icon,
  onClick,
  disabled,
  busy,
  variant = 'ghost',
  tone,
  type = 'button',
  className,
}: {
  label: string
  icon: LucideIcon
  onClick?: () => void
  disabled?: boolean
  /** Shows a spinner in place of the icon and disables the button. */
  busy?: boolean
  variant?: 'ghost' | 'outline' | 'secondary'
  tone?: 'danger'
  type?: 'button' | 'submit'
  className?: string
}) {
  return (
    <Tooltip label={label}>
      <Button
        type={type}
        variant={variant}
        size="icon-sm"
        aria-label={label}
        disabled={disabled || busy}
        onClick={onClick}
        className={cn(tone === 'danger' && 'text-muted-foreground hover:text-destructive', className)}
      >
        <Icon className={cn('size-4', busy && 'animate-spin')} aria-hidden />
      </Button>
    </Tooltip>
  )
}
