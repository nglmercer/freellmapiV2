import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useTranslations } from '@/hooks/useTranslations'

interface ApiKeyRecord {
  id: string
  name: string
  prefix: string
  revoked: boolean
  created_at: number
}

function endpointBase(): string {
  return import.meta.env.DEV
    ? `http://${window.location.hostname}:${__SERVER_PORT__}/v1`
    : `${window.location.origin}/v1`
}

/**
 * Lists, creates and revokes the signed-in owner's API keys for the proxy.
 * A full key is returned only once, at creation, so it is shown until dismissed.
 */
export function ApiKeysPanel() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

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
    onSuccess: data => {
      setNewKey(data.key)
      setName('')
      queryClient.invalidateQueries({ queryKey: ['api-keys'] })
    },
  })

  const revoke = useMutation({
    mutationFn: (id: string) =>
      apiFetch('/api/auth/api-key/revoke', { method: 'POST', body: JSON.stringify({ id }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['api-keys'] }),
  })

  const baseUrl = endpointBase()
  const example = newKey ?? 'YOUR_API_KEY'
  const active = (keys.data?.api_keys ?? []).filter(key => !key.revoked)

  function submit(event: React.FormEvent) {
    event.preventDefault()
    const trimmed = name.trim()
    if (trimmed) create.mutate(trimmed)
  }

  function copy() {
    if (!newKey) return
    navigator.clipboard.writeText(newKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="space-y-5">
      <form onSubmit={submit} className="flex items-center gap-2">
        <Input
          value={name}
          onChange={event => setName(event.target.value)}
          placeholder={t('keys.apiKeys.namePlaceholder')}
          aria-label={t('keys.apiKeys.namePlaceholder')}
          maxLength={100}
        />
        <Button type="submit" disabled={!name.trim() || create.isPending}>
          {create.isPending ? t('keys.apiKeys.creating') : t('keys.apiKeys.create')}
        </Button>
      </form>
      {create.isError && (
        <p className="text-sm text-destructive">{t('keys.apiKeys.error')}</p>
      )}

      {newKey && (
        <div className="rounded-md border border-amber-500/50 bg-amber-500/5 p-4 space-y-3">
          <div>
            <p className="text-sm font-medium">{t('keys.apiKeys.newKeyTitle')}</p>
            <p className="text-xs text-muted-foreground mt-0.5">{t('keys.apiKeys.newKeyDescription')}</p>
          </div>
          <div className="flex items-center gap-2">
            <code className="flex-1 font-mono text-xs bg-muted px-3 py-2 rounded-md select-all break-all">
              {newKey}
            </code>
            <Button variant="outline" size="sm" onClick={copy}>
              {copied ? t('keys.apiKeys.copied') : t('keys.apiKeys.copy')}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setNewKey(null)}>
              {t('keys.apiKeys.dismiss')}
            </Button>
          </div>
        </div>
      )}

      {active.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t('keys.apiKeys.empty')}</p>
      ) : (
        <ul className="divide-y rounded-md border">
          {active.map(key => (
            <li key={key.id} className="flex items-center gap-3 px-4 py-2.5 text-sm">
              <span className="font-medium truncate">{key.name}</span>
              <code className="font-mono text-xs text-muted-foreground">{key.prefix}…</code>
              <span className="ml-auto text-xs text-muted-foreground tabular-nums">
                {new Date(key.created_at * 1000).toLocaleDateString()}
              </span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => revoke.mutate(key.id)}
                disabled={revoke.isPending}
              >
                {t('keys.apiKeys.revoke')}
              </Button>
            </li>
          ))}
        </ul>
      )}

      <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-xs">
        <span className="text-muted-foreground">{t('keys.apiKeys.baseUrl')}</span>
        <code className="font-mono">{baseUrl}</code>
        <span className="text-muted-foreground">{t('keys.apiKeys.endpoint')}</span>
        <code className="font-mono">/chat/completions</code>
      </div>
      <pre className="text-xs font-mono bg-muted p-3 rounded-md overflow-x-auto whitespace-pre-wrap">
{`curl ${baseUrl}/chat/completions \\
  -H "Authorization: Bearer ${example}" \\
  -H "Content-Type: application/json" \\
  -d '{"model": "gemini-2.0-flash", "messages": [{"role": "user", "content": "Hello!"}]}'`}
      </pre>
    </div>
  )
}
