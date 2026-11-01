import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { generateJoinCode, getJoinCodeStatus, revokeJoinCode } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { useTranslations } from '@/hooks/useTranslations'

/**
 * Creates, shows once, and revokes the owner's join code. The code signs a
 * browser in without the password, so it is shown as a link as well.
 */
export function JoinCodePanel() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [code, setCode] = useState<string | null>(null)
  const [copied, setCopied] = useState<'code' | 'link' | null>(null)

  const status = useQuery({
    queryKey: ['join-code'],
    queryFn: getJoinCodeStatus,
  })

  const generate = useMutation({
    mutationFn: generateJoinCode,
    onSuccess: data => {
      setCode(data.code)
      queryClient.invalidateQueries({ queryKey: ['join-code'] })
    },
  })

  const revoke = useMutation({
    mutationFn: revokeJoinCode,
    onSuccess: () => {
      setCode(null)
      queryClient.invalidateQueries({ queryKey: ['join-code'] })
    },
  })

  const base = import.meta.env.BASE_URL.replace(/\/$/, '')
  const link = code ? `${window.location.origin}${base}/?join=${encodeURIComponent(code)}` : ''

  function copy(which: 'code' | 'link', value: string) {
    navigator.clipboard.writeText(value)
    setCopied(which)
    setTimeout(() => setCopied(null), 1500)
  }

  const enabled = status.data?.enabled ?? false

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Button onClick={() => generate.mutate()} disabled={generate.isPending}>
          {enabled ? t('keys.joinCode.regenerate') : t('keys.joinCode.generate')}
        </Button>
        {enabled && (
          <Button variant="ghost" onClick={() => revoke.mutate()} disabled={revoke.isPending}>
            {t('keys.joinCode.revoke')}
          </Button>
        )}
        {!enabled && !code && (
          <span className="text-sm text-muted-foreground">{t('keys.joinCode.disabled')}</span>
        )}
      </div>

      {code && (
        <div className="rounded-md border border-amber-500/50 bg-amber-500/5 p-4 space-y-3">
          <div>
            <p className="text-sm font-medium">{t('keys.joinCode.newCodeTitle')}</p>
            <p className="text-xs text-muted-foreground mt-0.5">{t('keys.joinCode.newCodeDescription')}</p>
          </div>
          <div className="flex items-center gap-2">
            <code className="flex-1 font-mono text-sm bg-muted px-3 py-2 rounded-md select-all tracking-wider">
              {code}
            </code>
            <Button variant="outline" size="sm" onClick={() => copy('code', code)}>
              {copied === 'code' ? t('keys.joinCode.copied') : t('keys.joinCode.copy')}
            </Button>
          </div>
          <div className="flex items-center gap-2">
            <code className="flex-1 font-mono text-xs bg-muted px-3 py-2 rounded-md select-all break-all">
              {link}
            </code>
            <Button variant="outline" size="sm" onClick={() => copy('link', link)}>
              {copied === 'link' ? t('keys.joinCode.copied') : t('keys.joinCode.copyLink')}
            </Button>
          </div>
          <Button variant="ghost" size="sm" onClick={() => setCode(null)}>
            {t('keys.joinCode.dismiss')}
          </Button>
        </div>
      )}
    </div>
  )
}
