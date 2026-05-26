import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { PageHeader } from '@/components/page-header'
import { Pencil, Trash2, Plus, X, ExternalLink } from 'lucide-react'

interface CustomProvider {
  id: number
  name: string
  baseUrl: string
  timeoutMs: number
  extraHeaders: Record<string, string> | null
  enabled: boolean
  createdAt: string
  updatedAt: string
}

function ProviderForm({
  initial,
  onSave,
  onCancel,
  saving,
}: {
  initial?: CustomProvider
  onSave: (data: { name: string; baseUrl: string; timeoutMs: number; extraHeaders?: Record<string, string>; enabled?: boolean }) => void
  onCancel: () => void
  saving: boolean
}) {
  const [name, setName] = useState(initial?.name ?? '')
  const [baseUrl, setBaseUrl] = useState(initial?.baseUrl ?? '')
  const [timeoutMs, setTimeoutMs] = useState(String(initial?.timeoutMs ?? 15000))
  const [headerKey, setHeaderKey] = useState('')
  const [headerVal, setHeaderVal] = useState('')
  const [extraHeaders, setExtraHeaders] = useState<Record<string, string>>(initial?.extraHeaders ?? {})
  const [enabled, setEnabled] = useState(initial?.enabled ?? true)

  function addHeader() {
    if (!headerKey.trim()) return
    setExtraHeaders(h => ({ ...h, [headerKey.trim()]: headerVal }))
    setHeaderKey('')
    setHeaderVal('')
  }

  function removeHeader(key: string) {
    setExtraHeaders(h => {
      const n = { ...h }
      delete n[key]
      return n
    })
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!name || !baseUrl) return
    onSave({
      name,
      baseUrl: baseUrl.replace(/\/+$/, ''),
      timeoutMs: parseInt(timeoutMs, 10) || 15000,
      extraHeaders: Object.keys(extraHeaders).length > 0 ? extraHeaders : undefined,
      enabled,
    })
  }

  return (
    <form onSubmit={handleSubmit} className="rounded-lg border bg-card p-4 space-y-3">
      <div className="flex flex-wrap items-end gap-3">
        <div className="space-y-1.5 flex-1 min-w-[180px]">
          <Label className="text-xs">Name</Label>
          <Input value={name} onChange={e => setName(e.target.value)} placeholder="My Local Server" required />
        </div>
        <div className="space-y-1.5 flex-1 min-w-[260px]">
          <Label className="text-xs">Base URL</Label>
          <Input value={baseUrl} onChange={e => setBaseUrl(e.target.value)} placeholder="http://localhost:11434/v1" required className="font-mono text-xs" />
        </div>
        <div className="space-y-1.5 w-[100px]">
          <Label className="text-xs">Timeout (ms)</Label>
          <Input value={timeoutMs} onChange={e => setTimeoutMs(e.target.value)} type="number" min="1000" max="300000" />
        </div>
      </div>

      <div className="space-y-1.5">
        <Label className="text-xs">Extra headers</Label>
        <div className="flex items-center gap-2">
          <Input value={headerKey} onChange={e => setHeaderKey(e.target.value)} placeholder="Header name" className="w-[200px] text-xs" />
          <Input value={headerVal} onChange={e => setHeaderVal(e.target.value)} placeholder="Value" className="text-xs" />
          <Button type="button" size="sm" variant="outline" onClick={addHeader} disabled={!headerKey.trim()}>
            <Plus className="size-3.5" />
          </Button>
        </div>
        {Object.keys(extraHeaders).length > 0 && (
          <div className="flex flex-wrap gap-2 mt-1.5">
            {Object.entries(extraHeaders).map(([k, v]) => (
              <span key={k} className="inline-flex items-center gap-1 text-xs bg-muted rounded-md px-2 py-1">
                <span className="font-mono">{k}: {v}</span>
                <button type="button" onClick={() => removeHeader(k)} className="text-muted-foreground hover:text-foreground">
                  <X className="size-3" />
                </button>
              </span>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <Switch checked={enabled} onCheckedChange={setEnabled} />
        <Label className="text-xs">Enabled</Label>
      </div>

      <div className="flex items-center gap-2 pt-1">
        <Button type="submit" size="sm" disabled={saving || !name || !baseUrl}>
          {saving ? 'Saving…' : initial ? 'Update' : 'Create'}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  )
}

export default function ProvidersPage() {
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState<number | null>(null)
  const [adding, setAdding] = useState(false)

  const { data: providers = [], isLoading } = useQuery<CustomProvider[]>({
    queryKey: ['custom-providers'],
    queryFn: () => apiFetch('/api/providers'),
  })

  const create = useMutation({
    mutationFn: (body: { name: string; baseUrl: string; timeoutMs: number; extraHeaders?: Record<string, string> }) =>
      apiFetch('/api/providers', { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['custom-providers'] })
      setAdding(false)
    },
  })

  const update = useMutation({
    mutationFn: ({ id, ...body }: { id: number; name?: string; baseUrl?: string; timeoutMs?: number; extraHeaders?: Record<string, string> | null; enabled?: boolean }) =>
      apiFetch(`/api/providers/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['custom-providers'] })
      setEditing(null)
    },
  })

  const remove = useMutation({
    mutationFn: (id: number) => apiFetch(`/api/providers/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['custom-providers'] })
      queryClient.invalidateQueries({ queryKey: ['keys'] })
    },
  })

  const toggle = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      apiFetch(`/api/providers/${id}`, { method: 'PATCH', body: JSON.stringify({ enabled }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['custom-providers'] }),
  })

  return (
    <div>
      <PageHeader
        title="Custom Providers"
        description="Add your own OpenAI-compatible servers (localhost, vLLM, llama.cpp, Ollama, etc.)."
        actions={
          !adding && (
            <Button size="sm" onClick={() => setAdding(true)}>
              <Plus className="size-3.5 mr-1" /> Add provider
            </Button>
          )
        }
      />

      <div className="space-y-4">
        {adding && (
          <ProviderForm
            onSave={data => create.mutate(data)}
            onCancel={() => { setAdding(false); create.reset() }}
            saving={create.isPending}
          />
        )}

        {isLoading && <p className="text-xs text-muted-foreground">Loading...</p>}
        {!isLoading && providers.length === 0 && !adding && (
          <p className="text-xs text-muted-foreground">No custom providers yet. Add one to start proxying to your own servers.</p>
        )}

        {providers.map(provider => (
          <div key={provider.id}>
            {editing === provider.id ? (
              <ProviderForm
                initial={provider}
                onSave={data => update.mutate({ id: provider.id, ...data })}
                onCancel={() => setEditing(null)}
                saving={update.isPending}
              />
            ) : (
              <div className="rounded-lg border bg-card p-4 flex items-start justify-between gap-4">
                <div className="space-y-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium">{provider.name}</span>
                    <span className={`inline-block size-1.5 rounded-full ${provider.enabled ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`} />
                    <span className="text-[10px] text-muted-foreground uppercase tracking-wider">
                      {provider.enabled ? 'active' : 'disabled'}
                    </span>
                  </div>
                  <code className="text-xs text-muted-foreground font-mono">{provider.baseUrl}</code>
                  {provider.extraHeaders && Object.keys(provider.extraHeaders).length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-1">
                      {Object.entries(provider.extraHeaders).map(([k, v]) => (
                        <span key={k} className="text-[10px] bg-muted rounded px-1.5 py-0.5 font-mono">{k}: {v}</span>
                      ))}
                    </div>
                  )}
                  <div className="flex items-center gap-3 mt-1.5">
                    <Link
                      to={`/providers/${provider.id}/models`}
                      className="text-xs text-primary hover:underline inline-flex items-center gap-1"
                    >
                      Models <ExternalLink className="size-3" />
                    </Link>
                    <button onClick={() => setEditing(provider.id)} className="text-xs text-muted-foreground hover:text-foreground inline-flex items-center gap-1">
                      <Pencil className="size-3" /> Edit
                    </button>
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <Switch
                    checked={provider.enabled}
                    onCheckedChange={v => toggle.mutate({ id: provider.id, enabled: v })}
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7 text-rose-600 hover:text-rose-700"
                    onClick={() => { if (confirm('Delete this provider and all its models/keys?')) remove.mutate(provider.id) }}
                    disabled={remove.isPending}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
