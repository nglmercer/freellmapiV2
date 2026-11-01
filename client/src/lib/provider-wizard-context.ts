import { createContext, useContext } from 'react'
import type { Platform } from '../../../shared/types'

export type WizardStep = 'providers' | 'key' | 'benchmarks'

export interface WizardStart {
  step?: WizardStep
  platform?: Platform
}

export interface ProviderWizardApi {
  open: (start?: WizardStart) => void
}

export const ProviderWizardContext = createContext<ProviderWizardApi | null>(null)

/** Opens the provider setup wizard from any page. */
export function useProviderWizard(): ProviderWizardApi {
  const api = useContext(ProviderWizardContext)
  if (!api) throw new Error('useProviderWizard must be used inside ProviderWizardProvider')
  return api
}
