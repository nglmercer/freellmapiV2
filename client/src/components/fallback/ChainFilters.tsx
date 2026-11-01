import { useMemo } from 'react'
import { Search, X } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  EMPTY_FILTERS,
  hasFilters,
  providerLabel,
  type FallbackEntry,
  type FallbackFilters,
  type RankingFilter,
  type StateFilter,
  type TierFilter,
} from './chain-model'

const ALL_PLATFORMS = '__all__'

interface Item {
  value: string
  label: string
}

function FilterSelect({
  label,
  value,
  items,
  onValueChange,
}: {
  label: string
  value: string
  items: Item[]
  onValueChange: (value: string) => void
}) {
  return (
    <div className="min-w-0 space-y-1.5">
      <span className="block text-xs text-muted-foreground">{label}</span>
      {/* Base UI labels the trigger from `items`; without it the raw value shows until the menu opens. */}
      <Select
        items={items}
        value={value}
        onValueChange={(next) => {
          if (next != null) onValueChange(next)
        }}
      >
        <SelectTrigger className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {items.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

export function ChainFilters({
  entries,
  value,
  onChange,
}: {
  entries: FallbackEntry[]
  value: FallbackFilters
  onChange: (next: FallbackFilters) => void
}) {
  const { t } = useTranslations()

  const platformItems = useMemo<Item[]>(() => {
    const platforms = [...new Set(entries.map((e) => e.platform))].sort()
    return [
      { value: ALL_PLATFORMS, label: t('fallback.filters.platformAll') },
      ...platforms.map((p) => ({ value: p, label: providerLabel(p) })),
    ]
  }, [entries, t])

  const tierItems: Item[] = [
    { value: 'all', label: t('fallback.filters.tierAll') },
    { value: 'free', label: t('fallback.filters.tierFree') },
    { value: 'paid', label: t('fallback.filters.tierPaid') },
  ]
  const stateItems: Item[] = [
    { value: 'all', label: t('fallback.filters.stateAll') },
    { value: 'enabled', label: t('fallback.filters.stateEnabled') },
    { value: 'disabled', label: t('fallback.filters.stateDisabled') },
  ]
  const rankingItems: Item[] = [
    { value: 'all', label: t('fallback.filters.rankingAll') },
    { value: 'ranked', label: t('fallback.filters.rankingRanked') },
    { value: 'unranked', label: t('fallback.filters.rankingUnranked') },
    { value: 'high-confidence', label: t('fallback.filters.rankingHighConfidence') },
    { value: 'has-quality', label: t('fallback.filters.rankingHasQuality') },
    { value: 'has-speed', label: t('fallback.filters.rankingHasSpeed') },
    { value: 'local-performance', label: t('fallback.filters.rankingLocalPerformance') },
  ]

  function patch(next: Partial<FallbackFilters>) {
    onChange({ ...value, ...next })
  }

  return (
    <div className="space-y-3">
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-[minmax(0,2fr)_repeat(4,minmax(0,1fr))]">
        <div className="relative min-w-0 space-y-1.5">
          <span className="block text-xs text-muted-foreground">{t('fallback.filters.title')}</span>
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
            <Input
              type="search"
              aria-label={t('fallback.filters.search')}
              placeholder={t('fallback.filters.search')}
              value={value.search}
              onChange={(e) => patch({ search: e.target.value })}
              className="pl-8"
            />
          </div>
        </div>
        <FilterSelect
          label={t('fallback.filters.platform')}
          value={value.platform || ALL_PLATFORMS}
          items={platformItems}
          onValueChange={(next) => patch({ platform: next === ALL_PLATFORMS ? '' : next })}
        />
        <FilterSelect
          label={t('fallback.filters.tier')}
          value={value.tier}
          items={tierItems}
          onValueChange={(next) => patch({ tier: next as TierFilter })}
        />
        <FilterSelect
          label={t('fallback.filters.state')}
          value={value.state}
          items={stateItems}
          onValueChange={(next) => patch({ state: next as StateFilter })}
        />
        <FilterSelect
          label={t('fallback.filters.ranking')}
          value={value.ranking}
          items={rankingItems}
          onValueChange={(next) => patch({ ranking: next as RankingFilter })}
        />
      </div>

      {hasFilters(value) && (
        <div className="flex justify-end">
          <Button variant="ghost" size="sm" onClick={() => onChange(EMPTY_FILTERS)} className="gap-1">
            <X className="size-3.5" aria-hidden />
            {t('fallback.filters.clear')}
          </Button>
        </div>
      )}
    </div>
  )
}
