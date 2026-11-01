import { useTranslations } from '@/hooks/useTranslations'
import type { FallbackEntry } from '../../../shared/types'
import { KeyValueSummary } from './key-value-summary'
import { qualityOf, speedOf } from './fallback-format'

/** The next models a request would try, in priority order. */
export function ChainPreview({ entries }: { entries: FallbackEntry[] }) {
  const { t } = useTranslations()
  const next = entries.filter((e) => e.enabled).slice(0, 6)

  return (
    <section className="rounded-lg border bg-card p-4">
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h2 className="text-sm font-medium">{t('fallback.preview.title')}</h2>
        <span className="text-xs text-muted-foreground">{t('fallback.preview.hint')}</span>
      </div>
      {next.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t('fallback.preview.empty')}</p>
      ) : (
        <ol className="flex gap-2 overflow-x-auto pb-1">
          {next.map((entry, index) => {
            const quality = qualityOf(entry)
            const speed = speedOf(entry)
            return (
              <li
                key={entry.modelDbId}
                className="min-w-[11rem] flex-1 space-y-1.5 rounded-md border bg-background px-3 py-2"
              >
                <div className="flex items-center gap-2">
                  <span className="font-mono text-xs tabular-nums text-muted-foreground">{index + 1}</span>
                  <span className="truncate text-sm font-medium">{entry.displayName}</span>
                </div>
                <KeyValueSummary
                  emptyLabel={t('fallback.preview.noScores')}
                  items={[
                    { label: 'Q', value: quality != null ? quality.toFixed(1) : null },
                    { label: t('fallback.metrics.speed'), value: speed != null ? `${speed.toFixed(1)} tok/s` : null },
                  ]}
                />
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}
