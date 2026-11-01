import { lazy, Suspense, useMemo, useState, type ReactNode } from 'react'
import { ProviderWizardContext, type WizardStart } from '@/lib/provider-wizard-context'

// The dialog and its step components only matter once a user opens the wizard,
// so they stay out of the entry chunk and load on first open.
const ProviderWizard = lazy(() =>
  import('./provider-wizard').then((m) => ({ default: m.ProviderWizard })),
)

/** Owns the wizard's open state so any page can open it. */
export function ProviderWizardProvider({ children }: { children: ReactNode }) {
  const [start, setStart] = useState<WizardStart | null>(null)
  const api = useMemo(
    () => ({
      open: (next: WizardStart = {}) =>
        setStart({
          step: next.step ?? (next.platform ? 'key' : 'providers'),
          platform: next.platform,
        }),
    }),
    [],
  )
  return (
    <ProviderWizardContext.Provider value={api}>
      {children}
      {start && (
        <Suspense fallback={null}>
          <ProviderWizard
            key={`${start.step}-${start.platform ?? ''}`}
            start={start}
            onClose={() => setStart(null)}
          />
        </Suspense>
      )}
    </ProviderWizardContext.Provider>
  )
}
