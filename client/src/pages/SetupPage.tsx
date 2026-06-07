import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useTranslations } from '@/hooks/useTranslations'
import { Trans } from 'react-i18next'
import { switchLanguage, getCurrentLanguage } from '@/i18n'
import type { Platform } from '../../../shared/types'

const PLATFORMS: { value: Platform; label: string; description: string }[] = [
  { value: 'google', label: 'Google AI Studio', description: 'Gemini models (recommended)' },
  { value: 'groq', label: 'Groq', description: 'Fast inference, Llama/Mixtral' },
  { value: 'cerebras', label: 'Cerebras', description: 'Ultra-fast inference' },
  { value: 'sambanova', label: 'SambaNova', description: 'Fast inference' },
  { value: 'nvidia', label: 'NVIDIA NIM', description: 'NVIDIA hosted models' },
  { value: 'mistral', label: 'Mistral', description: 'Mistral/Pixtral models' },
  { value: 'openrouter', label: 'OpenRouter', description: 'Aggregator, many free models' },
  { value: 'github', label: 'GitHub Models', description: 'GitHub hosted models' },
  { value: 'cohere', label: 'Cohere', description: 'Command/R+ models' },
  { value: 'cloudflare', label: 'Cloudflare Workers AI', description: 'Edge AI models' },
  { value: 'zhipu', label: 'Zhipu AI', description: 'GLM models' },
  { value: 'ollama', label: 'Ollama Cloud', description: 'Cloud hosted Ollama' },
  { value: 'kilo', label: 'Kilo Gateway', description: 'Anonymous access OK' },
  { value: 'pollinations', label: 'Pollinations', description: 'Anonymous access OK' },
  { value: 'llm7', label: 'LLM7', description: 'Anonymous access OK' },
]

const ANONYMOUS_PLATFORMS = new Set(['kilo', 'pollinations', 'llm7'])

interface AddedKey {
  platform: string
  label: string
  maskedKey: string
}

interface CreateKeyResponse {
  id: number
  platform: string
  label: string
  maskedKey: string
  status: string
  enabled: boolean
}

function StepIndicator({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex items-center gap-2 mb-8">
      {Array.from({ length: total }, (_, i) => (
        <div key={i} className="flex items-center gap-2">
          <div
            className={`size-8 rounded-full flex items-center justify-center text-xs font-medium transition-colors ${
              i < current
                ? 'bg-primary text-primary-foreground'
                : i === current
                ? 'bg-primary text-primary-foreground ring-2 ring-primary/30'
                : 'bg-muted text-muted-foreground'
            }`}
          >
            {i < current ? (
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
            ) : (
              i + 1
            )}
          </div>
          {i < total - 1 && (
            <div className={`w-12 h-0.5 ${i < current ? 'bg-primary' : 'bg-muted'}`} />
          )}
        </div>
      ))}
    </div>
  )
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  const { t } = useTranslations()
  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.welcome.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.welcome.description')}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 space-y-4">
        <h3 className="text-sm font-medium">{t('setup.welcome.whatYoullConfigure')}</h3>
        <div className="space-y-3">
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">1</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.providerKeys')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.providerKeysDescription')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">2</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.unifiedApiKey')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.unifiedApiKeyDescription')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">3</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.startUsing')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.startUsingDescription')}</p>
            </div>
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-dashed p-4 mb-6">
        <p className="text-xs text-muted-foreground">
          <strong>{t('setup.welcome.tipPrefix')}</strong> {t('setup.welcome.tipSuffix')}
        </p>
      </div>

      <Button onClick={onNext} className="w-full">
        {t('setup.welcome.button')}
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
      </Button>
    </div>
  )
}

