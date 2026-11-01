import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'

/** Explains the entries the chain hides by default, and toggles them into view. */
export function HiddenNotice({
  count,
  providers,
  showing,
  onToggle,
}: {
  count: number
  providers: string
  showing: boolean
  onToggle: () => void
}) {
  const { t } = useTranslations()
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-dashed px-4 py-3">
      <p className="text-sm text-muted-foreground">
        {showing
          ? t('fallback.unconfiguredShown', { count })
          : t('fallback.unconfiguredNote', { count, platforms: providers })}
      </p>
      <Button variant="outline" size="sm" onClick={onToggle}>
        {showing ? t('fallback.hideUnconfigured') : t('fallback.showUnconfigured', { count })}
      </Button>
    </div>
  )
}
