import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'

export function WelcomeStep({ onNext }: { onNext: () => void }) {
  const { t } = useTranslations()
  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.welcome.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.welcome.description')}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 space-y-4">
        <h3 className="text-sm font-medium">{t('setup.welcome.whatYoullConfigure')}</h3>
        <div className="space-y-3">
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">1</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.providerKeys')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.providerKeysDescription')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">2</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.unifiedApiKey')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.unifiedApiKeyDescription')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <div className="size-6 rounded-full bg-primary/10 flex items-center justify-center shrink-0 mt-0.5">
              <span className="text-xs font-medium text-primary">3</span>
            </div>
            <div>
              <p className="text-sm font-medium">{t('setup.welcome.startUsing')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.welcome.startUsingDescription')}</p>
            </div>
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-dashed p-4 mb-6">
        <p className="text-xs text-muted-foreground">
          <strong>{t('setup.welcome.tipPrefix')}</strong> {t('setup.welcome.tipSuffix')}
        </p>
      </div>

      <Button onClick={onNext} className="w-full">
        {t('setup.welcome.button')}
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
      </Button>
    </div>
  )
}
