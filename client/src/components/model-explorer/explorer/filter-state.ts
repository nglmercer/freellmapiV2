/** Explorer filter state and the option lists its controls share. */

export const PAGE_SIZE = 25
export const MAX_COMPARE = 4

export interface CatalogFilters {
  query: string
  provider: string
  capability: string
  free: boolean
  context: string
  sort: string
}

export const DEFAULT_FILTERS: CatalogFilters = {
  query: '',
  provider: '',
  capability: '',
  free: false,
  context: '',
  sort: 'name',
}

/** Capabilities the list can filter on; the backend accepts a single one. */
export const CAPABILITY_FILTERS = ['tools', 'vision', 'reasoning', 'structured_output'] as const

/** `value` is the minimum context in tokens; empty means no minimum. */
export const CONTEXT_PRESETS = [
  { value: '', label: '' },
  { value: '8000', label: '8K' },
  { value: '32000', label: '32K' },
  { value: '128000', label: '128K' },
  { value: '1000000', label: '1M' },
] as const

export const SORT_OPTIONS = [
  'name',
  'quality',
  'coding',
  'speed',
  'ttft',
  'price',
  'reliability',
  'context',
  'recency',
] as const

export function countActiveFilters(filters: CatalogFilters): number {
  return [
    filters.query !== '',
    filters.provider !== '',
    filters.capability !== '',
    filters.free,
    filters.context !== '',
  ].filter(Boolean).length
}
