import { Check, RefreshCw } from 'lucide-react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import type { SaveStatus as Status } from './use-fallback-chain'

/** Autosave feedback: changes save on their own, so this reports rather than asks. */
export function SaveStatus({ status, onRetry }: { status: Status; onRetry: () => void }) {
  const { t } = useTranslations()
  if (status === 'idle') return null

  if (status === 'error') {
    return (
      <div role="alert" className="flex items-center gap-2 text-xs text-destructive">
        {t('fallback.save.failed')}
        <Button variant="outline" size="sm" onClick={onRetry} className="h-6 px-2 text-xs">
          {t('fallback.save.retry')}
        </Button>
      </div>
    )
  }

  if (status === 'saved') {
    return (
      <p role="status" className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Check className="size-3.5" aria-hidden />
        {t('fallback.save.saved')}
      </p>
    )
  }

  return (
    <p role="status" className="flex items-center gap-1.5 text-xs text-muted-foreground">
      <RefreshCw className="size-3.5 animate-spin" aria-hidden />
      {t('fallback.save.saving')}
    </p>
  )
}
