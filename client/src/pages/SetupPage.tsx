import { useState } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { LanguageToggle } from '@/app/header-controls'
import { StepIndicator } from '@/components/setup/StepIndicator'
import { WelcomeStep } from '@/components/setup/WelcomeStep'
import { AddKeysStep } from '@/components/setup/AddKeysStep'
import { UnifiedKeyStep } from '@/components/setup/UnifiedKeyStep'
import { CompleteStep } from '@/components/setup/CompleteStep'
import type { AddedKey } from '@/components/setup/setup-model'

export default function SetupPage() {
  const { t } = useTranslations()
  const [step, setStep] = useState(0)
  const [addedKeys, setAddedKeys] = useState<AddedKey[]>([])

  const steps = [
    <WelcomeStep onNext={() => setStep(1)} />,
    <AddKeysStep
      addedKeys={addedKeys}
      onAdd={(key) => setAddedKeys(prev => [...prev, key])}
      onRemove={(index) => setAddedKeys(prev => prev.filter((_, i) => i !== index))}
      onNext={() => setStep(2)}
      onBack={() => setStep(0)}
    />,
    <UnifiedKeyStep onNext={() => setStep(3)} onBack={() => setStep(1)} />,
    <CompleteStep addedKeys={addedKeys} />,
  ]

  return (
    <div className="min-h-[calc(100vh-4rem)] flex flex-col items-center justify-center -mt-8">
      <div className="w-full max-w-2xl px-6">
        <div className="flex items-center justify-between mb-8">
          <div className="flex items-center gap-2">
            <span className="inline-block size-2 rounded-full bg-foreground" />
            <span className="font-semibold tracking-tight">{t('app.brand')}</span>
          </div>
          <LanguageToggle />
        </div>
        <p className="text-xs text-muted-foreground text-center mb-8">{t('app.initialSetup')}</p>

        <StepIndicator current={step} total={steps.length} />

        <div className="flex justify-center">
          {steps[step]}
        </div>
      </div>
    </div>
  )
}
