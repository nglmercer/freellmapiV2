import { useQuery } from '@tanstack/react-query'
import type { MetadataMetric } from '@freellmapi/shared'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'

export function Comparison({ ids }: { ids: number[] }) {
  const { t } = useTranslations()
  const query = useQuery<{ data: { id: number; metrics: MetadataMetric[] }[] }>(
    {
      queryKey: ['metadata-compare', ids],
      queryFn: () => apiFetch(`/api/models/compare?ids=${ids.join(',')}`),
      enabled: ids.length >= 2,
    },
  )
  if (ids.length < 2) return null
  const keys = Array.from(
    new Set(
      query.data?.data.flatMap((d) =>
        d.metrics.map(
          (m) => `${m.metric}|${m.scope}|${m.source}|${m.benchmarkVersion}`,
        ),
      ) ?? [],
    ),
  )
  return (
    <section className="border rounded-md p-4 space-y-3">
      <h2 className="font-medium">{t('metadata.compare')}</h2>
      <p className="text-xs text-muted-foreground">
        {t('metadata.compareNote')}
      </p>
      {query.isError && <p role="alert">{query.error.message}</p>}
      <div className="overflow-x-auto">
        <table className="text-xs w-full text-left">
          <thead>
            <tr>
              <th>{t('metadata.metric')}</th>
              {ids.map((id) => (
                <th key={id}>#{id}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {keys.map((key) => (
              <tr key={key} className="border-t">
                <td className="py-2">{key.split('|').join(' · ')}</td>
                {ids.map((id) => {
                  const m = query.data?.data
                    .find((d) => d.id === id)
                    ?.metrics.find(
                      (m) =>
                        `${m.metric}|${m.scope}|${m.source}|${m.benchmarkVersion}` ===
                        key,
                    )
                  return (
                    <td key={id} className="px-2">
                      {m ? `${String(m.value.value)} ${m.unit}` : 'N/A'}
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}
