import { useMemo, useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { BarChart3, Check, ChevronLeft, ChevronRight, CircleAlert, CircleCheck, KeyRound, Layers } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { apiFetch, ApiError } from '@/lib/api'
import { PLATFORMS, platformInfo, type PlatformInfo } from '@/lib/platforms'
import type { WizardStart, WizardStep } from '@/lib/provider-wizard-context'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Stepper, type StepperItem } from './stepper'
import type { ApiKey, Platform } from '../../../shared/types'
import { ProvidersStep } from './provider-wizard-steps/ProvidersStep'
import { KeyStep } from './provider-wizard-steps/KeyStep'
import { BenchmarksStep, type BenchmarkStatus } from './provider-wizard-steps/BenchmarksStep'

const STEPS: WizardStep[] = ['providers', 'key', 'benchmarks']
const STEP_ICONS = { providers: Layers, key: KeyRound, benchmarks: BarChart3 } as const

function errorText(error: unknown, fallback: string): string {
  if (error instanceof ApiError || error instanceof Error) return error.message || fallback
  return fallback
}

export function ProviderWizard({ start, onClose }: { start: WizardStart; onClose: () => void }) {
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
            <ProvidersStep
              search={search}
              onSearchChange={setSearch}
              matchCount={filtered.length}
              configured={configured}
              available={available}
              keyCounts={keyCounts}
              onChoose={choose}
            />
          )}

          {step === 'key' && selected && (
            <KeyStep
              selected={selected}
              keyCount={keyCounts.get(selected.value) ?? 0}
              platformKeys={platformKeys}
              keyValue={key}
              onKeyChange={setKey}
              accountId={accountId}
              onAccountIdChange={setAccountId}
              label={label}
              onLabelChange={setLabel}
              onSubmit={submitKey}
              onEnableAnonymous={() => platform && addKey.mutate({ platform, key: 'anonymous', label: '' })}
              saving={addKey.isPending}
              onRemoveKey={(id) => removeKey.mutate(id)}
              removing={removeKey.isPending}
            />
          )}

          {step === 'benchmarks' && (
            <BenchmarksStep
              status={benchmark.data}
              value={benchmarkKey}
              onValueChange={setBenchmarkKey}
              onSave={(value) => saveBenchmark.mutate(value)}
              saving={saveBenchmark.isPending}
              onRefresh={() => refreshSources.mutate()}
              refreshing={refreshSources.isPending}
              onClear={() => clearBenchmark.mutate()}
              clearing={clearBenchmark.isPending}
            />
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
