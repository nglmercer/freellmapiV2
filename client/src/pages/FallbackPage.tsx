import { useMemo } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { PageHeader } from '@/components/page-header'
import { useProviderWizard } from '@/lib/provider-wizard-context'
import { ChainStats } from '@/components/fallback/ChainStats'
import { ChainToolbar } from '@/components/fallback/ChainToolbar'
import { HiddenNotice } from '@/components/fallback/HiddenNotice'
import { TokenBudget } from '@/components/fallback/TokenBudget'
import { ChainSection } from '@/components/fallback/ChainSection'
import { FlashMessage } from '@/components/fallback/FlashMessage'
import { useFlash } from '@/components/fallback/use-flash'
import { useFallbackChain } from '@/components/fallback/use-fallback-chain'
import { providerLabel } from '@/components/fallback/chain-model'

export default function FallbackPage() {
  const { t } = useTranslations()
  const wizard = useProviderWizard()
  const { flash, notify } = useFlash()
  const chain = useFallbackChain(notify)

  const hiddenProviders = useMemo(
    () => [...new Set(chain.unconfigured.map((e) => providerLabel(e.platform)))].join(', '),
    [chain.unconfigured],
  )
  const openWizard = () => wizard.open()

  return (
    <div>
      <PageHeader
        title={t('fallback.title')}
        description={t('fallback.description')}
        actions={
          <>
            <Button size="sm" onClick={openWizard}>
              {t('fallback.configureProviders')}
            </Button>
            <Button variant="outline" size="sm" onClick={chain.sync} disabled={chain.syncing}>
              {chain.syncing ? t('fallback.syncing') : t('fallback.sync')}
            </Button>
          </>
        }
      />

      <div className="space-y-6">
        <ChainStats entries={chain.configured} />

        {flash && <FlashMessage flash={flash} />}

        <ChainToolbar
          entries={chain.configured}
          filters={chain.filters}
          onFiltersChange={chain.setFilters}
          busy={chain.sorting || chain.bulkPending}
          bulkPending={chain.bulkPending}
          onSort={chain.sort}
          onRunBulk={chain.runBulk}
        />

        {chain.unconfigured.length > 0 && (
          <HiddenNotice
            count={chain.unconfigured.length}
            providers={hiddenProviders}
            showing={chain.showUnconfigured}
            onToggle={chain.toggleUnconfigured}
          />
        )}

        {chain.tokenUsage && chain.tokenUsage.totalBudget > 0 && <TokenBudget data={chain.tokenUsage} />}

        <ChainSection chain={chain} onConfigure={openWizard} />
      </div>
    </div>
  )
}
