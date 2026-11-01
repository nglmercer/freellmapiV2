import type { FormEvent } from 'react'
import { CircleCheck, KeyRound, Power, Trash2 } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { keyStatusTone } from '@/lib/source-status'
import type { PlatformInfo } from '@/lib/platforms'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { IconButton } from '../icon-button'
import { StatusIcon } from '../status-badge'
import type { ApiKey } from '../../../../shared/types'
import { Field, SecretInput } from './fields'

interface KeyStepProps {
  selected: PlatformInfo
  /** Keys already stored for this provider. */
  keyCount: number
  platformKeys: ApiKey[]
  keyValue: string
  onKeyChange: (value: string) => void
  accountId: string
  onAccountIdChange: (value: string) => void
  label: string
  onLabelChange: (value: string) => void
  onSubmit: (event: FormEvent) => void
  onEnableAnonymous: () => void
  saving: boolean
  onRemoveKey: (id: number) => void
  removing: boolean
}

/** Step 2: store an API key (or enable an anonymous provider) and manage existing keys. */
export function KeyStep({
  selected,
  keyCount,
  platformKeys,
  keyValue,
  onKeyChange,
  accountId,
  onAccountIdChange,
  label,
  onLabelChange,
  onSubmit,
  onEnableAnonymous,
  saving,
  onRemoveKey,
  removing,
}: KeyStepProps) {
  const { t } = useTranslations()
  return (
      <div className="space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium">{selected.label}</h3>
          {keyCount > 0 && (
            <StatusIcon tone="success" icon={CircleCheck} label={t('wizard.configuredBadge')} />
          )}
        </div>
  
        {selected.anonymous ? (
          <div className="flex items-center justify-between gap-4 rounded-lg border p-4">
            <p className="text-sm text-muted-foreground">{t('wizard.anonymousNote')}</p>
            <Button onClick={onEnableAnonymous} disabled={saving}>
              <Power className="size-4" aria-hidden />
              {saving ? t('wizard.saving') : t('wizard.enable')}
            </Button>
          </div>
        ) : (
          <form onSubmit={onSubmit} className="space-y-4 rounded-lg border p-4">
            {selected.needsAccountId && (
              <Field label={t('wizard.accountId')} htmlFor="wizard-account">
                <Input
                  id="wizard-account"
                  value={accountId}
                  onChange={(event) => onAccountIdChange(event.target.value)}
                  placeholder={t('wizard.accountIdPlaceholder')}
                  autoComplete="off"
                />
              </Field>
            )}
            <Field label={t('wizard.apiKey')} htmlFor="wizard-key">
              <SecretInput
                id="wizard-key"
                value={keyValue}
                onChange={onKeyChange}
                placeholder={t('wizard.keyPlaceholder')}
                revealLabel={t('wizard.showKey')}
                hideLabel={t('wizard.hideKey')}
              />
            </Field>
            <Field label={t('wizard.label')} htmlFor="wizard-label">
              <Input
                id="wizard-label"
                value={label}
                onChange={(event) => onLabelChange(event.target.value)}
                placeholder={t('wizard.labelPlaceholder')}
              />
            </Field>
            <div className="flex items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">{t('wizard.storedNote')}</span>
              <Button
                type="submit"
                disabled={saving || !keyValue.trim() || (selected.needsAccountId && !accountId.trim())}
              >
                <KeyRound className="size-4" aria-hidden />
                {saving ? t('wizard.saving') : t('wizard.save')}
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
                      busy={removing}
                      onClick={() => onRemoveKey(row.id)}
                    />
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
  )
}
