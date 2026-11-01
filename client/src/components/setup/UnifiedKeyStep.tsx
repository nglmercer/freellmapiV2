import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { ApiKeysPanel } from '@/components/api-keys-panel'

export function UnifiedKeyStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { t } = useTranslations()

  return (
    <div className="max-w-lg">
      <div className="mb-6">
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.unifiedKey.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.unifiedKey.description')}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6">
        <ApiKeysPanel />
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={onBack} className="flex-1">
          {t('setup.unifiedKey.back')}
        </Button>
        <Button onClick={onNext} className="flex-1">
          {t('setup.unifiedKey.continue')}
          <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="ml-2"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>
        </Button>
      </div>
    </div>
  )
}
