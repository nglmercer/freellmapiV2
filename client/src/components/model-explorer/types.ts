import type { MetadataMetric, MetadataModel } from '@freellmapi/shared'

export type Page = { data: MetadataModel[]; total: number }
export type Match = {
  id: number
  source: string
  sourceModelId: string
  modelId: string
  canonicalHint: string | null
}
export type Decision = {
  modelDbId: number
  eligible: boolean
  rejectionReasons: string[]
  score: number | null
  estimatedCostUsd: number | null
}
export type HistoryPoint = {
  metric: string
  source: string
  changedAt: string
  benchmarkVersion: string
  value: MetadataMetric['value']
  unit: string
}
