import { BarChart3, CircleCheck, KeyRound, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { IconButton } from '../icon-button'
import { StatusIcon } from '../status-badge'
import { Field, SecretInput } from './fields'

export interface BenchmarkStatus {
  id: string
  configured: boolean
  source: 'dashboard' | 'environment' | 'none'
  maskedKey: string | null
}

interface BenchmarksStepProps {
  status: BenchmarkStatus | undefined
  value: string
  onValueChange: (value: string) => void
  onSave: (value: string) => void
  saving: boolean
  onRefresh: () => void
  refreshing: boolean
  onClear: () => void
  clearing: boolean
}

/** Step 3: configure the optional Artificial Analysis benchmark key. */
export function BenchmarksStep({
  status,
  value,
  onValueChange,
  onSave,
  saving,
  onRefresh,
  refreshing,
  onClear,
  clearing,
}: BenchmarksStepProps) {
  const { t } = useTranslations()
  return (
      <div className="space-y-4">
        <div className="flex items-start gap-3 rounded-lg border p-4">
          <BarChart3 className="mt-0.5 size-5 shrink-0 text-muted-foreground" aria-hidden />
          <div className="min-w-0 flex-1 space-y-1">
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-medium">{t('wizard.benchmark.title')}</h3>
              <StatusIcon
                tone={status?.configured ? 'success' : 'warning'}
                icon={status?.configured ? CircleCheck : KeyRound}
                label={status?.configured ? t('wizard.benchmark.configured') : t('wizard.benchmark.notConfigured')}
              />
            </div>
            <p className="text-xs text-muted-foreground">{t('wizard.benchmark.description')}</p>
            {status?.maskedKey && (
              <p className="font-mono text-xs text-muted-foreground">{status.maskedKey}</p>
            )}
          </div>
        </div>
  
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            if (value.trim()) onSave(value.trim())
          }}
        >
          <Field label={t('wizard.benchmark.keyLabel')} htmlFor="wizard-benchmark">
            <SecretInput
              id="wizard-benchmark"
              value={value}
              onChange={onValueChange}
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
                busy={refreshing}
                disabled={!status?.configured}
                onClick={() => onRefresh()}
              />
              {status?.source === 'dashboard' && (
                <IconButton
                  label={t('wizard.remove')}
                  icon={Trash2}
                  tone="danger"
                  busy={clearing}
                  onClick={() => onClear()}
                />
              )}
            </div>
            <Button type="submit" disabled={saving || !value.trim()}>
              <KeyRound className="size-4" aria-hidden />
              {saving ? t('wizard.saving') : t('wizard.save')}
            </Button>
          </div>
        </form>
      </div>
  )
}
