import { useCallback, useState } from 'react'
import { DEFAULT_FILTERS, MAX_COMPARE, type CatalogFilters } from './filter-state'

/** A deployment selected for comparison, with the label the table header shows. */
export interface CompareItem {
  id: number
  label: string
}

export interface ExplorerState {
  filters: CatalogFilters
  offset: number
  selection: CompareItem[]
  canSelectMore: boolean
  setFilters: (patch: Partial<CatalogFilters>) => void
  resetFilters: () => void
  setOffset: (offset: number) => void
  toggleCompare: (item: CompareItem, checked: boolean) => void
  removeCompare: (id: number) => void
  clearCompare: () => void
}

/**
 * Explorer state lives in the page, not the tab, so filters, the current page
 * and the comparison survive switching to another model-intelligence tab.
 */
export function useExplorerState(): ExplorerState {
  const [filters, setFiltersState] = useState<CatalogFilters>(DEFAULT_FILTERS)
  const [offset, setOffset] = useState(0)
  const [selection, setSelection] = useState<CompareItem[]>([])

  const setFilters = useCallback((patch: Partial<CatalogFilters>) => {
    setFiltersState((current) => ({ ...current, ...patch }))
    setOffset(0)
  }, [])

  const resetFilters = useCallback(() => {
    setFiltersState(DEFAULT_FILTERS)
    setOffset(0)
  }, [])

  const toggleCompare = useCallback((item: CompareItem, checked: boolean) => {
    setSelection((current) => {
      if (!checked) return current.filter((s) => s.id !== item.id)
      if (current.some((s) => s.id === item.id) || current.length >= MAX_COMPARE) return current
      return [...current, item]
    })
  }, [])

  const removeCompare = useCallback((id: number) => {
    setSelection((current) => current.filter((s) => s.id !== id))
  }, [])

  const clearCompare = useCallback(() => setSelection([]), [])

  return {
    filters,
    offset,
    selection,
    canSelectMore: selection.length < MAX_COMPARE,
    setFilters,
    resetFilters,
    setOffset,
    toggleCompare,
    removeCompare,
    clearCompare,
  }
}
