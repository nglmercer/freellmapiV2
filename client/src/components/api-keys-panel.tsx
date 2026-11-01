import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { SecretDialog } from '@/components/secret-view'
import { formatDate } from '@/components/fallback-format'

interface ApiKeyRecord {
  id: string
  name: string
  prefix: string
  revoked: boolean
  created_at: number
}

/** Dashboard-issued keys for the OpenAI-compatible API. */
export function ApiKeysPanel() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [issued, setIssued] = useState<string | null>(null)
  const [revokeId, setRevokeId] = useState<string | null>(null)

  const keys = useQuery<{ api_keys: ApiKeyRecord[] }>({
    queryKey: ['api-keys'],
    queryFn: () => apiFetch('/api/auth/api-key/list'),
  })

  const create = useMutation({
    mutationFn: (keyName: string) =>
      apiFetch<{ key: string }>('/api/auth/api-key/create', {
        method: 'POST',
        body: JSON.stringify({ name: keyName }),
      }),
    onSuccess: (data) => {
      setCreateOpen(false)
      setName('')
      setIssued(data.key)
      queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    },
  })

  const revoke = useMutation({
    mutationFn: (id: string) =>
      apiFetch('/api/auth/api-key/revoke', { method: 'POST', body: JSON.stringify({ id }) }),
    onSuccess: () => {
      setRevokeId(null)
      queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    },
  })

  const active = (keys.data?.api_keys ?? []).filter((key) => !key.revoked)

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          {t('keys.apiKeys.count', { count: active.length })}
        </p>
        <Button size="sm" onClick={() => setCreateOpen(true)}>
          {t('keys.apiKeys.create')}
        </Button>
      </div>

      {active.length === 0 ? (
        <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
          {t('keys.apiKeys.empty')}
        </div>
      ) : (
        <div className="overflow-hidden rounded-md border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('keys.apiKeys.name')}</TableHead>
                <TableHead>{t('keys.apiKeys.prefix')}</TableHead>
                <TableHead>{t('keys.apiKeys.created')}</TableHead>
                <TableHead className="text-right">
                  <span className="sr-only">{t('keys.apiKeys.actions')}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {active.map((key) => (
                <TableRow key={key.id}>
                  <TableCell className="font-medium">{key.name}</TableCell>
                  <TableCell>
                    <code className="font-mono text-xs text-muted-foreground">{key.prefix}…</code>
                  </TableCell>
                  <TableCell className="tabular-nums text-muted-foreground">
                    {formatDate(new Date(key.created_at * 1000).toISOString())}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button variant="ghost" size="sm" onClick={() => setRevokeId(key.id)}>
                      {t('keys.apiKeys.revoke')}
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogPopup>
          <DialogHeader>
            <DialogTitle>{t('keys.apiKeys.createTitle')}</DialogTitle>
            <DialogDescription>{t('keys.apiKeys.createDescription')}</DialogDescription>
          </DialogHeader>
          <form
            className="space-y-4"
            onSubmit={(event) => {
              event.preventDefault()
              const trimmed = name.trim()
              if (trimmed) create.mutate(trimmed)
            }}
          >
            <div className="space-y-1.5">
              <Label htmlFor="api-key-name">{t('keys.apiKeys.name')}</Label>
              <Input
                id="api-key-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t('keys.apiKeys.namePlaceholder')}
                maxLength={100}
                autoFocus
              />
            </div>
            {create.isError && (
              <p role="alert" className="text-sm text-destructive">{t('keys.apiKeys.error')}</p>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setCreateOpen(false)}>
                {t('keys.cancel')}
              </Button>
              <Button type="submit" disabled={!name.trim() || create.isPending}>
                {create.isPending ? t('keys.apiKeys.creating') : t('keys.apiKeys.create')}
              </Button>
            </DialogFooter>
          </form>
        </DialogPopup>
      </Dialog>

      {issued && (
        <SecretDialog
          open
          onOpenChange={(open) => !open && setIssued(null)}
          title={t('keys.apiKeys.newKeyTitle')}
          description={t('keys.apiKeys.newKeyDescription')}
          secret={issued}
          note={t('keys.apiKeys.newKeyDescription')}
        />
      )}

      <ConfirmDialog
        open={revokeId !== null}
        onOpenChange={(open) => !open && setRevokeId(null)}
        title={t('keys.apiKeys.revokeTitle')}
        description={t('keys.apiKeys.revokeDescription')}
        confirmLabel={t('keys.apiKeys.revoke')}
        variant="danger"
        busy={revoke.isPending}
        onConfirm={() => revokeId && revoke.mutate(revokeId)}
      />
    </div>
  )
}
