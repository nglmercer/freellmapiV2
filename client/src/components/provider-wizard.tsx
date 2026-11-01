import { useMemo, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Eye, EyeOff, Trash2 } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch, ApiError } from '@/lib/api'
import { PLATFORMS, platformInfo, type PlatformInfo } from '@/lib/platforms'
import {
  ProviderWizardContext,
  type WizardStart,
  type WizardStep,
} from '@/lib/provider-wizard-context'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'
import type { ApiKey, Platform } from '../../../shared/types'

interface BenchmarkStatus {
  id: string
  configured: boolean
  source: 'dashboard' | 'environment' | 'none'
  maskedKey: string | null
}

const STEPS: WizardStep[] = ['providers', 'key', 'benchmarks']

function errorText(error: unknown, fallback: string): string {
  if (error instanceof ApiError || error instanceof Error) return error.message || fallback
  return fallback
}

/** Owns the wizard's open state so any page can open it. */
export function ProviderWizardProvider({ children }: { children: React.ReactNode }) {
  const [start, setStart] = useState<WizardStart | null>(null)
  const api = useMemo(
    () => ({
      open: (next: WizardStart = {}) =>
        setStart({
          step: next.step ?? (next.platform ? 'key' : 'providers'),
          platform: next.platform,
        }),
    }),
    [],
  )
  return (
    <ProviderWizardContext.Provider value={api}>
      {children}
      {start && (
        <ProviderWizard
          key={`${start.step}-${start.platform ?? ''}`}
          start={start}
          onClose={() => setStart(null)}
        />
      )}
    </ProviderWizardContext.Provider>
  )
}

