import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
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
  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">Welcome to FreeLLMAPI</h2>
        <p className="text-sm text-muted-foreground mt-2">
          A unified proxy for free LLM API providers. Route requests through multiple providers with automatic fallback, rate limiting, and a single OpenAI-compatible endpoint.
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 space-y-4">
        <h3 className="text-sm font-medium">What you'll configure:</h3>
        <div className="space-y-3">
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">1</span>
            </div>
            <div>
              <p className="text-sm font-medium">Provider API Keys</p>
              <p className="text-xs text-muted-foreground">Add keys from free LLM providers (Google, Groq, Mistral, etc.)</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">2</span>
            </div>
            <div>
              <p className="text-sm font-medium">Unified API Key</p>
              <p className="text-xs text-muted-foreground">Get your personal key to authenticate requests to this proxy</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">3</span>
            </div>
            <div>
              <p className="text-sm font-medium">Start Using</p>
              <p className="text-xs text-muted-foreground">Use the OpenAI-compatible endpoint with any client</p>
            </div>
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-dashed p-4 mb-6">
        <p className="text-xs text-muted-foreground">
          <strong>Tip:</strong> Some providers like Kilo Gateway, Pollinations, and LLM7 work without API keys (anonymous access). You can add them directly or skip to add keys later.
        </p>
      </div>

      <Button onClick={onNext} className="w-full">
        Get Started
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
        <h2 className="text-xl font-semibold tracking-tight">Add Provider Keys</h2>
        <p className="text-sm text-muted-foreground mt-2">
          Add API keys from free LLM providers. You need at least one to start routing requests.
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-4 mb-6">
        <div className="space-y-1.5">
          <Label className="text-sm">Provider</Label>
          <Select value={platform} onValueChange={(v) => { setPlatform(v as Platform); setApiKey('') }}>
            <SelectTrigger>
              <SelectValue placeholder="Select a provider" />
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
                <Label className="text-sm">Account ID</Label>
                <Input
                  value={accountId}
                  onChange={e => setAccountId(e.target.value)}
                  placeholder="Your Cloudflare account ID"
                  className="font-mono text-xs"
                />
              </div>
            )}
            <div className="space-y-1.5">
              <Label className="text-sm">{needsAccountId ? 'API Token' : 'API Key'}</Label>
              <Input
                type="password"
                value={apiKey}
                onChange={e => setApiKey(e.target.value)}
                placeholder={needsAccountId ? 'Bearer token' : 'Paste your API key'}
                className="font-mono text-xs"
              />
              <p className="text-xs text-muted-foreground">
                {platform === 'google' && 'Get a free key at aistudio.google.com'}
                {platform === 'groq' && 'Get a free key at console.groq.com'}
                {platform === 'cerebras' && 'Get a free key at cloud.cerebras.ai'}
                {platform === 'sambanova' && 'Get a free key at cloud.sambanova.ai'}
                {platform === 'nvidia' && 'Get a free key at build.nvidia.com'}
                {platform === 'mistral' && 'Get a free key at console.mistral.ai'}
                {platform === 'openrouter' && 'Get a free key at openrouter.ai'}
                {platform === 'github' && 'Get a free key at github.com/settings/tokens'}
                {platform === 'cohere' && 'Get a free key at dashboard.cohere.com'}
                {platform === 'cloudflare' && 'Get a token at dash.cloudflare.com'}
                {platform === 'zhipu' && 'Get a key at open.bigmodel.cn'}
                {platform === 'ollama' && 'Get a key at ollama.com'}
              </p>
            </div>
          </>
        )}

        {platform && isAnonymous && (
          <div className="rounded-lg border border-dashed p-4 bg-muted/30">
            <p className="text-xs text-muted-foreground">
              <strong>{selectedPlatform?.label}</strong> works without an API key. Click "Add" to enable it.
            </p>
          </div>
        )}

        <div className="space-y-1.5">
          <Label className="text-sm">Label <span className="text-muted-foreground">(optional)</span></Label>
          <Input
            value={label}
            onChange={e => setLabel(e.target.value)}
            placeholder="e.g., my-groq-key"
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
          {addKey.isPending ? 'Adding…' : isAnonymous ? 'Enable Provider' : 'Add Key'}
        </Button>
      </form>

      {addedKeys.length > 0 && (
        <div className="space-y-3 mb-6">
          <h3 className="text-sm font-medium">Added providers ({addedKeys.length})</h3>
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
                  Remove
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} className="flex-1">
          Back
        </Button>
        <Button onClick={onNext} className="flex-1" disabled={addedKeys.length === 0}>
          Continue
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
        </Button>
      </div>

      {addedKeys.length === 0 && (
        <p className="text-xs text-muted-foreground text-center mt-3">
          Add at least one provider key to continue, or{' '}
          <button
            type="button"
            className="text-primary underline underline-offset-2"
            onClick={onNext}
          >
            skip for now
          </button>
        </p>
      )}
    </div>
  )
}

function UnifiedKeyStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
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
        <h2 className="text-xl font-semibold tracking-tight">Your Unified API Key</h2>
        <p className="text-sm text-muted-foreground mt-2">
          Use this key as your OpenAI <code className="font-mono text-xs bg-muted px-1 py-0.5 rounded">api_key</code> when making requests. It authenticates and routes through all your configured providers.
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 space-y-4">
        <div>
          <Label className="text-xs text-muted-foreground">API Key</Label>
          <div className="flex items-center gap-2 mt-1.5">
            <code className="flex-1 font-mono text-xs bg-muted px-3 py-2 rounded-md select-all truncate tabular-nums">
              {showKey ? apiKey : masked}
            </code>
            <Button variant="outline" size="sm" onClick={() => setShowKey(!showKey)}>
              {showKey ? 'Hide' : 'Show'}
            </Button>
            <Button variant="outline" size="sm" onClick={copy}>
              {copied ? 'Copied' : 'Copy'}
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
          <span className="text-muted-foreground">Base URL</span>
          <code className="font-mono text-xs">{baseUrl}</code>
          <span className="text-muted-foreground">Endpoint</span>
          <code className="font-mono text-xs">/v1/chat/completions</code>
        </div>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6">
        <h3 className="text-sm font-medium mb-3">Quick Start Example</h3>
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
          Back
        </Button>
        <Button onClick={onNext} className="flex-1">
          Continue
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
        </Button>
      </div>
    </div>
  )
}

function CompleteStep({ addedKeys }: { addedKeys: AddedKey[] }) {
  const navigate = useNavigate()

  return (
    <div className="max-w-lg text-center">
      <div className="mb-6">
        <div className="size-16 rounded-full bg-emerald-500/10 flex items-center justify-center mx-auto mb-4">
          <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-emerald-500"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
        </div>
        <h2 className="text-xl font-semibold tracking-tight">Setup Complete</h2>
        <p className="text-sm text-muted-foreground mt-2">
          FreeLLMAPI is ready to use. You've configured {addedKeys.length} provider{addedKeys.length !== 1 ? 's' : ''}.
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 text-left space-y-3">
        <h3 className="text-sm font-medium">What's next?</h3>
        <div className="space-y-2">
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
            <div>
              <p className="text-sm font-medium">Try the Playground</p>
              <p className="text-xs text-muted-foreground">Test your setup with a chat interface</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            <div>
              <p className="text-sm font-medium">Add more providers</p>
              <p className="text-xs text-muted-foreground">Visit the Keys page to add more API keys</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2"/><polyline points="22,6 12,13 2,6"/></svg>
            <div>
              <p className="text-sm font-medium">Configure fallback order</p>
              <p className="text-xs text-muted-foreground">Drag and drop to set provider priority</p>
            </div>
          </div>
        </div>
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={() => navigate('/keys')} className="flex-1">
          Go to Keys
        </Button>
        <Button onClick={() => navigate('/playground')} className="flex-1">
          Open Playground
        </Button>
      </div>
    </div>
  )
}

export default function SetupPage() {
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
        <div className="text-center mb-8">
          <div className="flex items-center justify-center gap-2 mb-2">
            <span className="inline-block size-2 rounded-full bg-foreground" />
            <span className="font-semibold tracking-tight">FreeLLMAPI</span>
          </div>
          <p className="text-xs text-muted-foreground">Initial Setup</p>
        </div>

        <StepIndicator current={step} total={steps.length} />

        <div className="flex justify-center">
          {steps[step]}
        </div>
      </div>
    </div>
  )
}
