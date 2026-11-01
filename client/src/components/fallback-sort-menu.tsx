import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface FallbackSortMenuProps {
  disabled: boolean
  onSelect: (preset: 'manual' | 'balanced' | 'quality' | 'fastest' | 'reliability' | 'budget') => void
}

export function FallbackSortMenu({ disabled, onSelect }: FallbackSortMenuProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)

  return (
    <div className="relative">
      <Button
        variant="outline"
        size="sm"
        onClick={() => setOpen((o) => !o)}
        disabled={disabled}
        className="gap-1"
      >
        {t('fallback.bulk.sort.label')}
        <ChevronDown className="size-3.5" />
      </Button>
      {open && (
        <>
          <div
            className="fixed inset-0 z-10"
            onClick={() => setOpen(false)}
          />
          <div className="absolute right-0 mt-1 z-20 min-w-44 rounded-md border bg-popover shadow-md p-1 text-sm">
            {(['manual', 'balanced', 'quality', 'fastest', 'reliability', 'budget'] as const).map((preset) => (
              <button
                key={preset}
                className="w-full text-left px-2 py-1.5 rounded hover:bg-accent hover:text-accent-foreground flex items-center justify-between"
                onClick={() => {
                  setOpen(false)
                  onSelect(preset)
                }}
              >
                {t(`fallback.bulk.sort.${preset}`)}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  )
}
