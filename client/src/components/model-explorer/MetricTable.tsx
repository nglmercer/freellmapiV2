import type { MetadataMetric } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'

export function MetricTable({ metrics }: { metrics: MetadataMetric[] }) {
  const { t } = useTranslations()
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-xs text-left">
        <thead>
          <tr className="border-b">
            <th className="p-2">{t('metadata.metric')}</th>
            <th>{t('metadata.value')}</th>
            <th>{t('metadata.source')}</th>
            <th>{t('metadata.scope')}</th>
            <th>{t('metadata.updated')}</th>
          </tr>
        </thead>
        <tbody>
          {metrics.map((m) => (
            <tr key={`${m.id}-${m.metric}-${m.scope}`} className="border-b">
              <td className="p-2 font-mono">{m.metric}</td>
              <td className="pr-3">
                {typeof m.value.value === 'boolean'
                  ? t(
                      m.value.value
                        ? 'metadata.supported'
                        : 'metadata.unsupported',
                    )
                  : Array.isArray(m.value.value)
                    ? m.value.value.join(', ')
                    : String(m.value.value)}{' '}
                <span className="text-muted-foreground">
                  {!['text', 'boolean', 'json'].includes(m.unit) && m.unit}
                </span>
              </td>
              <td className="pr-3">
                {m.source}
                <div className="text-muted-foreground">
                  {m.benchmarkVersion && `v${m.benchmarkVersion}`}{' '}
                  {m.conflict && t('metadata.conflict')}
                </div>
              </td>
              <td className="pr-3">
                {t(`metadata.${m.scope}`)}
                <div>
                  {m.verified
                    ? t('metadata.verified')
                    : `${Math.round(m.confidence * 100)}%`}
                </div>
              </td>
              <td className="whitespace-nowrap" title={m.selectionReason}>
                {new Date(m.fetchedAt).toLocaleString()}{' '}
                {m.freshness === 'stale' && (
                  <span className="text-amber-600">{t('metadata.stale')}</span>
                )}
              </td>
            </tr>
          ))}
          {!metrics.length && (
            <tr>
              <td className="p-3" colSpan={5}>
                {t('metadata.unknown')}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
