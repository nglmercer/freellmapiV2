import { useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  BarChart3,
  Check,
  ChevronLeft,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  Eye,
  EyeOff,
  KeyRound,
  Layers,
  Plus,
  Power,
  RefreshCw,
  Search,
  Trash2,
  Zap,
} from 'lucide-react'
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
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { IconButton } from './icon-button'
import { Stepper, type StepperItem } from './stepper'
import { StatusIcon } from './status-badge'
import { Tooltip } from './tooltip'
import { keyStatusTone } from '@/lib/source-status'
import type { ApiKey, Platform } from '../../../shared/types'

interface BenchmarkStatus {
  id: string
  configured: boolean
  source: 'dashboard' | 'environment' | 'none'
  maskedKey: string | null
}

const STEPS: WizardStep[] = ['providers', 'key', 'benchmarks']
const STEP_ICONS = { providers: Layers, key: KeyRound, benchmarks: BarChart3 } as const

function errorText(error: unknown, fallback: string): string {
  if (error instanceof ApiError || error instanceof Error) return error.message || fallback
  return fallback
}

/** Owns the wizard's open state so any page can open it. */
export function ProviderWizardProvider({ children }: { children: ReactNode }) {
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

function Field({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={htmlFor} className="text-xs text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  )
}

/** A key input with an eye toggle inside the field. */
function SecretInput({
  id,
  value,
  onChange,
  placeholder,
  revealLabel,
  hideLabel,
}: {
  id: string
  value: string
  onChange: (value: string) => void
  placeholder: string
  revealLabel: string
  hideLabel: string
}) {
  const [reveal, setReveal] = useState(false)
  return (
    <div className="relative">
      <Input
        id={id}
        type={reveal ? 'text' : 'password'}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        className="pr-10 font-mono text-xs"
      />
      <div className="absolute right-1 top-1/2 -translate-y-1/2">
        <IconButton
          label={reveal ? hideLabel : revealLabel}
          icon={reveal ? EyeOff : Eye}
          onClick={() => setReveal((value) => !value)}
          className="size-7"
        />
      </div>
    </div>
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

  function flash(message: string) {
    setError(null)
    setNotice(message)
  }

  function fail(err: unknown) {
    setNotice(null)
    setError(errorText(err, t('wizard.failed')))
  }

  const addKey = useMutation({
    mutationFn: (body: { platform: Platform; key: string; label: string }) =>
      apiFetch('/api/keys', { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: () => {
      setKey('')
      setAccountId('')
      flash(t('wizard.saved'))
      refreshAll()
    },
    onError: fail,
  })

  const removeKey = useMutation({
    mutationFn: (id: number) => apiFetch(`/api/keys/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      flash(t('wizard.removed'))
      refreshAll()
    },
    onError: fail,
  })

  const saveBenchmark = useMutation({
    mutationFn: (value: string) =>
      apiFetch('/api/integrations/artificial-analysis', { method: 'PUT', body: JSON.stringify({ key: value }) }),
    onSuccess: () => {
      setBenchmarkKey('')
      flash(t('wizard.benchmark.saved'))
      refreshAll()
    },
    onError: fail,
  })

  const clearBenchmark = useMutation({
    mutationFn: () => apiFetch('/api/integrations/artificial-analysis', { method: 'DELETE' }),
    onSuccess: () => {
      flash(t('wizard.benchmark.removed'))
      refreshAll()
    },
    onError: fail,
  })

  const refreshSources = useMutation({
    mutationFn: () => apiFetch('/api/metadata/sync', { method: 'POST' }),
    onSuccess: () => {
      flash(t('wizard.benchmark.refreshed'))
      refreshAll()
      queryClient.invalidateQueries({ queryKey: ['metadata-explorer'] })
    },
    onError: fail,
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

  const query = search.trim().toLowerCase()
  const filtered = PLATFORMS.filter(
    (p) => !query || p.label.toLowerCase().includes(query) || p.value.includes(query),
  )
  const configured = filtered.filter((p) => (keyCounts.get(p.value) ?? 0) > 0)
  const available = filtered.filter((p) => (keyCounts.get(p.value) ?? 0) === 0)

  const stepIndex = STEPS.indexOf(step)
  const stepItems: StepperItem[] = STEPS.map((name, index) => ({
    id: name,
    label: t(`wizard.steps.${name}`),
    icon: STEP_ICONS[name],
    state: name === step ? 'active' : index < stepIndex ? 'done' : 'todo',
    disabled: name === 'key' && !platform,
  }))

  function goForward() {
    if (step === 'benchmarks') onClose()
    else if (step === 'providers') setStep('key')
    else setStep('benchmarks')
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogPopup className="w-[min(92vw,38rem)] max-w-none gap-5">
        <DialogHeader>
          <DialogTitle>{t('wizard.title')}</DialogTitle>
          <DialogDescription>{t('wizard.description')}</DialogDescription>
        </DialogHeader>

        <Stepper items={stepItems} onSelect={(id) => setStep(id as WizardStep)} label={t('wizard.title')} />

        <div className="min-h-[21rem] space-y-4">
          {error && (
            <div role="alert" className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm text-destructive">
              <CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
              <span className="min-w-0 break-words">{error}</span>
            </div>
          )}
          {notice && !error && (
            <div role="status" className="flex items-start gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/5 px-3 py-2 text-sm">
              <CircleCheck className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
              <span className="min-w-0 break-words">{notice}</span>
            </div>
          )}

          {step === 'providers' && (
            <div className="space-y-4">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
                <Input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder={t('wizard.search')}
                  aria-label={t('wizard.search')}
                  className="pl-9"
                />
              </div>
              {filtered.length === 0 && (
                <p className="py-6 text-center text-sm text-muted-foreground">{t('wizard.noMatch')}</p>
              )}
              <ProviderSection
                title={t('wizard.configured')}
                items={configured}
                keyCounts={keyCounts}
                onChoose={choose}
              />
              <ProviderSection
                title={t('wizard.available')}
                items={available}
                keyCounts={keyCounts}
                onChoose={choose}
              />
            </div>
          )}

          {step === 'key' && selected && (
            <div className="space-y-4">
              <div className="flex items-center gap-2">
                <h3 className="text-sm font-medium">{selected.label}</h3>
                {(keyCounts.get(selected.value) ?? 0) > 0 && (
                  <StatusIcon tone="success" icon={CircleCheck} label={t('wizard.configuredBadge')} />
                )}
              </div>

              {selected.anonymous ? (
                <div className="flex items-center justify-between gap-4 rounded-lg border p-4">
                  <p className="text-sm text-muted-foreground">{t('wizard.anonymousNote')}</p>
                  <Button onClick={() => platform && addKey.mutate({ platform, key: 'anonymous', label: '' })} disabled={addKey.isPending}>
                    <Power className="size-4" aria-hidden />
                    {addKey.isPending ? t('wizard.saving') : t('wizard.enable')}
                  </Button>
                </div>
              ) : (
                <form onSubmit={submitKey} className="space-y-4 rounded-lg border p-4">
                  {selected.needsAccountId && (
                    <Field label={t('wizard.accountId')} htmlFor="wizard-account">
                      <Input
                        id="wizard-account"
                        value={accountId}
                        onChange={(event) => setAccountId(event.target.value)}
                        placeholder={t('wizard.accountIdPlaceholder')}
                        autoComplete="off"
                      />
                    </Field>
                  )}
                  <Field label={t('wizard.apiKey')} htmlFor="wizard-key">
                    <SecretInput
                      id="wizard-key"
                      value={key}
                      onChange={setKey}
                      placeholder={t('wizard.keyPlaceholder')}
                      revealLabel={t('wizard.showKey')}
                      hideLabel={t('wizard.hideKey')}
                    />
                  </Field>
                  <Field label={t('wizard.label')} htmlFor="wizard-label">
                    <Input
                      id="wizard-label"
                      value={label}
                      onChange={(event) => setLabel(event.target.value)}
                      placeholder={t('wizard.labelPlaceholder')}
                    />
                  </Field>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-xs text-muted-foreground">{t('wizard.storedNote')}</span>
                    <Button
                      type="submit"
                      disabled={addKey.isPending || !key.trim() || (selected.needsAccountId && !accountId.trim())}
                    >
                      <KeyRound className="size-4" aria-hidden />
                      {addKey.isPending ? t('wizard.saving') : t('wizard.save')}
                    </Button>
                  </div>
                </form>
              )}

              <div className="space-y-2">
                <h4 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                  {t('wizard.existingKeys')}
                </h4>
                {platformKeys.length === 0 ? (
                  <p className="text-sm text-muted-foreground">{t('wizard.noKeysYet')}</p>
                ) : (
                  <ul className="divide-y rounded-md border">
                    {platformKeys.map((row) => (
                      <li key={row.id} className="flex items-center gap-3 px-3 py-2">
                        <StatusIcon
                          tone={keyStatusTone(row.status)}
                          icon={CircleCheck}
                          label={t(`keys.status.${row.status}`, { defaultValue: row.status })}
                        />
                        <code className="min-w-0 truncate font-mono text-xs">{row.maskedKey}</code>
                        {row.label && <span className="truncate text-xs text-muted-foreground">{row.label}</span>}
                        <div className="ml-auto">
                          <IconButton
                            label={t('wizard.removeKey')}
                            icon={Trash2}
                            tone="danger"
                            busy={removeKey.isPending}
                            onClick={() => removeKey.mutate(row.id)}
                          />
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          )}

          {step === 'benchmarks' && (
            <div className="space-y-4">
              <div className="flex items-start gap-3 rounded-lg border p-4">
                <BarChart3 className="mt-0.5 size-5 shrink-0 text-muted-foreground" aria-hidden />
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2">
                    <h3 className="text-sm font-medium">{t('wizard.benchmark.title')}</h3>
                    <StatusIcon
                      tone={benchmark.data?.configured ? 'success' : 'warning'}
                      icon={benchmark.data?.configured ? CircleCheck : KeyRound}
                      label={benchmark.data?.configured ? t('wizard.benchmark.configured') : t('wizard.benchmark.notConfigured')}
                    />
                  </div>
                  <p className="text-xs text-muted-foreground">{t('wizard.benchmark.description')}</p>
                  {benchmark.data?.maskedKey && (
                    <p className="font-mono text-xs text-muted-foreground">{benchmark.data.maskedKey}</p>
                  )}
                </div>
              </div>

              <form
                className="space-y-4"
                onSubmit={(event) => {
                  event.preventDefault()
                  if (benchmarkKey.trim()) saveBenchmark.mutate(benchmarkKey.trim())
                }}
              >
                <Field label={t('wizard.benchmark.keyLabel')} htmlFor="wizard-benchmark">
                  <SecretInput
                    id="wizard-benchmark"
                    value={benchmarkKey}
                    onChange={setBenchmarkKey}
                    placeholder={t('wizard.benchmark.placeholder')}
                    revealLabel={t('wizard.showKey')}
                    hideLabel={t('wizard.hideKey')}
                  />
                </Field>
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-1">
                    <IconButton
                      label={t('wizard.benchmark.refresh')}
                      icon={RefreshCw}
                      busy={refreshSources.isPending}
                      disabled={!benchmark.data?.configured}
                      onClick={() => refreshSources.mutate()}
                    />
                    {benchmark.data?.source === 'dashboard' && (
                      <IconButton
                        label={t('wizard.remove')}
                        icon={Trash2}
                        tone="danger"
                        busy={clearBenchmark.isPending}
                        onClick={() => clearBenchmark.mutate()}
                      />
                    )}
                  </div>
                  <Button type="submit" disabled={saveBenchmark.isPending || !benchmarkKey.trim()}>
                    <KeyRound className="size-4" aria-hidden />
                    {saveBenchmark.isPending ? t('wizard.saving') : t('wizard.save')}
                  </Button>
                </div>
              </form>
            </div>
          )}
        </div>

        <DialogFooter className="items-center justify-between gap-3 sm:justify-between">
          <div>
            {stepIndex > 0 && (
              <Button variant="outline" onClick={() => setStep(STEPS[stepIndex - 1])}>
                <ChevronLeft className="size-4" aria-hidden />
                {t('wizard.back')}
              </Button>
            )}
          </div>
          {step !== 'providers' && (
            <Button onClick={goForward} disabled={step === 'key' && !platform}>
              {step === 'benchmarks' ? <Check className="size-4" aria-hidden /> : null}
              {step === 'benchmarks' ? t('wizard.done') : t('wizard.next')}
              {step !== 'benchmarks' && <ChevronRight className="size-4" aria-hidden />}
            </Button>
          )}
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  )
}

function ProviderSection({
  title,
  items,
  keyCounts,
  onChoose,
}: {
  title: string
  items: PlatformInfo[]
  keyCounts: Map<string, number>
  onChoose: (platform: PlatformInfo) => void
}) {
  const { t } = useTranslations()
  if (items.length === 0) return null
  return (
    <section className="space-y-2">
      <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        <span>{title}</span>
        <span className="tabular-nums">{items.length}</span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        {items.map((item) => {
          const count = keyCounts.get(item.value) ?? 0
          return (
            <button
              key={item.value}
              type="button"
              onClick={() => onChoose(item)}
              className="flex h-11 items-center gap-3 rounded-lg border bg-background px-3 text-left transition-colors hover:border-foreground/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{item.label}</span>
              {count > 0 && (
                <Tooltip label={t('wizard.keyCount', { count })}>
                  <span
                    role="img"
                    aria-label={t('wizard.keyCount', { count })}
                    className="inline-flex items-center gap-1 text-xs tabular-nums text-emerald-600 dark:text-emerald-400"
                  >
                    <CircleCheck className="size-4" aria-hidden />
                    {count}
                  </span>
                </Tooltip>
              )}
              {count === 0 && item.anonymous && (
                <Tooltip label={t('wizard.anonymousShort')}>
                  <Zap className="size-4 text-muted-foreground" aria-label={t('wizard.anonymousShort')} />
                </Tooltip>
              )}
              {count === 0 && !item.anonymous && (
                <Tooltip label={t('wizard.add')}>
                  <Plus className="size-4 text-muted-foreground" aria-label={t('wizard.add')} />
                </Tooltip>
              )}
            </button>
          )
        })}
      </div>
    </section>
  )
}
