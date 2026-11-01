import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { PAGE_SIZE } from './filter-state'

export function CatalogPagination({
  total,
  offset,
  onChange,
}: {
  total: number
  offset: number
  onChange: (offset: number) => void
}) {
  const { t } = useTranslations()
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const page = Math.floor(offset / PAGE_SIZE) + 1
  const hasPrevious = offset > 0
  const hasNext = offset + PAGE_SIZE < total

  return (
    <nav
      aria-label={t('metadata.pagination')}
      className="flex flex-wrap items-center justify-between gap-3 border-t pt-4"
    >
      <p className="text-xs tabular-nums text-muted-foreground">
        {t('metadata.range', {
          from: total === 0 ? 0 : offset + 1,
          to: Math.min(offset + PAGE_SIZE, total),
          total,
        })}
      </p>
      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" disabled={!hasPrevious} onClick={() => onChange(Math.max(0, offset - PAGE_SIZE))}>
          {t('metadata.previous')}
        </Button>
        <span className="min-w-[6rem] text-center text-xs tabular-nums">
          {t('metadata.page', { page, pages })}
        </span>
        <Button variant="outline" size="sm" disabled={!hasNext} onClick={() => onChange(offset + PAGE_SIZE)}>
          {t('metadata.next')}
        </Button>
      </div>
    </nav>
  )
}
