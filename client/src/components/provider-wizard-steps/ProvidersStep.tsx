import { CircleCheck, Plus, Search, Zap } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import type { PlatformInfo } from '@/lib/platforms'
import { Input } from '@/components/ui/input'
import { Tooltip } from '../tooltip'

interface ProvidersStepProps {
  search: string
  onSearchChange: (value: string) => void
  /** Number of platforms matching the current search. */
  matchCount: number
  configured: PlatformInfo[]
  available: PlatformInfo[]
  keyCounts: Map<string, number>
  onChoose: (platform: PlatformInfo) => void
}

/** Step 1: pick a provider, split into configured and available. */
export function ProvidersStep({
  search,
  onSearchChange,
  matchCount,
  configured,
  available,
  keyCounts,
  onChoose,
}: ProvidersStepProps) {
  const { t } = useTranslations()
  return (
    <div className="space-y-4">
      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
        <Input
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder={t('wizard.search')}
          aria-label={t('wizard.search')}
          className="pl-9"
        />
      </div>
      {matchCount === 0 && (
        <p className="py-6 text-center text-sm text-muted-foreground">{t('wizard.noMatch')}</p>
      )}
      <ProviderSection
        title={t('wizard.configured')}
        items={configured}
        keyCounts={keyCounts}
        onChoose={onChoose}
      />
      <ProviderSection
        title={t('wizard.available')}
        items={available}
        keyCounts={keyCounts}
        onChoose={onChoose}
      />
    </div>
  )
}

function ProviderSection({
  title,
  items,
  keyCounts,
  onChoose,
}: {
  title: string
  items: PlatformInfo[]
  keyCounts: Map<string, number>
  onChoose: (platform: PlatformInfo) => void
}) {
  const { t } = useTranslations()
  if (items.length === 0) return null
  return (
    <section className="space-y-2">
      <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        <span>{title}</span>
        <span className="tabular-nums">{items.length}</span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        {items.map((item) => {
          const count = keyCounts.get(item.value) ?? 0
          return (
            <button
              key={item.value}
              type="button"
              onClick={() => onChoose(item)}
              className="flex h-11 items-center gap-3 rounded-lg border bg-background px-3 text-left transition-colors hover:border-foreground/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{item.label}</span>
              {count > 0 && (
                <Tooltip label={t('wizard.keyCount', { count })}>
                  <span
                    role="img"
                    aria-label={t('wizard.keyCount', { count })}
                    className="inline-flex items-center gap-1 text-xs tabular-nums text-emerald-600 dark:text-emerald-400"
                  >
                    <CircleCheck className="size-4" aria-hidden />
                    {count}
                  </span>
                </Tooltip>
              )}
              {count === 0 && item.anonymous && (
                <Tooltip label={t('wizard.anonymousShort')}>
                  <Zap className="size-4 text-muted-foreground" aria-label={t('wizard.anonymousShort')} />
                </Tooltip>
              )}
              {count === 0 && !item.anonymous && (
                <Tooltip label={t('wizard.add')}>
                  <Plus className="size-4 text-muted-foreground" aria-label={t('wizard.add')} />
                </Tooltip>
              )}
            </button>
          )
        })}
      </div>
    </section>
  )
}
