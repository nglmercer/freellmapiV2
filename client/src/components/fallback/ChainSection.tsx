import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { ChainList } from './ChainList'
import { SaveStatus } from './SaveStatus'
import type { FallbackChain } from './use-fallback-chain'

/** The chain's list, or the empty state that explains why there is nothing to show. */
export function ChainSection({
  chain,
  onConfigure,
}: {
  chain: FallbackChain
  onConfigure: () => void
}) {
  const { t } = useTranslations()

  if (chain.isLoading) {
    return (
      <div className="space-y-2" aria-busy="true" aria-label={t('fallback.loading')}>
        {[0, 1, 2].map((i) => (
          <div key={i} className="h-16 animate-pulse rounded-xl border bg-card" />
        ))}
      </div>
    )
  }

  if (chain.configured.length === 0 && !chain.showUnconfigured) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed p-10 text-center">
        <p className="text-sm text-muted-foreground">{t('fallback.emptyState')}</p>
        <Button onClick={onConfigure}>{t('fallback.configureProviders')}</Button>
      </div>
    )
  }

  if (chain.visible.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed p-8 text-center">
        <p className="text-sm text-muted-foreground">{t('fallback.filters.empty')}</p>
        <Button variant="outline" size="sm" onClick={chain.resetFilters}>
          {t('fallback.filters.clear')}
        </Button>
      </div>
    )
  }

  return (
    <section className="space-y-3">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-medium">{t('fallback.order.title')}</h2>
        <SaveStatus status={chain.saveStatus} onRetry={chain.retrySave} />
      </header>
      <ChainList
        entries={chain.visible}
        maxSpeed={chain.maxSpeed}
        onReorder={chain.reorder}
        onToggle={chain.toggle}
      />
    </section>
  )
}
