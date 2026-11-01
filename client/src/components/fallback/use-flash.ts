import { useCallback, useEffect, useRef, useState } from 'react'

export type FlashTone = 'success' | 'error'

export interface Flash {
  tone: FlashTone
  message: string
}

const FLASH_MS = 5000

/** A status line that clears itself. A newer message replaces the older one. */
export function useFlash() {
  const [flash, setFlash] = useState<Flash | null>(null)
  const timer = useRef<number | null>(null)

  useEffect(() => () => {
    if (timer.current != null) window.clearTimeout(timer.current)
  }, [])

  const notify = useCallback((tone: FlashTone, message: string) => {
    setFlash({ tone, message })
    if (timer.current != null) window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setFlash(null), FLASH_MS)
  }, [])

  return { flash, notify }
}
