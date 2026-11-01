import type { ReactNode } from 'react'
import { Menu } from '@base-ui/react/menu'
import { ChevronDown } from 'lucide-react'
import { buttonVariants } from '@/components/ui/button'
import { cn } from '@/lib/utils'

/**
 * A button that opens a list of actions. Built on base-ui's menu, so arrow
 * keys, Escape and focus return work the same as the rest of the dashboard.
 */
export function ActionMenu({
  label,
  disabled,
  children,
}: {
  label: ReactNode
  disabled?: boolean
  children: ReactNode
}) {
  return (
    <Menu.Root>
      <Menu.Trigger
        disabled={disabled}
        className={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'gap-1')}
      >
        {label}
        <ChevronDown className="size-3.5" aria-hidden />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner sideOffset={6} align="end" className="z-50">
          <Menu.Popup className="min-w-52 rounded-lg border bg-popover p-1 text-sm text-popover-foreground shadow-md outline-none">
            {children}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  )
}

export function ActionMenuItem({
  onSelect,
  tone = 'default',
  children,
}: {
  onSelect: () => void
  tone?: 'default' | 'danger'
  children: ReactNode
}) {
  return (
    <Menu.Item
      onClick={onSelect}
      className={cn(
        'flex w-full cursor-default select-none items-center rounded-md px-2.5 py-1.5 outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground',
        tone === 'danger' && 'text-destructive data-[highlighted]:text-destructive',
      )}
    >
      {children}
    </Menu.Item>
  )
}

export function ActionMenuSeparator() {
  return <Menu.Separator className="my-1 h-px bg-border" />
}
