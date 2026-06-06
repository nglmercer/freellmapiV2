'use client'

import { useTranslation } from 'react-i18next'
import { AlertTriangle, Info } from 'lucide-react'

import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

export type ConfirmVariant = 'default' | 'danger'

interface ConfirmDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  confirmLabel?: string
  cancelLabel?: string
  variant?: ConfirmVariant
  busy?: boolean
  onConfirm: () => void
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  cancelLabel,
  variant = 'default',
  busy = false,
  onConfirm,
}: ConfirmDialogProps) {
  const { t } = useTranslation()
  const Icon = variant === 'danger' ? AlertTriangle : Info
  const iconClass =
    variant === 'danger'
      ? 'text-amber-600 dark:text-amber-400'
      : 'text-muted-foreground'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup>
        <DialogHeader>
          <div className="flex items-start gap-3">
            <Icon className={`mt-0.5 size-5 shrink-0 ${iconClass}`} />
            <div className="min-w-0 space-y-1.5">
              <DialogTitle>{title}</DialogTitle>
              <DialogDescription>{description}</DialogDescription>
            </div>
          </div>
        </DialogHeader>
        <DialogFooter className="mt-2">
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            {cancelLabel ?? t('common.cancel')}
          </Button>
          <Button
            variant={variant === 'danger' ? 'destructive' : 'default'}
            onClick={onConfirm}
            disabled={busy}
          >
            {confirmLabel ?? t('common.confirm')}
          </Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  )
}
