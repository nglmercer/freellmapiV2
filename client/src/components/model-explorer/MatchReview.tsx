import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Disclosure } from '@/components/disclosure'
import { selectClass } from './styles'
import type { Match } from './types'

export function MatchReview() {
  const { t } = useTranslations()
  const cache = useQueryClient()
  const [status, setStatus] = useState('unmatched')
  const [canonical, setCanonical] = useState('')
  const [reason, setReason] = useState('')
  const [offset, setOffset] = useState(0)
  const query = useQuery<{ data: Match[] }>({
    queryKey: ['metadata-matches', status, offset],
    queryFn: () =>
      apiFetch(
        `/api/metadata/matches?status=${status}&limit=25&offset=${offset}`,
      ),
  })
  const mutation = useMutation({
    mutationFn: ({ id, reject }: { id: number; reject: boolean }) =>
      apiFetch(`/api/metadata/matches/${id}/resolve`, {
        method: 'POST',
        body: JSON.stringify({
          ...(reject
            ? { reject: true }
            : { canonicalModelId: Number(canonical) }),
          reason,
        }),
      }),
    onSuccess: () =>
      cache.invalidateQueries({
        predicate: (q) => String(q.queryKey[0]).startsWith('metadata'),
      }),
  })
  return (
    <section className="space-y-4">
      <h2 className="font-medium">{t('metadata.matches')}</h2>
      <p className="text-sm text-muted-foreground">{t('metadata.matchNote')}</p>
      <div className="flex flex-wrap gap-2">
        <select
          aria-label={t('metadata.status')}
          className={selectClass}
          value={status}
          onChange={(e) => {
            setStatus(e.target.value)
            setOffset(0)
          }}
        >
          {[
            'unmatched',
            'ambiguous',
            'matched',
            'rejected',
            'canonicalOnly',
          ].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <Input
          className="w-48"
          placeholder={t('metadata.canonicalId')}
          aria-label={t('metadata.canonicalId')}
          type="number"
          min={1}
          value={canonical}
          onChange={(e) => setCanonical(e.target.value)}
        />
        <Input
          className="sm:max-w-sm"
          placeholder={t('metadata.reason')}
          aria-label={t('metadata.reason')}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </div>
      {(query.error || mutation.error) && (
        <p role="alert">{(query.error || mutation.error)?.message}</p>
      )}
      {query.data?.data.map((m) => (
        <div
          className="flex flex-wrap items-center gap-2 border-b py-2 text-xs"
          key={m.id}
        >
          <span className="flex-1 min-w-48">
            {m.source} · {m.sourceModelId}
            <span className="block text-muted-foreground">
              {m.canonicalHint && `${t('metadata.hint')}: ${m.canonicalHint}`}
            </span>
          </span>
          <Button
            size="sm"
            disabled={
              mutation.isPending ||
              !reason.trim() ||
              !Number.isInteger(Number(canonical)) ||
              Number(canonical) < 1
            }
            onClick={() => mutation.mutate({ id: m.id, reject: false })}
          >
            {t('metadata.link')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={mutation.isPending || !reason.trim()}
            onClick={() => mutation.mutate({ id: m.id, reject: true })}
          >
            {t('metadata.reject')}
          </Button>
          <MatchAudit id={m.id} />
        </div>
      ))}
      <div className="flex gap-2">
        <Button
          variant="outline"
          disabled={!offset}
          onClick={() => setOffset(Math.max(0, offset - 25))}
        >
          {t('metadata.previous')}
        </Button>
        <Button
          variant="outline"
          disabled={(query.data?.data.length ?? 0) < 25}
          onClick={() => setOffset(offset + 25)}
        >
          {t('metadata.next')}
        </Button>
      </div>
    </section>
  )
}

export function MatchAudit({ id }: { id: number }) {
  const { t } = useTranslations()
  return (
    <Disclosure
      summary={<span className="text-xs text-muted-foreground">{t('metadata.audit')}</span>}
      summaryClassName="py-2"
    >
      <MatchAuditList id={id} />
    </Disclosure>
  )
}

/** Fetched only while the audit disclosure is open. */
export function MatchAuditList({ id }: { id: number }) {
  const query = useQuery<{
    data: {
      id: number
      decision: string
      canonicalModelId: number | null
      reason: string
      createdAt: string
    }[]
  }>({
    queryKey: ['metadata-audit', id],
    queryFn: () => apiFetch(`/api/metadata/matches/${id}/audit`),
  })
  return (
    <div className="text-xs">
      {query.error && <p role="alert">{query.error.message}</p>}
      <ul>
        {query.data?.data.map((a) => (
          <li key={a.id}>
            {new Date(a.createdAt).toLocaleString()} · {a.decision} ·{' '}
            {a.canonicalModelId ?? 'N/A'} · {a.reason}
          </li>
        ))}
      </ul>
    </div>
  )
}
