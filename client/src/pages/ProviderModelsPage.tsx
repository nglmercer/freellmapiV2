import { useState } from 'react'
import { useParams, Link } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { PageHeader } from '@/components/page-header'
import { Pencil, Trash2, Plus, ChevronLeft } from 'lucide-react'

interface CustomModel {
  id: number
  modelId: string
  displayName: string
  intelligenceRank: number
  speedRank: number
  sizeLabel: string
  rpmLimit: number | null
  rpdLimit: number | null
  tpmLimit: number | null
  tpdLimit: number | null
  monthlyTokenBudget: string
  contextWindow: number | null
  enabled: boolean
}

interface CustomProvider {
  id: number
  name: string
}

function ModelForm({
  initial,
  onSave,
  onCancel,
  saving,
}: {
  initial?: CustomModel
  onSave: (data: any) => void
  onCancel: () => void
  saving: boolean
}) {
  const [modelId, setModelId] = useState(initial?.modelId ?? '')
  const [displayName, setDisplayName] = useState(initial?.displayName ?? '')
  const [contextWindow, setContextWindow] = useState(initial?.contextWindow ? String(initial.contextWindow) : '')
  const [intelligenceRank, setIntelligenceRank] = useState(String(initial?.intelligenceRank ?? 99))
  const [speedRank, setSpeedRank] = useState(String(initial?.speedRank ?? 10))
  const [enabled, setEnabled] = useState(initial?.enabled ?? true)

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!modelId || !displayName) return
    onSave({
      modelId,
      displayName,
      intelligenceRank: parseInt(intelligenceRank, 10) || 99,
      speedRank: parseInt(speedRank, 10) || 10,
      sizeLabel: '',
      rpmLimit: null,
      rpdLimit: null,
      tpmLimit: null,
      tpdLimit: null,
      monthlyTokenBudget: '',
      contextWindow: contextWindow ? parseInt(contextWindow, 10) : null,
      enabled,
    })
  }

  return (
    <form onSubmit={handleSubmit} className="rounded-lg border bg-card p-4 space-y-3">
      <div className="flex flex-wrap items-end gap-3">
        <div className="space-y-1.5 flex-1 min-w-[180px]">
          <Label className="text-xs">Model ID <span className="text-muted-foreground">(as sent to API)</span></Label>
          <Input value={modelId} onChange={e => setModelId(e.target.value)} placeholder="llama3.2" required className="font-mono text-xs" disabled={!!initial} />
        </div>
        <div className="space-y-1.5 flex-1 min-w-[180px]">
          <Label className="text-xs">Display name</Label>
          <Input value={displayName} onChange={e => setDisplayName(e.target.value)} placeholder="Llama 3.2 3B" required />
        </div>
        <div className="space-y-1.5 w-[120px]">
          <Label className="text-xs">Context window</Label>
          <Input value={contextWindow} onChange={e => setContextWindow(e.target.value)} placeholder="8192" type="number" />
        </div>
        <div className="space-y-1.5 w-[80px]">
          <Label className="text-xs">Intel. rank</Label>
          <Input value={intelligenceRank} onChange={e => setIntelligenceRank(e.target.value)} type="number" min="1" max="999" />
        </div>
        <div className="space-y-1.5 w-[80px]">
          <Label className="text-xs">Speed rank</Label>
          <Input value={speedRank} onChange={e => setSpeedRank(e.target.value)} type="number" min="1" max="999" />
        </div>
      </div>

      <div className="flex items-center gap-2">
        <Switch checked={enabled} onCheckedChange={setEnabled} />
        <Label className="text-xs">Enabled (appears in fallback chain)</Label>
      </div>

      <div className="flex items-center gap-2 pt-1">
        <Button type="submit" size="sm" disabled={saving || !modelId || !displayName}>
          {saving ? 'Saving…' : initial ? 'Update' : 'Add model'}
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>Cancel</Button>
      </div>
    </form>
  )
}

