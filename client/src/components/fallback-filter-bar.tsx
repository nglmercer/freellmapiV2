import { useId, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Search, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  EMPTY_FILTERS,
  type FallbackEntry,
  type FallbackFilters,
  type RankingFilter,
  type StateFilter,
  type TierFilter,
} from './fallback-filters'

interface FallbackFilterBarProps {
  entries: FallbackEntry[]
  filters: FallbackFilters
  onFiltersChange: (next: FallbackFilters) => void
}

export function FallbackFilterBar({ entries, filters, onFiltersChange }: FallbackFilterBarProps) {
  const { t } = useTranslation()
  const baseId = useId()

  const platforms = useMemo(() => {
    const set = new Set<string>()
    for (const e of entries) set.add(e.platform)
    return [...set].sort()
  }, [entries])

  // Base UI labels a trigger from `items`; without it the raw value (e.g. "__all__") shows until the menu opens.
  const platformItems = useMemo(
    () => [
      { value: '__all__', label: t('fallback.filters.platformAll') },
      ...platforms.map((p) => ({ value: p, label: p })),
    ],
    [platforms, t],
  )
  const tierItems = useMemo(
    () => [
      { value: 'all', label: t('fallback.filters.tierAll') },
      { value: 'free', label: t('fallback.filters.tierFree') },
      { value: 'paid', label: t('fallback.filters.tierPaid') },
    ],
    [t],
  )
  const stateItems = useMemo(
    () => [
      { value: 'all', label: t('fallback.filters.stateAll') },
      { value: 'enabled', label: t('fallback.filters.stateEnabled') },
      { value: 'disabled', label: t('fallback.filters.stateDisabled') },
    ],
    [t],
  )
  const rankingItems = useMemo(
    () => [
      { value: 'all', label: t('fallback.filters.rankingAll') },
      { value: 'ranked', label: t('fallback.filters.rankingRanked') },
      { value: 'unranked', label: t('fallback.filters.rankingUnranked') },
      { value: 'high-confidence', label: t('fallback.filters.rankingHighConfidence') },
      { value: 'has-quality', label: t('fallback.filters.rankingHasQuality') },
      { value: 'has-speed', label: t('fallback.filters.rankingHasSpeed') },
      { value: 'local-performance', label: t('fallback.filters.rankingLocalPerformance') },
    ],
    [t],
  )

  const hasActiveFilters =
    filters.search !== '' ||
    filters.platform !== '' ||
    filters.tier !== 'all' ||
    filters.state !== 'all' ||
    filters.ranking !== 'all'

  function patch(next: Partial<FallbackFilters>) {
    onFiltersChange({ ...filters, ...next })
  }

  return (
    <div className="flex flex-wrap items-end gap-3">
      <div className="flex-1 min-w-48 space-y-1.5">
        <Label htmlFor={`${baseId}-search`} className="text-xs text-muted-foreground">
          {t('fallback.filters.search')}
        </Label>
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-muted-foreground pointer-events-none" />
          <Input
            id={`${baseId}-search`}
            value={filters.search}
            onChange={(e) => patch({ search: e.target.value })}
            placeholder={t('fallback.filters.search')}
            className="pl-8"
          />
        </div>
      </div>

      <div className="space-y-1.5 min-w-40">
        <Label className="text-xs text-muted-foreground">{t('fallback.filters.platform')}</Label>
        <Select
          items={platformItems}
          value={filters.platform || '__all__'}
          onValueChange={(v) => patch({ platform: v && v !== '__all__' ? v : '' })}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('fallback.filters.platformAll')} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t('fallback.filters.platformAll')}</SelectItem>
            {platforms.map((p) => (
              <SelectItem key={p} value={p}>
                {p}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-1.5 min-w-32">
        <Label className="text-xs text-muted-foreground">{t('fallback.filters.tier')}</Label>
        <Select
          items={tierItems}
          value={filters.tier}
          onValueChange={(v) => patch({ tier: v as TierFilter })}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('fallback.filters.tierAll')}</SelectItem>
            <SelectItem value="free">{t('fallback.filters.tierFree')}</SelectItem>
            <SelectItem value="paid">{t('fallback.filters.tierPaid')}</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-1.5 min-w-32">
        <Label className="text-xs text-muted-foreground">{t('fallback.filters.state')}</Label>
        <Select
          items={stateItems}
          value={filters.state}
          onValueChange={(v) => patch({ state: v as StateFilter })}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('fallback.filters.stateAll')}</SelectItem>
            <SelectItem value="enabled">{t('fallback.filters.stateEnabled')}</SelectItem>
            <SelectItem value="disabled">{t('fallback.filters.stateDisabled')}</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="space-y-1.5 min-w-32">
        <Label className="text-xs text-muted-foreground">{t('fallback.filters.ranking')}</Label>
        <Select
          items={rankingItems}
          value={filters.ranking}
          onValueChange={(v) => patch({ ranking: v as RankingFilter })}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t('fallback.filters.rankingAll')}</SelectItem>
            <SelectItem value="ranked">{t('fallback.filters.rankingRanked')}</SelectItem>
            <SelectItem value="unranked">{t('fallback.filters.rankingUnranked')}</SelectItem>
            <SelectItem value="high-confidence">{t('fallback.filters.rankingHighConfidence')}</SelectItem>
            <SelectItem value="has-quality">{t('fallback.filters.rankingHasQuality')}</SelectItem>
            <SelectItem value="has-speed">{t('fallback.filters.rankingHasSpeed')}</SelectItem>
            <SelectItem value="local-performance">{t('fallback.filters.rankingLocalPerformance')}</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {hasActiveFilters && (
        <Button variant="ghost" size="sm" onClick={() => onFiltersChange(EMPTY_FILTERS)} className="gap-1">
          <X className="size-3.5" />
          {t('fallback.filters.clear')}
        </Button>
      )}
    </div>
  )
}
