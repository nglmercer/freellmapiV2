import { useState } from 'react'
import {
  AlertTriangle,
  CalendarClock,
  CircleCheck,
  CircleDashed,
  Copy,
  KeyRound,
  Pause,
  Timer,
  Clock,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { MetadataSource } from '@freellmapi/shared'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Dialog, DialogDescription, DialogFooter, DialogHeader, DialogPopup, DialogTitle } from '@/components/ui/dialog'
import { StatusIcon, type Tone } from '@/components/status-badge'
import { IconButton } from '@/components/icon-button'
import { Tooltip } from '@/components/tooltip'
import { formatDate } from '@/components/fallback-format'
import { formatInterval, sourceErrorKey, sourceStatusView } from '@/lib/source-status'

const MIN_TTL = 300
const MAX_TTL = 604800

const STATUS_ICON: Record<string, LucideIcon> = {
  fetched: CircleCheck,
  notModified: CircleCheck,
  never: CircleDashed,
  notConfigured: KeyRound,
  disabled: Pause,
  backoff: Clock,
  persistenceFailed: AlertTriangle,
  duplicateIdentity: Copy,
}

/** Icon for a source status code; anything unknown is treated as a failure. */
function statusIcon(status: string): LucideIcon {
  return STATUS_ICON[status] ?? AlertTriangle
}

/** One fact as an icon with its value. The label is the hover title. */
function Fact({ icon: Icon, label, value, tone }: { icon: LucideIcon; label: string; value: string; tone?: Tone }) {
  return (
    <Tooltip label={label}>
      <span
        className={`inline-flex items-center gap-1.5 text-xs tabular-nums ${
          tone === 'warning' ? 'text-amber-600 dark:text-amber-400' : 'text-muted-foreground'
        }`}
        aria-label={`${label}: ${value}`}
      >
        <Icon className="size-3.5 shrink-0" aria-hidden />
        <span>{value}</span>
      </span>
    </Tooltip>
  )
}

/** Dialog to change how often a source is fetched. */
function IntervalDialog({
  open,
  onOpenChange,
  seconds,
  onSave,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  seconds: number
  onSave: (seconds: number) => void
}) {
  const { t } = useTranslations()
  const [value, setValue] = useState(String(seconds))
  const numeric = Number(value)
  const valid = Number.isInteger(numeric) && numeric >= MIN_TTL && numeric <= MAX_TTL
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPopup className="w-[min(92vw,22rem)] max-w-none">
        <DialogHeader>
          <DialogTitle>{t('sources.intervalTitle')}</DialogTitle>
          <DialogDescription>{t('sources.intervalRange', { min: MIN_TTL, max: MAX_TTL })}</DialogDescription>
        </DialogHeader>
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            if (valid && numeric !== seconds) onSave(numeric)
            onOpenChange(false)
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor="source-interval" className="text-xs text-muted-foreground">
              {t('sources.intervalSeconds')}
            </Label>
            <Input
              id="source-interval"
              type="number"
              inputMode="numeric"
              min={MIN_TTL}
              max={MAX_TTL}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              aria-invalid={!valid}
              className="font-mono tabular-nums"
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t('keys.cancel')}
            </Button>
            <Button type="submit" disabled={!valid}>
              {t('sources.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogPopup>
    </Dialog>
  )
}

/** One metadata source: a status icon, a switch, and icon facts. Details live
 * in tooltips and dialogs, so cards keep the same height. */
export function SourceCard({
  source,
  busy,
  onToggle,
  onInterval,
  onAddKey,
}: {
  source: MetadataSource
  busy: boolean
  onToggle: (enabled: boolean) => void
  onInterval: (seconds: number) => void
  onAddKey?: () => void
}) {
  const { t } = useTranslations()
  const [intervalOpen, setIntervalOpen] = useState(false)
  const view = sourceStatusView(source.status)
  const errorKey = source.error ? sourceErrorKey(source.error) : null
  const errorText = source.error ? (errorKey ? t(errorKey) : source.error) : null

  return (
    <article className="flex flex-col gap-3 rounded-lg border bg-card p-4">
      <div className="flex items-center gap-3">
        <StatusIcon tone={view.tone} icon={statusIcon(source.status)} label={t(view.labelKey)} />
        <Tooltip label={[source.license, source.domains.join(', ')].filter(Boolean).join(' · ')}>
          <h3 className="min-w-0 truncate text-sm font-medium">{source.attribution}</h3>
        </Tooltip>
        <div className="ml-auto flex items-center gap-2">
          <Switch
            checked={source.enabled}
            disabled={busy}
            onCheckedChange={onToggle}
            aria-label={`${t('sources.enable')}: ${source.attribution}`}
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-muted-foreground">
        <Fact
          icon={CalendarClock}
          label={t('sources.lastSuccess')}
          value={formatDate(source.lastSuccess) ?? t('metadata.unknown')}
        />
        <Fact icon={Timer} label={t('sources.refreshEvery')} value={formatInterval(source.ttlSeconds)} />
        <Tooltip label={source.configured ? t('sources.keyConfigured') : t('sources.keyMissing')}>
          <span
            role="img"
            aria-label={source.configured ? t('sources.keyConfigured') : t('sources.keyMissing')}
            className={`inline-flex items-center ${source.configured ? 'text-emerald-600 dark:text-emerald-400' : 'text-amber-600 dark:text-amber-400'}`}
          >
            <KeyRound className="size-3.5" aria-hidden />
          </span>
        </Tooltip>
        {errorText && (
          <Tooltip label={errorText}>
            <span role="img" aria-label={errorText} className="inline-flex items-center text-red-600 dark:text-red-400">
              <AlertTriangle className="size-3.5" aria-hidden />
            </span>
          </Tooltip>
        )}
      </div>

      <div className="mt-auto flex items-center justify-end gap-1 border-t pt-3">
        {!source.configured && onAddKey && (
          <IconButton label={t('metadata.addBenchmarkKey')} icon={KeyRound} variant="outline" onClick={onAddKey} />
        )}
        <IconButton label={t('sources.intervalSettings')} icon={Clock} onClick={() => setIntervalOpen(true)} />
      </div>

      {intervalOpen && (
        <IntervalDialog
          open
          onOpenChange={setIntervalOpen}
          seconds={source.ttlSeconds}
          onSave={onInterval}
        />
      )}
    </article>
  )
}
