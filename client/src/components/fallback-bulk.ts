export type BulkAction = 'enableAll' | 'disableAll' | 'enableFree'

export interface BulkActionResult {
  modelsUpdated?: number
  fallbackEntriesUpdated?: number
  freeEnabled?: number
  paidDisabled?: number
  fallbackFreeEnabled?: number
  fallbackPaidDisabled?: number
}

export function bulkActionPath(action: BulkAction): string {
  if (action === 'enableAll') return '/api/models/enable-all'
  if (action === 'disableAll') return '/api/models/disable-all'
  return '/api/models/enable-free'
}

export function buildBulkResultMessage(
  t: (key: string, opts?: Record<string, unknown>) => string,
  action: BulkAction,
  data: BulkActionResult | undefined,
): string {
  if (!data) return ''
  if (action === 'enableAll' || action === 'disableAll') {
    const count = data.modelsUpdated ?? 0
    return t(`fallback.bulk.result.${action}`, {
      count,
      models: count,
      fallback: data.fallbackEntriesUpdated ?? 0,
    })
  }
  return t('fallback.bulk.result.enableFree', {
    count: 1,
    free: data.freeEnabled ?? 0,
    paid: data.paidDisabled ?? 0,
  })
}
