import { useState, type ReactNode } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { IconButton } from '../icon-button'

export function Field({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return (
    <div className="space-y-1.5">
      <Label htmlFor={htmlFor} className="text-xs text-muted-foreground">
        {label}
      </Label>
      {children}
    </div>
  )
}

/** A key input with an eye toggle inside the field. */
export function SecretInput({
  id,
  value,
  onChange,
  placeholder,
  revealLabel,
  hideLabel,
}: {
  id: string
  value: string
  onChange: (value: string) => void
  placeholder: string
  revealLabel: string
  hideLabel: string
}) {
  const [reveal, setReveal] = useState(false)
  return (
    <div className="relative">
      <Input
        id={id}
        type={reveal ? 'text' : 'password'}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        className="pr-10 font-mono text-xs"
      />
      <div className="absolute right-1 top-1/2 -translate-y-1/2">
        <IconButton
          label={reveal ? hideLabel : revealLabel}
          icon={reveal ? EyeOff : Eye}
          onClick={() => setReveal((value) => !value)}
          className="size-7"
        />
      </div>
    </div>
  )
}
