import type { Tone } from '@/components/status-badge'

export interface SourceStatusView {
  /** Translation key for the short status label. */
  labelKey: string
  tone: Tone
}

/** Maps a metadata source status code to a label and tone. */
export function sourceStatusView(status: string): SourceStatusView {
  switch (status) {
    case 'fetched':
    case 'notModified':
      return { labelKey: 'sources.status.synced', tone: 'success' }
    case 'never':
      return { labelKey: 'sources.status.notSynced', tone: 'muted' }
    case 'notConfigured':
      return { labelKey: 'sources.status.needsKey', tone: 'warning' }
    case 'disabled':
      return { labelKey: 'sources.status.disabled', tone: 'muted' }
    case 'backoff':
      return { labelKey: 'sources.status.waiting', tone: 'info' }
    case 'persistenceFailed':
      return { labelKey: 'sources.status.saveFailed', tone: 'danger' }
    case 'duplicateIdentity':
      return { labelKey: 'sources.status.duplicates', tone: 'warning' }
    default:
      return { labelKey: 'sources.status.failed', tone: 'danger' }
  }
}

/** Known error codes get a sentence; anything else is shown as the database or
 * network message it came from. */
export function sourceErrorKey(error: string): string | null {
  switch (error) {
    case 'notConfigured':
      return 'sources.error.notConfigured'
    case 'persistenceFailed':
      return 'sources.error.persistenceFailed'
    case 'duplicateIdentity':
      return 'sources.error.duplicates'
    case 'backoff':
      return 'sources.error.backoff'
    default:
      return null
  }
}

/** "24 h", "7 d", "45 min" for a refresh interval in seconds. */
export function formatInterval(seconds: number): string {
  if (seconds >= 86400) return `${Math.round(seconds / 86400)} d`
  if (seconds >= 3600) return `${Math.round(seconds / 3600)} h`
  return `${Math.round(seconds / 60)} min`
}

/** Tone for an API key's health status. */
export function keyStatusTone(status: string): Tone {
  switch (status) {
    case 'healthy':
      return 'success'
    case 'rate_limited':
      return 'warning'
    case 'invalid':
    case 'error':
      return 'danger'
    default:
      return 'muted'
  }
}

