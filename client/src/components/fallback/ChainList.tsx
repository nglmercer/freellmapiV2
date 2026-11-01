import { useMemo } from 'react'
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type UniqueIdentifier,
} from '@dnd-kit/core'
import { SortableContext, sortableKeyboardCoordinates, verticalListSortingStrategy } from '@dnd-kit/sortable'
import type { FallbackEntry } from '@freellmapi/shared'
import { ChainRow } from './ChainRow'

/** The visible part of the chain, reorderable by drag or keyboard. */
export function ChainList({
  entries,
  maxSpeed,
  onReorder,
  onToggle,
}: {
  entries: FallbackEntry[]
  maxSpeed: number
  onReorder: (activeId: UniqueIdentifier, overId: UniqueIdentifier) => void
  onToggle: (modelDbId: number, enabled: boolean) => void
}) {
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )
  const ids = useMemo(() => entries.map((e) => e.modelDbId), [entries])

  function handleDragEnd(event: DragEndEvent) {
    if (event.over) onReorder(event.active.id, event.over.id)
  }

  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <SortableContext items={ids} strategy={verticalListSortingStrategy}>
        <ol className="divide-y overflow-hidden rounded-xl border bg-card">
          {entries.map((entry, index) => (
            <ChainRow
              key={entry.modelDbId}
              entry={entry}
              rank={index + 1}
              maxSpeed={maxSpeed}
              onToggle={onToggle}
            />
          ))}
        </ol>
      </SortableContext>
    </DndContext>
  )
}
