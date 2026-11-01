import { X } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { MAX_COMPARE } from './filter-state'
import type { CompareItem } from './use-explorer-state'

/** Floating selection bar: what is picked, and the way into the comparison. */
export function CompareTray({
  items,
  onRemove,
  onClear,
  onCompare,
}: {
  items: CompareItem[]
  onRemove: (id: number) => void
  onClear: () => void
  onCompare: () => void
}) {
  const { t } = useTranslations()
  if (items.length === 0) return null

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-40 px-4">
      <section
        aria-label={t('metadata.stats.comparing', { count: items.length, max: MAX_COMPARE })}
        className="pointer-events-auto mx-auto flex max-w-4xl flex-wrap items-center gap-3 rounded-xl border bg-popover/95 p-3 shadow-lg backdrop-blur supports-[backdrop-filter]:bg-popover/80"
      >
        <span className="shrink-0 text-sm font-medium tabular-nums">
          {t('metadata.stats.comparing', { count: items.length, max: MAX_COMPARE })}
        </span>
        <ul className="flex min-w-0 flex-1 flex-wrap gap-1.5">
          {items.map((item) => (
            <li
              key={item.id}
              className="inline-flex max-w-full items-center gap-1 rounded-full bg-secondary py-0.5 pl-2.5 pr-1 text-xs text-secondary-foreground"
            >
              <span className="truncate font-mono">{item.label}</span>
              <button
                type="button"
                onClick={() => onRemove(item.id)}
                aria-label={t('metadata.removeCompare', { label: item.label })}
                className="shrink-0 rounded-full p-0.5 hover:bg-foreground/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X className="size-3" aria-hidden />
              </button>
            </li>
          ))}
        </ul>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onClear}>
            {t('metadata.stats.clearComparison')}
          </Button>
          <Button size="sm" disabled={items.length < 2} onClick={onCompare}>
            {t('metadata.compareSelected')}
          </Button>
        </div>
      </section>
    </div>
  )
}
