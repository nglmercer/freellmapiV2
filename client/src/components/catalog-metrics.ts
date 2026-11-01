import type { MetadataDeployment, MetadataMetric } from '@freellmapi/shared'

/** Quality metrics in preference order: reviewed manual score, then Artificial Analysis. */
export const QUALITY_METRICS = ['benchmark.manual.intelligence', 'benchmark.aa.intelligence'] as const
export const SPEED_METRIC = 'performance.output_tps'
export const TTFT_METRIC = 'performance.ttft_ms'
export const CONTEXT_METRIC = 'context.total'
export const PRICE_INPUT_METRIC = 'price.input'
export const PRICE_OUTPUT_METRIC = 'price.output'

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

export function capabilityList(metrics: MetadataMetric[]): string[] {
  return metrics
    .filter((m) => m.metric.startsWith('capability.') && m.value.value === true)
    .map((m) => m.metric.slice('capability.'.length))
    .sort()
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
    capabilities: capabilityList(metrics),
  }
}

export function compactTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${Math.round(value / 1_000)}K`
  return String(value)
}

export function usd(value: number | null): string | null {
  if (value == null) return null
  return `$${value.toFixed(value < 10 ? 2 : 1)}`
}