export default function ProviderModelsPage() {
  const { id } = useParams<{ id: string }>()
  const providerId = parseInt(id ?? '', 10)
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)

  const { data: providers = [] } = useQuery<CustomProvider[]>({
    queryKey: ['custom-providers'],
    queryFn: () => apiFetch('/api/providers'),
  })
  const provider = providers.find(p => p.id === providerId)

  const { data: models = [], isLoading } = useQuery<CustomModel[]>({
    queryKey: ['custom-providers', providerId, 'models'],
    queryFn: () => apiFetch(`/api/providers/${providerId}/models`),
    enabled: !isNaN(providerId),
  })

  const create = useMutation({
    mutationFn: (body: any) => apiFetch(`/api/providers/${providerId}/models`, { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['custom-providers', providerId, 'models'] })
      setAdding(false)
    },
  })

  const update = useMutation({
    mutationFn: ({ modelId, ...body }: any) =>
      apiFetch(`/api/providers/${providerId}/models/${encodeURIComponent(modelId)}`, { method: 'PATCH', body: JSON.stringify(body) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['custom-providers', providerId, 'models'] })
      setEditing(null)
    },
  })

  const remove = useMutation({
    mutationFn: (modelId: string) =>
      apiFetch(`/api/providers/${providerId}/models/${encodeURIComponent(modelId)}`, { method: 'DELETE' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['custom-providers', providerId, 'models'] }),
  })

  const toggle = useMutation({
    mutationFn: ({ modelId, enabled }: { modelId: string; enabled: boolean }) =>
      apiFetch(`/api/providers/${providerId}/models/${encodeURIComponent(modelId)}`, { method: 'PATCH', body: JSON.stringify({ enabled }) }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['custom-providers', providerId, 'models'] }),
  })

  if (!provider) {
    return (
      <div>
        <Link to="/providers" className="text-xs text-muted-foreground hover:text-foreground inline-flex items-center gap-1 mb-4">
          <ChevronLeft className="size-3" /> Back to providers
        </Link>
        <p className="text-xs text-muted-foreground">Provider not found.</p>
      </div>
    )
  }

  return (
    <div>
      <PageHeader
        title={
          <span className="inline-flex items-center gap-2">
            <Link to="/providers" className="text-muted-foreground hover:text-foreground -ml-1">
              <ChevronLeft className="size-4" />
            </Link>
            {provider.name} — Models
          </span>
        }
        description="Manage custom models for this provider. They appear in the fallback chain proxied through your server."
        actions={
          !adding && (
            <Button size="sm" onClick={() => setAdding(true)}>
              <Plus className="size-3.5 mr-1" /> Add model
            </Button>
          )
        }
      />

<div className="space-y-4">
         {adding && (
           <ModelForm
             onSave={data => create.mutate(data)}
             onCancel={() => { setAdding(false); create.reset() }}
             saving={create.isPending}
           />
         )}

         {isLoading && <p className="text-xs text-muted-foreground">Loading...</p>}
         {!isLoading && models.length === 0 && !adding && (
           <p className="text-xs text-muted-foreground">No models for this provider yet.</p>
         )}

         {models.map(m => (
           <div key={m.modelId}>
             {editing === m.modelId ? (
               <ModelForm
                 initial={m}
                 onSave={data => update.mutate({ modelId: m.modelId, ...data })}
                 onCancel={() => setEditing(null)}
                 saving={update.isPending}
               />
             ) : (
               <div className="rounded-lg border bg-card p-4 flex items-start justify-between gap-4">
                 <div className="space-y-1 min-w-0">
                   <div className="flex items-center gap-2">
                     <span className="text-sm font-medium">{m.displayName}</span>
                     <span className={`inline-block size-1.5 rounded-full ${m.enabled ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`} />
                     <span className="text-[10px] text-muted-foreground uppercase tracking-wider">
                       {m.enabled ? 'active' : 'disabled'}
                     </span>
                   </div>
                   <code className="text-xs text-muted-foreground font-mono">{m.modelId}</code>
                   <div className="flex items-center gap-3 text-[10px] text-muted-foreground mt-0.5">
                     {m.contextWindow && <span>{m.contextWindow.toLocaleString()} tokens</span>}
                     <span>Intel #{m.intelligenceRank}</span>
                     <span>Speed #{m.speedRank}</span>
                   </div>
                 </div>
                 <div className="flex items-center gap-2 shrink-0">
                   <Switch
                     checked={m.enabled}
                     onCheckedChange={v => toggle.mutate({ modelId: m.modelId, enabled: v })}
                   />
                   <Button variant="ghost" size="icon" className="size-7" onClick={() => setEditing(m.modelId)}>
                     <Pencil className="size-3.5" />
                   </Button>
                   <Button variant="ghost" size="icon" className="size-7 text-rose-600 hover:text-rose-700"
                     onClick={() => { if (confirm(`Delete model "${m.displayName}"?`)) remove.mutate(m.modelId) }}
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
