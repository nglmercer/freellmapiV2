import { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Disclosure } from '@/components/disclosure'
import { selectClass } from './styles'
import type { Decision } from './types'

export function Simulator() {
  const { t } = useTranslations()
  const [strategy, setStrategy] = useState('balanced')
  const [capability, setCapability] = useState('')
  const [free, setFree] = useState(false)
  const [input, setInput] = useState(1000)
  const [output, setOutput] = useState(500)
  const [maxCost, setMaxCost] = useState('')
  const [providers, setProviders] = useState('')
  const [weights, setWeights] = useState({
    quality: 0.35,
    speed: 0.15,
    reliability: 0.2,
    latency: 0.1,
    price: 0.1,
    availability: 0.1,
  })
  const policy = {
    strategy,
    requiredCapabilities: capability ? [capability] : [],
    freeOnly: free,
    ...(maxCost ? { maxCostUsd: Number(maxCost) } : {}),
    allowedProviders: providers
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
    weights,
  }
  const save = useMutation({
    mutationFn: () =>
      apiFetch('/api/routing/policy', {
        method: 'PATCH',
        body: JSON.stringify(policy),
      }),
  })
  const mutation = useMutation<{
    selectedModelId: number | null
    decisions: Decision[]
  }>({
    mutationFn: () =>
      apiFetch('/api/routing/simulate', {
        method: 'POST',
        body: JSON.stringify({
          ...policy,
          inputTokens: input,
          outputTokens: output,
        }),
      }),
  })
  return (
    <section className="space-y-4">
      <h2 className="font-medium">{t('metadata.simulator')}</h2>
      <div className="flex flex-wrap gap-3 items-end">
        <label className="text-xs">
          {t('metadata.strategy')}
          <select
            className={`${selectClass} block`}
            value={strategy}
            onChange={(e) => setStrategy(e.target.value)}
          >
            {[
              'manual',
              'balanced',
              'quality',
              'fastest',
              'most-reliable',
              'lowest-latency',
              'cheapest',
              'best-coding',
              'best-reasoning',
              'free-only',
              'weighted',
            ].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          {t('metadata.capability')}
          <select
            className={`${selectClass} block`}
            value={capability}
            onChange={(e) => setCapability(e.target.value)}
          >
            <option value="">{t('metadata.any')}</option>
            {['tools', 'vision', 'reasoning', 'structured_output'].map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="text-xs">
          {t('metadata.inputTokens')}
          <Input
            className="w-28"
            type="number"
            min={0}
            value={input}
            onChange={(e) => setInput(Number(e.target.value))}
          />
        </label>
        <label className="text-xs">
          {t('metadata.outputTokens')}
          <Input
            className="w-28"
            type="number"
            min={0}
            value={output}
            onChange={(e) => setOutput(Number(e.target.value))}
          />
        </label>
        <label className="text-sm flex gap-2">
          <input
            type="checkbox"
            checked={free}
            onChange={(e) => setFree(e.target.checked)}
          />
          {t('metadata.free')}
        </label>
        <Button disabled={mutation.isPending} onClick={() => mutation.mutate()}>
          {t('metadata.simulate')}
        </Button>
      </div>
      <Disclosure
        className="rounded-md border"
        summary={<span className="text-sm">{t('metadata.advanced')}</span>}
        summaryClassName="px-3 py-2"
      >
        <div className="p-3">
        <div className="flex flex-wrap gap-3 py-3">
          <label className="text-xs">
            {t('metadata.maxCost')}
            <Input
              className="w-40"
              min={0}
              type="number"
              step="any"
              value={maxCost}
              onChange={(e) => setMaxCost(e.target.value)}
            />
          </label>
          <label className="text-xs">
            {t('metadata.allowedProviders')}
            <Input
              className="w-64"
              value={providers}
              onChange={(e) => setProviders(e.target.value)}
            />
          </label>
          {Object.entries(weights).map(([key, value]) => (
            <label key={key} className="text-xs">
              {key}
              <Input
                className="w-24"
                type="number"
                min={0}
                max={1}
                step={0.05}
                value={value}
                onChange={(e) =>
                  setWeights((w) => ({ ...w, [key]: Number(e.target.value) }))
                }
              />
            </label>
          ))}
        </div>
        <p className="text-xs text-muted-foreground mb-3">
          {t('metadata.policyNote')}
        </p>
        <Button
          disabled={save.isPending}
          variant="outline"
          onClick={() => save.mutate()}
        >
          {t('metadata.savePolicy')}
        </Button>
        {save.isSuccess && (
          <span className="text-xs ml-3">{t('metadata.saved')}</span>
        )}
        {save.error && <p role="alert">{save.error.message}</p>}
        </div>
      </Disclosure>
      {mutation.error && <p role="alert">{mutation.error.message}</p>}
      {mutation.data && (
        <p className="text-sm">
          {t('metadata.selected')}:{' '}
          {mutation.data.selectedModelId ?? t('metadata.noCandidate')}
        </p>
      )}
      <ul className="text-xs space-y-2">
        {mutation.data?.decisions.map((d) => (
          <li key={d.modelDbId} className="border-b py-2">
            #{d.modelDbId} ·{' '}
            {t(d.eligible ? 'metadata.eligible' : 'metadata.excluded')} ·{' '}
            {d.score?.toFixed(3) ?? t('metadata.unknown')} ·{' '}
            {d.estimatedCostUsd === null
              ? t('metadata.unknownPrice')
              : `$${d.estimatedCostUsd}`}
            <p className="text-muted-foreground">
              {d.rejectionReasons.join(', ')}
            </p>
          </li>
        ))}
      </ul>
    </section>
  )
}
