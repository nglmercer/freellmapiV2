import { useNavigate } from 'react-router-dom'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import type { AddedKey } from './setup-model'

export function CompleteStep({ addedKeys }: { addedKeys: AddedKey[] }) {
  const { t } = useTranslations()
  const navigate = useNavigate()
  const count = addedKeys.length
  const providerPlural = count === 1 ? '' : 's'

  return (
    <div className="max-w-lg text-center">
      <div className="mb-6">
        <div className="size-16 rounded-full bg-emerald-500/10 flex items-center justify-center mx-auto mb-4">
          <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-emerald-500"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
        </div>
        <h2 className="text-xl font-semibold tracking-tight">{t('setup.complete.title')}</h2>
        <p className="text-sm text-muted-foreground mt-2">
          {t('setup.complete.description', { count, plural: providerPlural })}
        </p>
      </div>

      <div className="rounded-lg border bg-card p-5 mb-6 text-left space-y-3">
        <h3 className="text-sm font-medium">{t('setup.complete.whatsNext')}</h3>
        <div className="space-y-2">
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.tryPlayground')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.tryPlaygroundDesc')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.addMoreProviders')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.addMoreProvidersDesc')}</p>
            </div>
          </div>
          <div className="flex items-start gap-3">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-muted-foreground mt-0.5 shrink-0"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2"/><polyline points="22,6 12,13 2,6"/></svg>
            <div>
              <p className="text-sm font-medium">{t('setup.complete.configureFallback')}</p>
              <p className="text-xs text-muted-foreground">{t('setup.complete.configureFallbackDesc')}</p>
            </div>
          </div>
        </div>
      </div>

      <div className="flex gap-3">
        <Button variant="outline" onClick={() => navigate('/keys')} className="flex-1">
          {t('setup.complete.goToKeys')}
        </Button>
        <Button onClick={() => navigate('/playground')} className="flex-1">
          {t('setup.complete.openPlayground')}
        </Button>
      </div>
    </div>
  )
}
