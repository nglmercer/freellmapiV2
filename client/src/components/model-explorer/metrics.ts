import type { MetadataDeployment, MetadataMetric, MetadataModel } from '@freellmapi/shared'

/** Quality metrics in preference order: reviewed manual score, then Artificial Analysis. */
export const QUALITY_METRICS = ['benchmark.manual.intelligence', 'benchmark.aa.intelligence'] as const
export const SPEED_METRIC = 'performance.output_tps'
export const TTFT_METRIC = 'performance.ttft_ms'
export const CONTEXT_METRIC = 'context.total'
export const PRICE_INPUT_METRIC = 'price.input'
export const PRICE_OUTPUT_METRIC = 'price.output'

/** Every capability the backend can report; the detail view shows all of them. */
export const CAPABILITIES = [
  'tools',
  'vision',
  'reasoning',
  'structured_output',
  'audio_input',
  'video_input',
] as const
export type Capability = (typeof CAPABILITIES)[number]

export function numberMetric(metrics: MetadataMetric[], name: string): number | null {
  const found = metrics.find((m) => m.metric === name && m.value.type === 'number')
  return typeof found?.value.value === 'number' ? found.value.value : null
}

export function qualityMetric(metrics: MetadataMetric[]): number | null {
  for (const name of QUALITY_METRICS) {
    const value = numberMetric(metrics, name)
    if (value != null) return value
  }
  return null
}

/** Capabilities the deployment has explicitly reported as supported. */
export function supportedCapabilities(metrics: MetadataMetric[]): string[] {
  return metrics
    .filter((m) => m.metric.startsWith('capability.') && m.value.value === true)
    .map((m) => m.metric.slice('capability.'.length))
    .sort()
}

/**
 * Tri-state capability evidence for one deployment: `true`/`false` when a
 * provider-scoped observation exists, `null` when nothing is known.
 */
export function capabilityEvidence(metrics: MetadataMetric[], capability: Capability): boolean | null {
  const evidence = metrics.find(
    (m) => m.metric === `capability.${capability}` && m.scope === 'providerDeployment',
  )
  if (!evidence) return null
  return evidence.value.value === true
}

/** The deployment's summary numbers, read once from its metrics array. */
export interface DeploymentSummary {
  quality: number | null
  speed: number | null
  ttft: number | null
  context: number | null
  priceInput: number | null
  priceOutput: number | null
  capabilities: string[]
}

export function summarize(deployment: MetadataDeployment): DeploymentSummary {
  const metrics = deployment.metrics ?? []
  return {
    quality: qualityMetric(metrics),
    speed: numberMetric(metrics, SPEED_METRIC),
    ttft: numberMetric(metrics, TTFT_METRIC),
    context: numberMetric(metrics, CONTEXT_METRIC),
    priceInput: numberMetric(metrics, PRICE_INPUT_METRIC),
    priceOutput: numberMetric(metrics, PRICE_OUTPUT_METRIC),
    capabilities: supportedCapabilities(metrics),
  }
}

/** Best value per metric across a model's deployments. Unknown values never win. */
export interface ModelSummary {
  quality: number | null
  speed: number | null
  context: number | null
  priceInput: number | null
  priceOutput: number | null
  providers: string[]
  freeDeployments: number
}

function pick(values: (number | null)[], direction: 'max' | 'min'): number | null {
  const present = values.filter((v): v is number => v != null)
  if (present.length === 0) return null
  return direction === 'max' ? Math.max(...present) : Math.min(...present)
}

export function summarizeModel(model: MetadataModel): ModelSummary {
  const summaries = model.deployments.map(summarize)
  return {
    quality: pick(summaries.map((s) => s.quality), 'max'),
    speed: pick(summaries.map((s) => s.speed), 'max'),
    context: pick(summaries.map((s) => s.context), 'max'),
    priceInput: pick(summaries.map((s) => s.priceInput), 'min'),
    priceOutput: pick(summaries.map((s) => s.priceOutput), 'min'),
    providers: Array.from(new Set(model.deployments.map((d) => d.provider))).sort(),
    freeDeployments: model.deployments.filter((d) => d.freeTier).length,
  }
}
