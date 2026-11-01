import { Search, X } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { cn } from '@/lib/utils'
import {
  CAPABILITY_FILTERS,
  CONTEXT_PRESETS,
  SORT_OPTIONS,
  type CatalogFilters,
} from './catalog-filter-state'

export function ModelFilters({
  value,
  onChange,
  onClear,
}: {
  value: CatalogFilters
  onChange: (patch: Partial<CatalogFilters>) => void
  onClear: () => void
}) {
  const { t } = useTranslations()
  const active: { key: string; label: string; clear: () => void }[] = []
  if (value.query) active.push({ key: 'query', label: `"${value.query}"`, clear: () => onChange({ query: '' }) })
  if (value.provider) active.push({ key: 'provider', label: value.provider, clear: () => onChange({ provider: '' }) })
  if (value.capability)
    active.push({
      key: 'capability',
      label: t(`fallback.capability.${value.capability}`, { defaultValue: value.capability }),
      clear: () => onChange({ capability: '' }),
    })
  if (value.free) active.push({ key: 'free', label: t('metadata.free'), clear: () => onChange({ free: false }) })
  if (value.context)
    active.push({
      key: 'context',
      label: `≥ ${Number(value.context).toLocaleString()} ${t('metadata.context').toLowerCase()}`,
      clear: () => onChange({ context: '' }),
    })

  return (
    <section className="space-y-4 rounded-lg border bg-card p-4">
      <div className="grid gap-3 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,1fr)]">
        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input
            className="pl-9"
            aria-label={t('metadata.search')}
            placeholder={t('metadata.search')}
            value={value.query}
            onChange={(e) => onChange({ query: e.target.value })}
          />
        </div>
        <Input
          aria-label={t('metadata.provider')}
          placeholder={t('metadata.provider')}
          value={value.provider}
          onChange={(e) => onChange({ provider: e.target.value })}
        />
        <select
          aria-label={t('metadata.sort')}
          className="h-9 min-w-0 rounded-md border bg-background px-2 text-sm"
          value={value.sort}
          onChange={(e) => onChange({ sort: e.target.value })}
        >
          {SORT_OPTIONS.map((s) => (
            <option key={s} value={s}>
              {t(`metadata.${s}`)}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            {t('metadata.minContext')}
          </div>
          <div role="group" aria-label={t('metadata.minContext')} className="inline-flex rounded-md border bg-background p-0.5">
            {CONTEXT_PRESETS.map((preset) => (
              <button
                key={preset.value || 'any'}
                type="button"
                aria-pressed={value.context === preset.value}
                onClick={() => onChange({ context: preset.value })}
                className={cn(
                  'rounded px-2.5 py-1 text-xs font-medium transition-colors',
                  value.context === preset.value
                    ? 'bg-foreground text-background'
                    : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {preset.value ? preset.label : t('metadata.any')}
              </button>
            ))}
          </div>
        </div>

        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            {t('metadata.capability')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {CAPABILITY_FILTERS.map((capability) => {
              const on = value.capability === capability
              return (
                <button
                  key={capability}
                  type="button"
                  aria-pressed={on}
                  onClick={() => onChange({ capability: on ? '' : capability })}
                  className="rounded-full focus-visible:outline-none"
                >
                  <Badge variant={on ? 'default' : 'outline'} className="cursor-pointer">
                    {t(`fallback.capability.${capability}`, { defaultValue: capability })}
                  </Badge>
                </button>
              )
            })}
          </div>
        </div>

        <label className="flex items-center gap-2 text-sm">
          <Switch checked={value.free} onCheckedChange={(checked) => onChange({ free: checked })} />
          {t('metadata.free')}
        </label>
      </div>

      {active.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 border-t pt-3">
          <span className="text-xs text-muted-foreground">{t('fallback.filters.title')}:</span>
          {active.map((chip) => (
            <Badge key={chip.key} variant="secondary" className="gap-1 pr-1">
              {chip.label}
              <button
                type="button"
                onClick={chip.clear}
                aria-label={`${t('fallback.filters.clear')}: ${chip.label}`}
                className="rounded-full p-0.5 hover:bg-foreground/10"
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))}
          <Button variant="ghost" size="sm" onClick={onClear} className="h-7">
            {t('fallback.filters.clear')}
          </Button>
        </div>
      )}
    </section>
  )
}
