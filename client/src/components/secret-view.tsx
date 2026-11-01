import { useState, type ReactNode } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from '@/components/ui/dialog'

/** A value shown once, with copy buttons. Used for API keys and join codes. */
export function SecretView({
  secret,
  link,
  note,
}: {
  secret: string
  link?: { label: string; value: string }
  note: ReactNode
}) {
  const { t } = useTranslations()
  const [copied, setCopied] = useState<'secret' | 'link' | null>(null)

  function copy(which: 'secret' | 'link', value: string) {
    navigator.clipboard.writeText(value)
    setCopied(which)
    window.setTimeout(() => setCopied(null), 1500)
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">{note}</p>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 select-all break-all rounded-md bg-muted px-3 py-2 font-mono text-sm">
          {secret}
        </code>
        <Button variant="outline" size="sm" onClick={() => copy('secret', secret)}>
          {copied === 'secret' ? t('keys.apiKeys.copied') : t('keys.apiKeys.copy')}
        </Button>
      </div>
      {link && (
        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{link.label}</div>
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 select-all break-all rounded-md bg-muted px-3 py-2 font-mono text-xs">
              {link.value}
            </code>
            <Button variant="outline" size="sm" onClick={() => copy('link', link.value)}>
              {copied === 'link' ? t('keys.apiKeys.copied') : t('keys.joinCode.copyLink')}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

/** The one-time value in a dialog that the user closes when done. */
export function SecretDialog({
  open,
  onOpenChange,
  title,
  description,
  secret,
  link,
  note,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  secret: string
  link?: { label: string; value: string }
  note: ReactNode
}) {
  const { t } = useTranslations()
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <SecretView secret={secret} link={link} note={note} />
        <DialogFooter className="mt-2">
          <Button onClick={() => onOpenChange(false)}>{t('keys.joinCode.dismiss')}</Button>
        </DialogFooter>
      </DialogPopup>
    </Dialog>
  )
}
