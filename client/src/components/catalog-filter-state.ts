/** Catalog filter state and the option lists the filter controls share. */

export interface CatalogFilters {
  query: string
  provider: string
  capability: string
  free: boolean
  context: string
  sort: string
}

export const DEFAULT_CATALOG_FILTERS: CatalogFilters = {
  query: '',
  provider: '',
  capability: '',
  free: false,
  context: '',
  sort: 'name',
}

export const CAPABILITY_FILTERS = ['tools', 'vision', 'reasoning', 'structured_output'] as const

export const CONTEXT_PRESETS = [
  { value: '', label: 'Any' },
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
