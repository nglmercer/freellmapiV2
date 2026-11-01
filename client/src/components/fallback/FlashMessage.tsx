import { cn } from '@/lib/utils'
import type { Flash } from './use-flash'

export function FlashMessage({ flash }: { flash: Flash }) {
  return (
    <div
      role={flash.tone === 'error' ? 'alert' : 'status'}
      className={cn(
        'rounded-md border px-3 py-2 text-sm',
        flash.tone === 'success'
          ? 'border-emerald-300 bg-emerald-50 text-emerald-900 dark:border-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-100'
          : 'border-red-300 bg-red-50 text-red-900 dark:border-red-700 dark:bg-red-950/30 dark:text-red-100',
      )}
    >
      {flash.message}
    </div>
  )
}
