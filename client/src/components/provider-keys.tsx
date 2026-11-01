import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { PLATFORMS } from '@/lib/platforms'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { StatusBadge } from '@/components/status-badge'
import { keyStatusTone } from '@/lib/source-status'
import { formatDate } from '@/components/fallback-format'
import type { ApiKey } from '../../../shared/types'

export interface HealthKey {
  id: number
  status: string
  lastCheckedAt: string | null
}

const DECRYPT_FAILED = '[decrypt failed]'

/** Replace a stored key that can no longer be decrypted. */
function ReplaceKeyDialog({ keyId, onClose }: { keyId: number | null; onClose: () => void }) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [value, setValue] = useState('')
  const replace = useMutation({
    mutationFn: (key: string) =>
      apiFetch(`/api/keys/${keyId}`, { method: 'PATCH', body: JSON.stringify({ key }) }),
    onSuccess: () => {
      setValue('')
      queryClient.invalidateQueries({ queryKey: ['keys'] })
      queryClient.invalidateQueries({ queryKey: ['health'] })
      onClose()
    },
  })

  return (
    <Dialog open={keyId !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogPopup>
        <DialogHeader>
          <DialogTitle>{t('keys.replaceTitle')}</DialogTitle>
          <DialogDescription>{t('keys.replaceDescription')}</DialogDescription>
        </DialogHeader>
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            if (value.trim()) replace.mutate(value.trim())
          }}
        >
          <Input
            type="password"
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t('keys.reEnterPlaceholder')}
            className="font-mono text-xs"
            autoFocus
            autoComplete="off"
          />
          {replace.isError && (
            <p role="alert" className="text-sm text-destructive">{(replace.error as Error).message}</p>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>{t('keys.cancel')}</Button>
            <Button type="submit" disabled={!value.trim() || replace.isPending}>
              {replace.isPending ? t('keys.saving') : t('keys.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogPopup>
    </Dialog>
  )
}

function KeyRow({
  row,
  health,
  onCheck,
  onRemove,
  onReplace,
  busy,
}: {
  row: ApiKey
  health: HealthKey | undefined
  onCheck: () => void
  onRemove: () => void
  onReplace: () => void
  busy: boolean
}) {
  const { t } = useTranslations()
  const status = health?.status ?? row.status
  const undecryptable = row.maskedKey === DECRYPT_FAILED
  const lastChecked = health?.lastCheckedAt ?? row.lastCheckedAt

  return (
    <div className="flex flex-wrap items-center gap-3 px-4 py-3">
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <code className="font-mono text-xs">{undecryptable ? t('keys.undecryptable') : row.maskedKey}</code>
          {row.label && <span className="text-xs text-muted-foreground">{row.label}</span>}
        </div>
        <div className="text-[11px] tabular-nums text-muted-foreground">
          {lastChecked ? `${t('keys.lastChecked')} ${formatDate(lastChecked) ?? ''}` : t('keys.neverChecked')}
        </div>
      </div>
      <StatusBadge tone={undecryptable ? 'danger' : keyStatusTone(status)}>
        {undecryptable ? t('keys.status.undecryptable') : t(`keys.status.${status}`, { defaultValue: status })}
      </StatusBadge>
      <div className="flex shrink-0 items-center gap-1">
        {undecryptable && (
          <Button variant="secondary" size="sm" onClick={onReplace}>
            {t('keys.replace')}
          </Button>
        )}
        <Button variant="ghost" size="sm" onClick={onCheck} disabled={busy}>
          {t('keys.check')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-muted-foreground hover:text-destructive"
          onClick={onRemove}
          disabled={busy}
        >
          {t('keys.remove')}
        </Button>
      </div>
    </div>
  )
}

/** Provider keys grouped by provider, with each key's health and actions. */
export function ProviderKeyGroups({
  keys,
  health,
  onCheck,
  onRemove,
  busy,
}: {
  keys: ApiKey[]
  health: Map<number, HealthKey>
  onCheck: (id: number) => void
  onRemove: (id: number) => void
  busy: boolean
}) {
  const { t } = useTranslations()
  const [replaceId, setReplaceId] = useState<number | null>(null)
  const groups = PLATFORMS.map((p) => ({ ...p, keys: keys.filter((k) => k.platform === p.value) })).filter(
    (g) => g.keys.length > 0,
  )

  return (
    <div className="space-y-4">
      {groups.map((group) => (
        <div key={group.value} className="overflow-hidden rounded-lg border bg-card">
          <div className="flex items-center justify-between gap-3 border-b bg-muted/30 px-4 py-2.5">
            <h3 className="text-sm font-medium">{group.label}</h3>
            <span className="text-xs tabular-nums text-muted-foreground">
              {t('keys.keyCount', { count: group.keys.length })}
            </span>
          </div>
          <div className="divide-y">
            {group.keys.map((row) => (
              <KeyRow
                key={row.id}
                row={row}
                health={health.get(row.id)}
                busy={busy}
                onCheck={() => onCheck(row.id)}
                onRemove={() => onRemove(row.id)}
                onReplace={() => setReplaceId(row.id)}
              />
            ))}
          </div>
        </div>
      ))}
      <ReplaceKeyDialog keyId={replaceId} onClose={() => setReplaceId(null)} />
    </div>
  )
}
