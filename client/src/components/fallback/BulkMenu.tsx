import { useState } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { ConfirmDialog } from '@/components/confirm-dialog'
import { ActionMenu, ActionMenuItem, ActionMenuSeparator } from './ActionMenu'
import type { BulkAction } from './chain-model'

/**
 * Bulk enable and disable. Each action asks for confirmation first, and the
 * destructive one is set apart so it is not the first thing a click reaches.
 */
export function BulkMenu({
  disabled,
  hasFree,
  pending,
  onRun,
}: {
  disabled: boolean
  hasFree: boolean
  pending: boolean
  onRun: (action: BulkAction) => Promise<unknown>
}) {
  const { t } = useTranslations()
  const [action, setAction] = useState<BulkAction | null>(null)

  function confirm() {
    if (!action) return
    // Errors are reported by the chain hook, so both outcomes just close the dialog.
    void onRun(action).then(
      () => setAction(null),
      () => setAction(null),
    )
  }

  return (
    <>
      <ActionMenu label={t('fallback.bulk.title')} disabled={disabled}>
        <ActionMenuItem onSelect={() => setAction('enableAll')}>{t('fallback.bulk.enableAll')}</ActionMenuItem>
        {hasFree && (
          <ActionMenuItem onSelect={() => setAction('enableFree')}>{t('fallback.bulk.enableFree')}</ActionMenuItem>
        )}
        <ActionMenuSeparator />
        <ActionMenuItem tone="danger" onSelect={() => setAction('disableAll')}>
          {t('fallback.bulk.disableAll')}
        </ActionMenuItem>
      </ActionMenu>

      {action && (
        <ConfirmDialog
          open
          onOpenChange={(open) => {
            if (!open && !pending) setAction(null)
          }}
          title={t(`fallback.bulk.confirm.title.${action}`)}
          description={t(`fallback.bulk.confirm.${action}`)}
          confirmLabel={t('fallback.bulk.proceed')}
          cancelLabel={t('fallback.bulk.cancel')}
          variant={action === 'disableAll' ? 'danger' : 'default'}
          busy={pending}
          onConfirm={confirm}
        />
      )}
    </>
  )
}
