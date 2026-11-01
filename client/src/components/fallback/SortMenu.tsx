import { useTranslations } from '@/hooks/useTranslations'
import { ActionMenu, ActionMenuItem } from './ActionMenu'
import type { SortPreset } from './chain-model'

const PRESETS: SortPreset[] = ['manual', 'balanced', 'quality', 'fastest', 'reliability', 'budget']

/** Reorders the whole chain by a preset on the server. */
export function SortMenu({ disabled, onSelect }: { disabled: boolean; onSelect: (preset: SortPreset) => void }) {
  const { t } = useTranslations()
  return (
    <ActionMenu label={t('fallback.bulk.sort.label')} disabled={disabled}>
      {PRESETS.map((preset) => (
        <ActionMenuItem key={preset} onSelect={() => onSelect(preset)}>
          {t(`fallback.bulk.sort.${preset}`)}
        </ActionMenuItem>
      ))}
    </ActionMenu>
  )
}