function ProviderWizard({ start, onClose }: { start: WizardStart; onClose: () => void }) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const [step, setStep] = useState<WizardStep>(start.step ?? 'providers')
  const [platform, setPlatform] = useState<Platform | null>(start.platform ?? null)
  const [search, setSearch] = useState('')
  const [key, setKey] = useState('')
  const [label, setLabel] = useState('')
  const [accountId, setAccountId] = useState('')
  const [reveal, setReveal] = useState(false)
  const [benchmarkKey, setBenchmarkKey] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const keys = useQuery<ApiKey[]>({
    queryKey: ['keys'],
    queryFn: () => apiFetch('/api/keys'),
  })
  const benchmark = useQuery<BenchmarkStatus>({
    queryKey: ['integrations', 'artificial-analysis'],
    queryFn: () => apiFetch('/api/integrations/artificial-analysis'),
  })

  const keyCounts = useMemo(() => {
    const counts = new Map<string, number>()
    for (const row of keys.data ?? []) {
      counts.set(row.platform, (counts.get(row.platform) ?? 0) + 1)
    }
    return counts
  }, [keys.data])

  const selected: PlatformInfo | undefined = platform ? platformInfo(platform) : undefined
  const platformKeys = (keys.data ?? []).filter((row) => row.platform === platform)

  function refreshAll() {
    for (const queryKey of [['keys'], ['fallback'], ['health'], ['integrations'], ['metadata-sources']]) {
      queryClient.invalidateQueries({ queryKey })
    }
  }

  const addKey = useMutation({
    mutationFn: (body: { platform: Platform; key: string; label: string }) =>
      apiFetch('/api/keys', { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: () => {
      setKey('')
      setAccountId('')
      setError(null)
      setNotice(t('wizard.saved'))
      refreshAll()
    },
    onError: (err) => setError(errorText(err, t('wizard.failed'))),
  })

  const removeKey = useMutation({
    mutationFn: (id: number) => apiFetch(`/api/keys/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      setNotice(t('wizard.removed'))
      refreshAll()
    },
    onError: (err) => setError(errorText(err, t('wizard.failed'))),
  })

  const saveBenchmark = useMutation({
    mutationFn: (value: string) =>
      apiFetch('/api/integrations/artificial-analysis', {
        method: 'PUT',
        body: JSON.stringify({ key: value }),
      }),
    onSuccess: () => {
      setBenchmarkKey('')
      setError(null)
      setNotice(t('wizard.benchmark.saved'))
      refreshAll()
    },
    onError: (err) => setError(errorText(err, t('wizard.failed'))),
  })

  const clearBenchmark = useMutation({
    mutationFn: () => apiFetch('/api/integrations/artificial-analysis', { method: 'DELETE' }),
    onSuccess: () => {
      setNotice(t('wizard.benchmark.removed'))
      refreshAll()
    },
    onError: (err) => setError(errorText(err, t('wizard.failed'))),
  })

  const refreshSources = useMutation({
    mutationFn: () => apiFetch('/api/metadata/sync', { method: 'POST' }),
    onSuccess: () => {
      setError(null)
      setNotice(t('wizard.benchmark.refreshed'))
      refreshAll()
      queryClient.invalidateQueries({ queryKey: ['metadata-explorer'] })
    },
    onError: (err) => setError(errorText(err, t('wizard.failed'))),
  })

  function choose(next: PlatformInfo) {
    setPlatform(next.value)
    setStep('key')
    setKey('')
    setLabel('')
    setAccountId('')
    setError(null)
    setNotice(null)
  }

  function submitKey(event: FormEvent) {
    event.preventDefault()
    if (!platform || !selected) return
    const secret = selected.needsAccountId ? `${accountId.trim()}:${key.trim()}` : key.trim()
    if (!secret || (selected.needsAccountId && !accountId.trim())) return
    addKey.mutate({ platform, key: secret, label: label.trim() })
  }

  function enableAnonymous() {
    if (!platform) return
    addKey.mutate({ platform, key: 'anonymous', label: '' })
  }

  const filtered = PLATFORMS.filter((p) => {
    const q = search.trim().toLowerCase()
    return !q || p.label.toLowerCase().includes(q) || p.value.includes(q)
  })
  const configured = filtered.filter((p) => (keyCounts.get(p.value) ?? 0) > 0)
  const available = filtered.filter((p) => (keyCounts.get(p.value) ?? 0) === 0)

  const stepIndex = STEPS.indexOf(step)
  const canGoBack = stepIndex > 0
  const forwardLabel = step === 'benchmarks' ? t('wizard.done') : t('wizard.next')

  function goForward() {
    if (step === 'benchmarks') onClose()
    else if (step === 'providers') setStep('key')
    else setStep('benchmarks')
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogPopup className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('wizard.title')}</DialogTitle>
          <DialogDescription>{t('wizard.description')}</DialogDescription>
        </DialogHeader>

        <ol className="flex flex-wrap gap-2" aria-label={t('wizard.title')}>
          {STEPS.map((name, index) => {
            const active = name === step
            const done = index < stepIndex
            const disabled = name === 'key' && !platform
            return (
              <li key={name}>
                <button
                  type="button"
                  disabled={disabled}
                  onClick={() => setStep(name)}
                  aria-current={active ? 'step' : undefined}
                  className={cn(
                    'flex items-center gap-2 rounded-full border px-3 py-1 text-xs transition-colors disabled:opacity-40',
                    active && 'border-foreground bg-foreground text-background',
                    !active && done && 'border-emerald-500/50 text-emerald-600 dark:text-emerald-400',
                    !active && !done && 'text-muted-foreground hover:text-foreground',
                  )}
                >
                  <span className="font-mono">{done ? <Check className="size-3" aria-hidden /> : index + 1}</span>
                  {t(`wizard.steps.${name}`)}
                </button>
              </li>
            )
          })}
        </ol>

        {error && (
          <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
            {error}
          </p>
        )}
        {notice && !error && (
          <p role="status" className="rounded-md border border-emerald-500/40 bg-emerald-500/5 px-3 py-2 text-sm">
            {notice}
          </p>
        )}

        {step === 'providers' && (
          <div className="space-y-4">
            <Input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t('wizard.search')}
              aria-label={t('wizard.search')}
            />
            <ProviderGroup title={t('wizard.configured')} items={configured} keyCounts={keyCounts} onChoose={choose} t={t} />
            <ProviderGroup title={t('wizard.available')} items={available} keyCounts={keyCounts} onChoose={choose} t={t} />
          </div>
        )}

        {step === 'key' && selected && (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-medium">{selected.label}</h3>
              {(keyCounts.get(selected.value) ?? 0) > 0 && (
                <Badge variant="secondary">{t('wizard.configuredBadge')}</Badge>
              )}
            </div>

            {selected.anonymous ? (
              <div className="space-y-3 rounded-md border p-4">
                <p className="text-sm text-muted-foreground">{t('wizard.anonymousNote')}</p>
                <Button onClick={enableAnonymous} disabled={addKey.isPending}>
                  {addKey.isPending ? t('wizard.saving') : t('wizard.enable')}
                </Button>
              </div>
            ) : (
              <form onSubmit={submitKey} className="space-y-4 rounded-md border p-4">
                {selected.needsAccountId && (
                  <div className="space-y-1.5">
                    <Label htmlFor="wizard-account">{t('wizard.accountId')}</Label>
                    <Input
                      id="wizard-account"
                      value={accountId}
                      onChange={(event) => setAccountId(event.target.value)}
                      placeholder={t('wizard.accountIdPlaceholder')}
                      autoComplete="off"
                    />
                  </div>
                )}
                <div className="space-y-1.5">
                  <Label htmlFor="wizard-key">{t('wizard.apiKey')}</Label>
                  <div className="flex gap-2">
                    <Input
                      id="wizard-key"
                      type={reveal ? 'text' : 'password'}
                      value={key}
                      onChange={(event) => setKey(event.target.value)}
                      placeholder={t('wizard.keyPlaceholder')}
                      autoComplete="off"
                      spellCheck={false}
                    />
                    <Button
                      type="button"
                      variant="outline"
                      size="icon"
                      onClick={() => setReveal((value) => !value)}
                      aria-label={reveal ? t('wizard.hideKey') : t('wizard.showKey')}
                    >
                      {reveal ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
                    </Button>
                  </div>
                  <p className="text-xs text-muted-foreground">{t('wizard.storedNote')}</p>
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="wizard-label">{t('wizard.label')}</Label>
                  <Input
                    id="wizard-label"
                    value={label}
                    onChange={(event) => setLabel(event.target.value)}
                    placeholder={t('wizard.labelPlaceholder')}
                  />
                </div>
                <Button
                  type="submit"
                  disabled={addKey.isPending || !key.trim() || (selected.needsAccountId && !accountId.trim())}
                >
                  {addKey.isPending ? t('wizard.saving') : t('wizard.save')}
                </Button>
              </form>
            )}

            <div className="space-y-2">
              <h4 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {t('wizard.existingKeys')}
              </h4>
              {platformKeys.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t('wizard.noKeysYet')}</p>
              ) : (
                <ul className="divide-y rounded-md border">
                  {platformKeys.map((row) => (
                    <li key={row.id} className="flex items-center gap-3 px-3 py-2 text-sm">
                      <span className="font-mono text-xs">{row.maskedKey}</span>
                      {row.label && <span className="text-muted-foreground">{row.label}</span>}
                      <Badge variant="outline" className="ml-auto">{row.status}</Badge>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => removeKey.mutate(row.id)}
                        disabled={removeKey.isPending}
                        aria-label={t('wizard.remove')}
                      >
                        <Trash2 className="size-4" />
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}

        {step === 'benchmarks' && (
          <div className="space-y-4">
            <div className="space-y-2 rounded-md border p-4">
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-sm font-medium">{t('wizard.benchmark.title')}</h3>
                <Badge variant={benchmark.data?.configured ? 'secondary' : 'outline'}>
                  {benchmark.data?.configured
                    ? t('wizard.benchmark.configured')
                    : t('wizard.benchmark.notConfigured')}
                </Badge>
              </div>
              <p className="text-sm text-muted-foreground">{t('wizard.benchmark.description')}</p>
              {benchmark.data?.maskedKey && (
                <p className="font-mono text-xs text-muted-foreground">{benchmark.data.maskedKey}</p>
              )}
            </div>

            <form
              onSubmit={(event) => {
                event.preventDefault()
                if (benchmarkKey.trim()) saveBenchmark.mutate(benchmarkKey.trim())
              }}
              className="space-y-3"
            >
              <div className="space-y-1.5">
                <Label htmlFor="wizard-benchmark">{t('wizard.benchmark.keyLabel')}</Label>
                <Input
                  id="wizard-benchmark"
                  type="password"
                  value={benchmarkKey}
                  onChange={(event) => setBenchmarkKey(event.target.value)}
                  placeholder={t('wizard.benchmark.placeholder')}
                  autoComplete="off"
                  spellCheck={false}
                />
                <p className="text-xs text-muted-foreground">{t('wizard.storedNote')}</p>
              </div>
              <div className="flex flex-wrap gap-2">
                <Button type="submit" disabled={saveBenchmark.isPending || !benchmarkKey.trim()}>
                  {saveBenchmark.isPending ? t('wizard.saving') : t('wizard.save')}
                </Button>
                {benchmark.data?.source === 'dashboard' && (
                  <Button type="button" variant="outline" onClick={() => clearBenchmark.mutate()} disabled={clearBenchmark.isPending}>
                    {t('wizard.remove')}
                  </Button>
                )}
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => refreshSources.mutate()}
                  disabled={refreshSources.isPending || !benchmark.data?.configured}
                >
                  {refreshSources.isPending ? t('wizard.benchmark.refreshing') : t('wizard.benchmark.refresh')}
                </Button>
              </div>
            </form>
          </div>
        )}

        <DialogFooter className="mt-2">
          {canGoBack && (
            <Button variant="outline" onClick={() => setStep(STEPS[stepIndex - 1])}>
              {t('wizard.back')}
            </Button>
          )}
          {step !== 'providers' && (
            <Button onClick={goForward} disabled={step === 'key' && !platform}>
              {forwardLabel}
            </Button>
          )}
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  )
}

function ProviderGroup({
  title,
  items,
  keyCounts,
  onChoose,
  t,
}: {
  title: string
  items: PlatformInfo[]
  keyCounts: Map<string, number>
  onChoose: (platform: PlatformInfo) => void
  t: (key: string, options?: Record<string, unknown>) => string
}) {
  if (items.length === 0) return null
  return (
    <section className="space-y-2">
      <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
        {title} · {items.length}
      </h3>
      <div className="grid gap-2 sm:grid-cols-2">
        {items.map((item) => {
          const count = keyCounts.get(item.value) ?? 0
          return (
            <button
              key={item.value}
              type="button"
              onClick={() => onChoose(item)}
              className="flex items-center gap-3 rounded-md border bg-background px-3 py-2.5 text-left transition-colors hover:border-foreground/40 hover:bg-muted/40"
            >
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">{item.label}</span>
                <span className="block text-xs text-muted-foreground">
                  {count > 0
                    ? t('wizard.keyCount', { count })
                    : item.anonymous
                      ? t('wizard.anonymousShort')
                      : t('wizard.noKey')}
                </span>
              </span>
              {count > 0 && <Check className="size-4 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />}
            </button>
          )
        })}
      </div>
    </section>
  )
}