function AddKeysStep({
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

function UnifiedKeyStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { t } = useTranslations()
  const [showKey, setShowKey] = useState(false)
  const [copied, setCopied] = useState(false)

  const { data } = useQuery<{ apiKey: string }>({
    queryKey: ['unified-key'],
    queryFn: () => apiFetch('/api/settings/api-key'),
  })

  const apiKey = data?.apiKey ?? ''
  const masked = apiKey ? apiKey.slice(0, 13) + '•'.repeat(32) : '…'
  const baseUrl = import.meta.env.DEV
    ? `http://${window.location.hostname}:${__SERVER_PORT__}/v1`
    : `${window.location.origin}/v1`

  function copy() {
    navigator.clipboard.writeText(apiKey)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.unifiedKey.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.unifiedKey.description')}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 space-y-4">
        <div>
          <Label className="text-xs text-muted-foreground">{t('setup.unifiedKey.label')}</Label>
          <div className="flex items-center gap-2 mt-1.5">
            <code className="flex-1 font-mono text-xs bg-muted px-3 py-2 rounded-md select-all truncate tabular-nums">
              {showKey ? apiKey : masked}
            </code>
            <Button variant="outline" size="sm" onClick={() => setShowKey(!showKey)}>
              {showKey ? t('setup.unifiedKey.hide') : t('setup.unifiedKey.show')}
            </Button>
            <Button variant="outline" size="sm" onClick={copy}>
              {copied ? t('setup.unifiedKey.copied') : t('setup.unifiedKey.copy')}
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
          <span className="text-muted-foreground">{t('setup.unifiedKey.baseUrl')}</span>
          <code className="font-mono text-xs">{baseUrl}</code>
          <span className="text-muted-foreground">{t('setup.unifiedKey.endpoint')}</span>
          <code className="font-mono text-xs">/v1/chat/completions</code>
        </div>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6">
        <h3 className="text-sm font-medium mb-3">{t('setup.unifiedKey.quickStartTitle')}</h3>
        <pre className="text-xs font-mono bg-muted p-3 rounded-md overflow-x-auto whitespace-pre-wrap">
{`curl ${baseUrl}/chat/completions \\
  -H "Authorization: Bearer ${apiKey.slice(0, 13)}..." \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "gemini-2.0-flash",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'`}
        </pre>
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} className="flex-1">
          {t('setup.unifiedKey.back')}
        </Button>
        <Button onClick={onNext} className="flex-1">
          {t('setup.unifiedKey.continue')}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
        </Button>
      </div>
    </div>
  )
}

function CompleteStep({ addedKeys }: { addedKeys: AddedKey[] }) {
  const { t } = useTranslations()
  const navigate = useNavigate()
  const count = addedKeys.length
  const providerPlural = count === 1 ? '' : 's'

  return (
    <div className="max-w-lg text-center">
      <div className="mb-6">
        <div className="size-16 rounded-full bg-emerald-500/10 flex items-center justify-center mx-auto mb-4">
          <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-emerald-500"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
        </div>
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.complete.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.complete.description', { count, plural: providerPlural })}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 text-left space-y-3">
        <h3 className="text-sm font-medium">{t('setup.complete.whatsNext')}</h3>
        <div className="space-y-2">
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.tryPlayground')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.tryPlaygroundDesc')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.addMoreProviders')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.addMoreProvidersDesc')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2"/><polyline points="22,6 12,13 2,6"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.configureFallback')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.configureFallbackDesc')}</p>
            </div>
          </div>
        </div>
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={() => navigate('/keys')} className="flex-1">
          {t('setup.complete.goToKeys')}
        </Button>
        <Button onClick={() => navigate('/playground')} className="flex-1">
          {t('setup.complete.openPlayground')}
        </Button>
      </div>
    </div>
  )
}

function LanguageToggle() {
  const current = getCurrentLanguage()

  function toggle() {
    switchLanguage(current === 'en' ? 'es' : 'en')
  }

  return (
    <Button variant="ghost" size="sm" onClick={toggle}>
      {current === 'en' ? 'ES' : 'EN'}
    </Button>
  )
}

export default function SetupPage() {
  const { t } = useTranslations()
  const [step, setStep] = useState(0)
  const [addedKeys, setAddedKeys] = useState<AddedKey[]>([])

  const steps = [
    <WelcomeStep onNext={() => setStep(1)} />,
    <AddKeysStep
      addedKeys={addedKeys}
      onAdd={(key) => setAddedKeys(prev => [...prev, key])}
      onRemove={(index) => setAddedKeys(prev => prev.filter((_, i) => i !== index))}
      onNext={() => setStep(2)}
      onBack={() => setStep(0)}
    />,
    <UnifiedKeyStep onNext={() => setStep(3)} onBack={() => setStep(1)} />,
    <CompleteStep addedKeys={addedKeys} />,
  ]

  return (
    <div className="min-h-[calc(100vh-4rem)] flex flex-col items-center justify-center -mt-8">
      <div className="w-full max-w-2xl px-6">
        <div className="flex items-center justify-between mb-8">
          <div className="flex items-center gap-2">
            <span className="inline-block size-2 rounded-full bg-foreground" />
            <span className="font-semibold tracking-tight">{t('app.brand')}</span>
          </div>
          <LanguageToggle />
        </div>
        <p className="text-xs text-muted-foreground text-center mb-8">{t('app.initialSetup')}</p>

        <StepIndicator current={step} total={steps.length} />

        <div className="flex justify-center">
          {steps[step]}
        </div>
      </div>
    </div>
  )
}
