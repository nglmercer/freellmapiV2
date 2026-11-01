import { useState } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { cn } from '@/lib/utils'
import { PageHeader } from '@/components/page-header'
import { SourceControls } from '@/components/model-explorer/SourceControls'
import { MatchReview } from '@/components/model-explorer/MatchReview'
import { Simulator } from '@/components/model-explorer/Simulator'
import { ExplorerTab } from '@/components/model-explorer/explorer/ExplorerTab'
import { useExplorerState } from '@/components/model-explorer/explorer/use-explorer-state'

const TABS = ['explorer', 'sources', 'matches', 'simulator'] as const
type Tab = (typeof TABS)[number]

export default function ModelExplorerPage() {
  const { t } = useTranslations()
  const [tab, setTab] = useState<Tab>('explorer')
  // Held here, not in the tab, so filters and the comparison survive tab switches.
  const explorer = useExplorerState()

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('metadata.title')}
        description={t('metadata.description')}
        actions={
          <nav aria-label={t('metadata.title')} className="inline-flex flex-wrap gap-1 rounded-lg border bg-muted/40 p-1">
            {TABS.map((name) => (
              <button
                key={name}
                type="button"
                aria-pressed={tab === name}
                onClick={() => setTab(name)}
                className={cn(
                  'rounded-md px-3 py-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                  tab === name
                    ? 'bg-background font-medium shadow-sm'
                    : 'text-muted-foreground hover:text-foreground',
                )}
              >
                {t(`metadata.${name}`)}
              </button>
            ))}
          </nav>
        }
      />

      {tab === 'explorer' && <ExplorerTab state={explorer} />}
      {tab === 'sources' && <SourceControls />}
      {tab === 'matches' && <MatchReview />}
      {tab === 'simulator' && <Simulator />}
    </div>
  )
}
