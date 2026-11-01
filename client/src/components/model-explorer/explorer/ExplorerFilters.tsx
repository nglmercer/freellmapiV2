import { Search, X } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import {
  CAPABILITY_FILTERS,
  CONTEXT_PRESETS,
  SORT_OPTIONS,
  countActiveFilters,
  type CatalogFilters,
} from './filter-state'
import { formatTokens } from '../format'

const GROUP_LABEL = 'text-[11px] font-medium uppercase tracking-wide text-muted-foreground'
const SEGMENT_BASE = 'rounded-md px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'

export function ExplorerFilters({
  value,
  onChange,
  onClear,
}: {
  value: CatalogFilters
  onChange: (patch: Partial<CatalogFilters>) => void
  onClear: () => void
}) {
  const { t } = useTranslations()
  const activeCount = countActiveFilters(value)

  const chips: { key: string; label: string; remove: () => void }[] = []
  if (value.query) {
    chips.push({
      key: 'query',
      label: t('metadata.chips.query', { value: value.query }),
      remove: () => onChange({ query: '' }),
    })
  }
  if (value.provider) {
    chips.push({
      key: 'provider',
      label: t('metadata.chips.provider', { value: value.provider }),
      remove: () => onChange({ provider: '' }),
    })
  }
  if (value.capability) {
    chips.push({
      key: 'capability',
      label: t('metadata.chips.capability', {
        value: t(`fallback.capability.${value.capability}`, { defaultValue: value.capability }),
      }),
      remove: () => onChange({ capability: '' }),
    })
  }
  if (value.context) {
    chips.push({
      key: 'context',
      label: t('metadata.chips.context', { value: formatTokens(Number(value.context)) }),
      remove: () => onChange({ context: '' }),
    })
  }
  if (value.free) {
    chips.push({ key: 'free', label: t('metadata.free'), remove: () => onChange({ free: false }) })
  }

  return (
    <section aria-label={t('metadata.filters.title')} className="space-y-4 rounded-xl border bg-card p-4">
      <div className="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,14rem)_auto] lg:items-center">
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input
            type="search"
            className="h-9 pl-9"
            aria-label={t('metadata.search')}
            placeholder={t('metadata.search')}
            value={value.query}
            onChange={(e) => onChange({ query: e.target.value })}
          />
        </div>
        <Input
          className="h-9"
          aria-label={t('metadata.provider')}
          placeholder={t('metadata.provider')}
          value={value.provider}
          onChange={(e) => onChange({ provider: e.target.value })}
        />
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <span className="shrink-0">{t('metadata.sort')}</span>
          <select
            className="h-9 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm text-foreground lg:w-44 lg:flex-none"
            value={value.sort}
            onChange={(e) => onChange({ sort: e.target.value })}
          >
            {SORT_OPTIONS.map((sort) => (
              <option key={sort} value={sort}>
                {t(`metadata.${sort}`)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="flex flex-wrap items-end gap-x-8 gap-y-4">
        <div className="space-y-1.5">
          <div className={GROUP_LABEL}>{t('metadata.minContext')}</div>
          <div role="group" aria-label={t('metadata.minContext')} className="inline-flex rounded-md border bg-background p-0.5">
            {CONTEXT_PRESETS.map((preset) => {
              const on = value.context === preset.value
              return (
                <button
                  key={preset.value || 'any'}
                  type="button"
                  aria-pressed={on}
                  onClick={() => onChange({ context: preset.value })}
                  className={cn(
                    SEGMENT_BASE,
                    on ? 'bg-foreground text-background' : 'text-muted-foreground hover:text-foreground',
                  )}
                >
                  {preset.value ? preset.label : t('metadata.any')}
                </button>
              )
            })}
          </div>
        </div>

        <div className="space-y-1.5">
          <div className={GROUP_LABEL}>{t('metadata.capability')}</div>
          <div role="group" aria-label={t('metadata.capability')} className="flex flex-wrap gap-1.5">
            {CAPABILITY_FILTERS.map((capability) => {
              const on = value.capability === capability
              return (
                <button
                  key={capability}
                  type="button"
                  aria-pressed={on}
                  onClick={() => onChange({ capability: on ? '' : capability })}
                  className={cn(
                    'rounded-full border px-3 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                    on
                      ? 'border-foreground bg-foreground text-background'
                      : 'border-border text-muted-foreground hover:border-foreground/40 hover:text-foreground',
                  )}
                >
                  {t(`fallback.capability.${capability}`, { defaultValue: capability })}
                </button>
              )
            })}
          </div>
        </div>

        <label className="flex items-center gap-2 pb-1 text-sm">
          <Switch checked={value.free} onCheckedChange={(checked) => onChange({ free: checked })} />
          {t('metadata.free')}
        </label>
      </div>

      {activeCount > 0 && (
        <div className="flex flex-wrap items-center gap-2 border-t pt-3">
          <span className="text-xs font-medium text-muted-foreground">
            {t('metadata.filters.active', { count: activeCount })}
          </span>
          {chips.map((chip) => (
            <span
              key={chip.key}
              className="inline-flex items-center gap-1 rounded-full bg-secondary py-0.5 pl-2.5 pr-1 text-xs font-medium text-secondary-foreground"
            >
              {chip.label}
              <button
                type="button"
                onClick={chip.remove}
                aria-label={t('metadata.filters.remove', { label: chip.label })}
                className="rounded-full p-0.5 hover:bg-foreground/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <X className="size-3" aria-hidden />
              </button>
            </span>
          ))}
          <Button variant="ghost" size="sm" onClick={onClear} className="ml-auto h-7">
            {t('metadata.filters.clearAll')}
          </Button>
        </div>
      )}
    </section>
  )
}
