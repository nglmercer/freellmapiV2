import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { generateJoinCode, getJoinCodeStatus, revokeJoinCode } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { SecretDialog } from '@/components/secret-view'
import { StatusBadge } from '@/components/status-badge'

/**
 * Fast sign-in for another device. Generating a code shows it once in a dialog;
 * the page itself never changes size when a code is issued or revoked.
 */
export function JoinCodePanel() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [confirmGenerate, setConfirmGenerate] = useState(false)
  const [confirmRevoke, setConfirmRevoke] = useState(false)
  const [issued, setIssued] = useState<string | null>(null)

  const status = useQuery({ queryKey: ['join-code'], queryFn: getJoinCodeStatus })
  const enabled = status.data?.enabled ?? false

  const generate = useMutation({
    mutationFn: generateJoinCode,
    onSuccess: (data) => {
      setConfirmGenerate(false)
      setIssued(data.code)
      queryClient.invalidateQueries({ queryKey: ['join-code'] })
    },
  })

  const revoke = useMutation({
    mutationFn: revokeJoinCode,
    onSuccess: () => {
      setConfirmRevoke(false)
      queryClient.invalidateQueries({ queryKey: ['join-code'] })
    },
  })

  const base = import.meta.env.BASE_URL.replace(/\/$/, '')

  function requestGenerate() {
    if (enabled) setConfirmGenerate(true)
    else generate.mutate()
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-4">
      <div className="min-w-0 space-y-1.5">
        <StatusBadge tone={enabled ? 'success' : 'muted'}>
          {enabled ? t('keys.joinCode.active') : t('keys.joinCode.inactive')}
        </StatusBadge>
        <p className="text-xs text-muted-foreground">{t('keys.joinCode.description')}</p>
      </div>
      <div className="flex shrink-0 gap-2">
        <Button size="sm" onClick={requestGenerate} disabled={generate.isPending || status.isLoading}>
          {enabled ? t('keys.joinCode.regenerate') : t('keys.joinCode.generate')}
        </Button>
        {enabled && (
          <Button variant="outline" size="sm" onClick={() => setConfirmRevoke(true)}>
            {t('keys.joinCode.revoke')}
          </Button>
        )}
      </div>

      <ConfirmDialog
        open={confirmGenerate}
        onOpenChange={setConfirmGenerate}
        title={t('keys.joinCode.regenerateTitle')}
        description={t('keys.joinCode.regenerateDescription')}
        confirmLabel={t('keys.joinCode.regenerate')}
        variant="danger"
        busy={generate.isPending}
        onConfirm={() => generate.mutate()}
      />
      <ConfirmDialog
        open={confirmRevoke}
        onOpenChange={setConfirmRevoke}
        title={t('keys.joinCode.revokeTitle')}
        description={t('keys.joinCode.revokeDescription')}
        confirmLabel={t('keys.joinCode.revoke')}
        variant="danger"
        busy={revoke.isPending}
        onConfirm={() => revoke.mutate()}
      />

      {issued && (
        <SecretDialog
          open
          onOpenChange={(open) => !open && setIssued(null)}
          title={t('keys.joinCode.newCodeTitle')}
          description={t('keys.joinCode.newCodeDescription')}
          secret={issued}
          link={{
            label: t('keys.joinCode.link'),
            value: `${window.location.origin}${base}/?join=${encodeURIComponent(issued)}`,
          }}
          note={t('keys.joinCode.newCodeDescription')}
        />
      )}
    </div>
  )
}
