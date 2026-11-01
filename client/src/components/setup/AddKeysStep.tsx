import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Trans } from 'react-i18next'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type { Platform } from '../../../../shared/types'
import { ANONYMOUS_PLATFORMS, PLATFORMS, type AddedKey, type CreateKeyResponse } from './setup-model'

export function AddKeysStep({
  addedKeys,
  onAdd,
  onRemove,
  onNext,
  onBack,
}: {
  addedKeys: AddedKey[]
  onAdd: (key: AddedKey) => void
  onRemove: (index: number) => void
  onNext: () => void
  onBack: () => void
}) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [platform, setPlatform] = useState<Platform | ''>('')
  const [apiKey, setApiKey] = useState('')
  const [accountId, setAccountId] = useState('')
  const [label, setLabel] = useState('')

  const needsAccountId = platform === 'cloudflare'
  const isAnonymous = platform && ANONYMOUS_PLATFORMS.has(platform)

  const addKey = useMutation<CreateKeyResponse, Error, { platform: string; key: string; label?: string }>({
    mutationFn: (body) =>
      apiFetch('/api/keys', { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['keys'] })
      queryClient.invalidateQueries({ queryKey: ['health'] })
      queryClient.invalidateQueries({ queryKey: ['setup-status'] })
      onAdd({
        platform: platform!,
        label: label || PLATFORMS.find(p => p.value === platform)?.label || '',
        maskedKey: data.maskedKey || '****',
      })
      setPlatform('')
      setApiKey('')
      setAccountId('')
      setLabel('')
    },
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!platform) return
    if (!isAnonymous && !apiKey) return
    if (needsAccountId && !accountId) return
    const key = isAnonymous ? 'anonymous' : needsAccountId ? `${accountId}:${apiKey}` : apiKey
    addKey.mutate({ platform, key, label: label || undefined })
  }

  const selectedPlatform = PLATFORMS.find(p => p.value === platform)

  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.addKeys.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.addKeys.description')}
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-4 mb-6">
        <div className="space-y-1.5">
          <Label className="text-sm">{t('setup.addKeys.providerLabel')}</Label>
          <Select value={platform} onValueChange={(v) => { setPlatform(v as Platform); setApiKey('') }}>
            <SelectTrigger>
              <SelectValue placeholder={t('setup.addKeys.providerPlaceholder')} />
            </SelectTrigger>
            <SelectContent>
              {PLATFORMS.map(p => (
                <SelectItem key={p.value} value={p.value}>
                  <div className="flex items-center gap-2">
                    <span>{p.label}</span>
                    <span className="text-xs text-muted-foreground">({p.description})</span>
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {platform && !isAnonymous && (
          <>
            {needsAccountId && (
              <div className="space-y-1.5">
                <Label className="text-sm">{t('setup.addKeys.accountIdLabel')}</Label>
                <Input
                  value={accountId}
                  onChange={e => setAccountId(e.target.value)}
                  placeholder={t('setup.addKeys.accountIdPlaceholder')}
                  className="font-mono text-xs"
                />
              </div>
            )}
            <div className="space-y-1.5">
              <Label className="text-sm">{needsAccountId ? t('setup.addKeys.tokenLabel') : t('setup.addKeys.keyLabel')}</Label>
              <Input
                type="password"
                value={apiKey}
                onChange={e => setApiKey(e.target.value)}
                placeholder={needsAccountId ? t('setup.addKeys.tokenPlaceholder') : t('setup.addKeys.apiKeyPlaceholder')}
                className="font-mono text-xs"
              />
              <p className="text-xs text-muted-foreground">
                {platform === 'google' && t('setup.addKeys.help.google')}
                {platform === 'groq' && t('setup.addKeys.help.groq')}
                {platform === 'cerebras' && t('setup.addKeys.help.cerebras')}
                {platform === 'sambanova' && t('setup.addKeys.help.sambanova')}
                {platform === 'nvidia' && t('setup.addKeys.help.nvidia')}
                {platform === 'mistral' && t('setup.addKeys.help.mistral')}
                {platform === 'openrouter' && t('setup.addKeys.help.openrouter')}
                {platform === 'github' && t('setup.addKeys.help.github')}
                {platform === 'cohere' && t('setup.addKeys.help.cohere')}
                {platform === 'cloudflare' && t('setup.addKeys.help.cloudflare')}
                {platform === 'zhipu' && t('setup.addKeys.help.zhipu')}
                {platform === 'ollama' && t('setup.addKeys.help.ollama')}
              </p>
            </div>
          </>
        )}

        {platform && isAnonymous && (
          <div className="rounded-lg border border-dashed p-4 bg-muted/30">
            <p className="text-xs text-muted-foreground">
              <strong>{selectedPlatform?.label}</strong> {t('setup.addKeys.anonymousWorks')}
            </p>
          </div>
        )}

        <div className="space-y-1.5">
          <Label className="text-sm">{t('setup.addKeys.labelLabel')}</Label>
          <Input
            value={label}
            onChange={e => setLabel(e.target.value)}
            placeholder={t('setup.addKeys.labelPlaceholder')}
          />
        </div>

        {addKey.isError && (
          <p className="text-sm text-destructive">{(addKey.error as Error).message}</p>
        )}

        <Button
          type="submit"
          className="w-full"
          disabled={!platform || (!isAnonymous && !apiKey) || (needsAccountId && !accountId) || addKey.isPending}
        >
          {addKey.isPending ? t('setup.addKeys.adding') : isAnonymous ? t('setup.addKeys.enableProvider') : t('setup.addKeys.keyLabel')}
        </Button>
      </form>

      {addedKeys.length > 0 && (
        <div className="space-y-3 mb-6">
          <h3 className="text-sm font-medium">
            {t('setup.addKeys.addedProviders_other', { count: addedKeys.length })}
          </h3>
          <div className="rounded-lg border divide-y bg-card overflow-hidden">
            {addedKeys.map((k, i) => (
              <div key={i} className="flex items-center gap-3 px-4 py-3">
                <span className="size-1.5 rounded-full bg-emerald-500 shrink-0" />
                <span className="text-sm font-medium">{k.label || k.platform}</span>
                <code className="text-xs font-mono text-muted-foreground">{k.maskedKey}</code>
                <div className="flex-1" />
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground hover:text-destructive"
                  onClick={() => onRemove(i)}
                >
                  {t('setup.addKeys.remove')}
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} className="flex-1">
          {t('setup.addKeys.back')}
        </Button>
        <Button onClick={onNext} className="flex-1" disabled={addedKeys.length === 0}>
          {t('setup.addKeys.continue')}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
        </Button>
      </div>

      {addedKeys.length === 0 && (
        <p className="text-xs text-muted-foreground text-center mt-3">
          <Trans i18nKey="setup.addKeys.helpContinue" action={<button type="button" className="text-primary underline underline-offset-2" onClick={onNext}>{t('setup.addKeys.skipForNow')}</button>} />
        </p>
      )}
    </div>
  )
}
