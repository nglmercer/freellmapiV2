import { useEffect, useState } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import type { Page } from '../types'
import { PAGE_SIZE, type CatalogFilters } from './filter-state'

const SEARCH_DEBOUNCE_MS = 250

function useDebounced<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delay)
    return () => window.clearTimeout(id)
  }, [value, delay])
  return debounced
}

/**
 * Fetches one page of the explorer. Typing is debounced so each keystroke does
 * not hit the API, and the previous page stays on screen until the next arrives.
 */
export function useExplorerQuery(filters: CatalogFilters, offset: number) {
  const query = useDebounced(filters.query.trim(), SEARCH_DEBOUNCE_MS)
  const provider = useDebounced(filters.provider.trim(), SEARCH_DEBOUNCE_MS)

  const params = new URLSearchParams({
    query,
    provider,
    capability: filters.capability,
    sort: filters.sort,
    limit: String(PAGE_SIZE),
    offset: String(offset),
    ...(filters.free ? { free: 'true' } : {}),
    ...(filters.context ? { minContext: filters.context } : {}),
  })

  return useQuery<Page>({
    queryKey: ['metadata-explorer', params.toString()],
    queryFn: () => apiFetch(`/api/metadata/explorer?${params}`),
    placeholderData: keepPreviousData,
  })
}
