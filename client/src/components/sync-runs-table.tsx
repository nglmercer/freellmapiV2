import { useTranslations } from '@/hooks/useTranslations'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { StatusBadge, type Tone } from '@/components/status-badge'
import { formatDate } from '@/components/fallback-format'
import { sourceStatusView } from '@/lib/source-status'

export interface SyncRun {
  id: number
  source: string
  status: string
  completedAt?: string
  summary: {
    fetched: number
    valid: number
    matched: number
    unmatched: number
    changed: number
    durationMs?: number
  }
}

/** Recent metadata syncs, newest first, with one row per run. */
export function SyncRunsTable({ runs }: { runs: SyncRun[] }) {
  const { t } = useTranslations()
  if (runs.length === 0) {
    return <p className="text-sm text-muted-foreground">{t('sources.noRuns')}</p>
  }
  return (
    <div className="overflow-hidden rounded-md border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t('sources.runs.when')}</TableHead>
            <TableHead>{t('sources.runs.source')}</TableHead>
            <TableHead>{t('sources.runs.status')}</TableHead>
            <TableHead className="text-right">{t('sources.runs.fetched')}</TableHead>
            <TableHead className="text-right">{t('sources.runs.matched')}</TableHead>
            <TableHead className="text-right">{t('sources.runs.changed')}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {runs.slice(0, 12).map((run) => {
            const view = sourceStatusView(run.status)
            const tone: Tone = run.status === 'fetched' && run.summary.changed === 0 ? 'muted' : view.tone
            return (
              <TableRow key={run.id}>
                <TableCell className="whitespace-nowrap text-xs text-muted-foreground tabular-nums">
                  {formatDate(run.completedAt) ?? '—'}
                </TableCell>
                <TableCell className="font-mono text-xs">{run.source}</TableCell>
                <TableCell>
                  <StatusBadge tone={tone}>{t(view.labelKey)}</StatusBadge>
                </TableCell>
                <TableCell className="text-right font-mono text-xs tabular-nums">{run.summary.fetched}</TableCell>
                <TableCell className="text-right font-mono text-xs tabular-nums">{run.summary.matched}</TableCell>
                <TableCell className="text-right font-mono text-xs tabular-nums">{run.summary.changed}</TableCell>
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    </div>
  )
}
