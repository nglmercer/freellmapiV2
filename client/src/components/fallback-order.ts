import { arrayMove } from '@dnd-kit/sortable'
import type { UniqueIdentifier } from '@dnd-kit/core'
import type { FallbackEntry } from './fallback-filters'

/**
 * Applies a drag between two visible rows to the full chain. Hidden entries
 * keep their slots: the visible rows are reshuffled among the positions they
 * already occupy, then every entry is renumbered so priorities stay contiguous.
 * Returns null when the drop is a no-op.
 */
export function reorderFallbackEntries(
  allEntries: FallbackEntry[],
  visibleEntries: FallbackEntry[],
  activeId: UniqueIdentifier,
  overId: UniqueIdentifier,
): FallbackEntry[] | null {
  if (activeId === overId) return null
  const oldIndex = visibleEntries.findIndex((e) => e.modelDbId === activeId)
  const newIndex = visibleEntries.findIndex((e) => e.modelDbId === overId)
  if (oldIndex < 0 || newIndex < 0) return null

  const reorderedVisible = arrayMove(visibleEntries, oldIndex, newIndex)
  let visibleIndex = 0
  const visibleIds = new Set(visibleEntries.map((e) => e.modelDbId))
  return allEntries
    .map((e) => {
      if (!visibleIds.has(e.modelDbId)) return e
      return reorderedVisible[visibleIndex++]
    })
    .map((e, i) => ({ ...e, priority: i + 1 }))
}
