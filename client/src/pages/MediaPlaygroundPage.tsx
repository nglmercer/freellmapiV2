import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { apiFetch, ApiError } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { PageHeader } from '@/components/page-header'
import { useTranslations } from '@/hooks/useTranslations'

const BASE = import.meta.env.BASE_URL.replace(/\/$/, '')

type Tab = 'speech' | 'transcription' | 'image' | 'embeddings' | 'video' | 'declare'

const TABS: Tab[] = ['speech', 'transcription', 'image', 'embeddings', 'video', 'declare']

/** Declared models for one operation, from `GET /v1/models?capability=`. */
interface ListedModel {
  id: string
  owned_by: string
}

const SPEECH_FORMATS = ['mp3', 'wav', 'opus', 'flac'] as const

const TERMINAL_VIDEO_STATES = ['completed', 'failed', 'cancelled', 'deleted', 'expired']

/** Sends a multipart or binary request; the session cookie authorizes it. */
async function rawRequest(path: string, init: RequestInit): Promise<Response> {
  const res = await fetch(`${BASE}${path}`, { ...init, credentials: 'same-origin' })
  if (!res.ok) {
    const body = await res.json().catch(() => null)
    const message =
      (body?.error?.message as string | undefined) ?? (res.statusText || `HTTP ${res.status}`)
    throw new ApiError(message, res.status)
  }
  return res
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

/** Models declared for `capability`, sorted as the server returns them. */
function useDeclaredModels(capability: string) {
  return useQuery<ListedModel[]>({
    queryKey: ['media-models', capability],
    queryFn: async () => {
      const body = await apiFetch<{ data: ListedModel[] }>(
        `/v1/models?capability=${encodeURIComponent(capability)}`,
      )
      return body.data
    },
  })
}

function ModelSelect({
  models,
  value,
  onChange,
  disabled,
}: {
  models: ListedModel[]
  value: string
  onChange: (value: string) => void
  disabled?: boolean
}) {
  return (
    <select
      value={value}
      onChange={e => onChange(e.target.value)}
      disabled={disabled}
      className="rounded-md border bg-background px-3 py-2 text-sm font-mono disabled:opacity-50"
    >
      {models.map(m => (
        <option key={`${m.owned_by}/${m.id}`} value={m.id}>
          {m.id} ({m.owned_by})
        </option>
      ))}
    </select>
  )
}

/** Shared frame: a title, the declared-model gate, and the panel body. */
function Panel({
  title,
  capability,
  children,
}: {
  title: string
  capability: string
  children: (models: ListedModel[], selected: string, setSelected: (v: string) => void) => ReactNode
}) {
  const { t } = useTranslations()
  const { data: models = [], isLoading, error } = useDeclaredModels(capability)
  const [selected, setSelected] = useState('')
  const effective = models.some(m => m.id === selected) ? selected : (models[0]?.id ?? '')

  return (
    <section className="rounded-lg border bg-card p-4 flex flex-col gap-4">
      <h2 className="text-base font-semibold">{title}</h2>
      {isLoading ? (
        <p className="text-sm text-muted-foreground">{t('common.loading')}</p>
      ) : error ? (
        <p className="text-sm text-destructive">{errorText(error)}</p>
      ) : models.length === 0 ? (
        // Unsupported or undeclared: explain, do not show working-looking controls.
        <p className="text-sm text-muted-foreground" data-testid="media-unavailable">
          {t('media.unavailable', { capability })}
        </p>
      ) : (
        children(models, effective, setSelected)
      )}
    </section>
  )
}

function SpeechPanel() {
  const { t } = useTranslations()
  const [text, setText] = useState('')
  const [voice, setVoice] = useState('alloy')
  const [format, setFormat] = useState<(typeof SPEECH_FORMATS)[number]>('mp3')
  const [audioUrl, setAudioUrl] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => () => {
    if (audioUrl) URL.revokeObjectURL(audioUrl)
  }, [audioUrl])

  return (
    <Panel title={t('media.tabs.speech')} capability="speech">
      {(models, model, setModel) => (
        <form
          className="flex flex-col gap-3"
          onSubmit={async (e: FormEvent) => {
            e.preventDefault()
            setBusy(true)
            setError(null)
            try {
              const res = await rawRequest('/v1/audio/speech', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model, input: text, voice, response_format: format }),
              })
              const blob = await res.blob()
              setAudioUrl(URL.createObjectURL(blob))
            } catch (err) {
              setError(errorText(err))
            } finally {
              setBusy(false)
            }
          }}
        >
          <ModelSelect models={models} value={model} onChange={setModel} disabled={busy} />
          <textarea
            value={text}
            onChange={e => setText(e.target.value)}
            rows={3}
            placeholder={t('media.speech.input')}
            className="rounded-md border bg-background px-3 py-2 text-sm"
          />
          <div className="flex gap-2 flex-wrap">
            <input
              value={voice}
              onChange={e => setVoice(e.target.value)}
              placeholder={t('media.speech.voice')}
              className="rounded-md border bg-background px-3 py-2 text-sm"
            />
            <select
              value={format}
              onChange={e => setFormat(e.target.value as (typeof SPEECH_FORMATS)[number])}
              className="rounded-md border bg-background px-3 py-2 text-sm"
            >
              {SPEECH_FORMATS.map(f => (
                <option key={f} value={f}>{f}</option>
              ))}
            </select>
            <Button type="submit" disabled={busy || !text.trim()}>
              {busy ? t('media.generating') : t('media.generate')}
            </Button>
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {audioUrl && <audio controls src={audioUrl} className="w-full" />}
        </form>
      )}
    </Panel>
  )
}

function TranscriptionPanel() {
  const { t } = useTranslations()
  const [file, setFile] = useState<File | null>(null)
  const [result, setResult] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  return (
    <Panel title={t('media.tabs.transcription')} capability="transcription">
      {(models, model, setModel) => (
        <form
          className="flex flex-col gap-3"
          onSubmit={async (e: FormEvent) => {
            e.preventDefault()
            if (!file) return
            setBusy(true)
            setError(null)
            try {
              const form = new FormData()
              form.append('model', model)
              form.append('file', file)
              const res = await rawRequest('/v1/audio/transcriptions', { method: 'POST', body: form })
              const body = await res.json()
              setResult(typeof body.text === 'string' ? body.text : JSON.stringify(body))
            } catch (err) {
              setError(errorText(err))
            } finally {
              setBusy(false)
            }
          }}
        >
          <ModelSelect models={models} value={model} onChange={setModel} disabled={busy} />
          <input
            type="file"
            accept="audio/*"
            onChange={e => setFile(e.target.files?.[0] ?? null)}
            className="text-sm"
          />
          <Button type="submit" disabled={busy || !file}>
            {busy ? t('media.generating') : t('media.transcribe')}
          </Button>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {result !== null && (
            <div className="rounded-md bg-muted p-3 text-sm whitespace-pre-wrap">{result}</div>
          )}
        </form>
      )}
    </Panel>
  )
}

function ImagePanel() {
  const { t } = useTranslations()
  const [mode, setMode] = useState<'generate' | 'edit'>('generate')
  const [prompt, setPrompt] = useState('')
  const [file, setFile] = useState<File | null>(null)
  const [src, setSrc] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const capability = mode === 'generate' ? 'image_generation' : 'image_edit'

  return (
    <Panel title={t('media.tabs.image')} capability={capability}>
      {(models, model, setModel) => (
        <form
          className="flex flex-col gap-3"
          onSubmit={async (e: FormEvent) => {
            e.preventDefault()
            setBusy(true)
            setError(null)
            try {
              let body: { data?: Array<{ b64_json?: string; url?: string }> }
              if (mode === 'generate') {
                const res = await rawRequest('/v1/images/generations', {
                  method: 'POST',
                  headers: { 'Content-Type': 'application/json' },
                  body: JSON.stringify({ model, prompt }),
                })
                body = await res.json()
              } else {
                const form = new FormData()
                form.append('model', model)
                form.append('prompt', prompt)
                if (file) form.append('image', file)
                const res = await rawRequest('/v1/images/edits', { method: 'POST', body: form })
                body = await res.json()
              }
              const first = body.data?.[0]
              if (first?.b64_json) setSrc(`data:image/png;base64,${first.b64_json}`)
              else if (first?.url) setSrc(first.url)
              else setError(t('media.noImage'))
            } catch (err) {
              setError(errorText(err))
            } finally {
              setBusy(false)
            }
          }}
        >
          <div className="flex gap-2 text-sm">
            <button
              type="button"
              onClick={() => setMode('generate')}
              className={mode === 'generate' ? 'font-semibold underline' : 'text-muted-foreground'}
            >
              {t('media.image.generate')}
            </button>
            <button
              type="button"
              onClick={() => setMode('edit')}
              className={mode === 'edit' ? 'font-semibold underline' : 'text-muted-foreground'}
            >
              {t('media.image.edit')}
            </button>
          </div>
          <ModelSelect models={models} value={model} onChange={setModel} disabled={busy} />
          <textarea
            value={prompt}
            onChange={e => setPrompt(e.target.value)}
            rows={2}
            placeholder={t('media.image.prompt')}
            className="rounded-md border bg-background px-3 py-2 text-sm"
          />
          {mode === 'edit' && (
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp"
              onChange={e => setFile(e.target.files?.[0] ?? null)}
              className="text-sm"
            />
          )}
          <Button type="submit" disabled={busy || !prompt.trim() || (mode === 'edit' && !file)}>
            {busy ? t('media.generating') : t('media.generate')}
          </Button>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {src && <img src={src} alt="" className="max-h-96 rounded-md border" />}
        </form>
      )}
    </Panel>
  )
}

function EmbeddingsPanel() {
  const { t } = useTranslations()
  const [text, setText] = useState('')
  const [summary, setSummary] = useState<{ count: number; dimension: number; preview: number[] } | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  return (
    <Panel title={t('media.tabs.embeddings')} capability="embeddings">
      {(models, model, setModel) => (
        <form
          className="flex flex-col gap-3"
          onSubmit={async (e: FormEvent) => {
            e.preventDefault()
            const inputs = text.split('\n').map(s => s.trim()).filter(Boolean)
            setBusy(true)
            setError(null)
            try {
              const res = await rawRequest('/v1/embeddings', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model, input: inputs }),
              })
              const body = (await res.json()) as { data: Array<{ embedding: number[] }> }
              const first = body.data[0]?.embedding ?? []
              setSummary({ count: body.data.length, dimension: first.length, preview: first.slice(0, 8) })
            } catch (err) {
              setError(errorText(err))
            } finally {
              setBusy(false)
            }
          }}
        >
          <ModelSelect models={models} value={model} onChange={setModel} disabled={busy} />
          <textarea
            value={text}
            onChange={e => setText(e.target.value)}
            rows={4}
            placeholder={t('media.embeddings.help')}
            className="rounded-md border bg-background px-3 py-2 text-sm"
          />
          <Button type="submit" disabled={busy || !text.trim()}>
            {busy ? t('media.generating') : t('media.embeddings.run')}
          </Button>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {summary && (
            <div className="rounded-md bg-muted p-3 text-sm font-mono">
              <div>{t('media.embeddings.vectors', { count: summary.count, dimension: summary.dimension })}</div>
              <div className="text-muted-foreground break-all">[{summary.preview.map(n => n.toFixed(4)).join(', ')}, …]</div>
            </div>
          )}
        </form>
      )}
    </Panel>
  )
}

interface VideoJobView {
  id: string
  status: string
  error?: string | null
}

function VideoPanel() {
  const { t } = useTranslations()
  const [prompt, setPrompt] = useState('')
  const [job, setJob] = useState<VideoJobView | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const pollRef = useRef<number | null>(null)

  useEffect(() => () => {
    if (pollRef.current) window.clearTimeout(pollRef.current)
  }, [])

  const poll = (id: string) => {
    pollRef.current = window.setTimeout(async () => {
      try {
        const next = await apiFetch<VideoJobView>(`/v1/videos/${encodeURIComponent(id)}`)
        setJob(next)
        if (!TERMINAL_VIDEO_STATES.includes(next.status)) poll(id)
      } catch (err) {
        setError(errorText(err))
      }
    }, 3000)
  }

  return (
    <Panel title={t('media.tabs.video')} capability="video_generation">
      {(models, model, setModel) => (
        <div className="flex flex-col gap-3">
          <ModelSelect models={models} value={model} onChange={setModel} disabled={busy} />
          <textarea
            value={prompt}
            onChange={e => setPrompt(e.target.value)}
            rows={2}
            placeholder={t('media.video.prompt')}
            className="rounded-md border bg-background px-3 py-2 text-sm"
          />
          <Button
            disabled={busy || !prompt.trim()}
            onClick={async () => {
              setBusy(true)
              setError(null)
              try {
                const created = await apiFetch<VideoJobView>('/v1/videos', {
                  method: 'POST',
                  headers: { 'Idempotency-Key': crypto.randomUUID() },
                  body: JSON.stringify({ model, prompt }),
                })
                setJob(created)
                if (!TERMINAL_VIDEO_STATES.includes(created.status)) poll(created.id)
              } catch (err) {
                setError(errorText(err))
              } finally {
                setBusy(false)
              }
            }}
          >
            {busy ? t('media.generating') : t('media.generate')}
          </Button>
          {error && <p className="text-sm text-destructive">{error}</p>}
          {job && (
            <div className="flex flex-col gap-2 text-sm">
              <div>
                <span className="font-mono">{job.id}</span> · {t('media.video.status')}: {job.status}
              </div>
              {job.error && <p className="text-destructive">{job.error}</p>}
              {job.status === 'completed' && (
                <video controls src={`${BASE}/v1/videos/${encodeURIComponent(job.id)}/content`} className="w-full rounded-md" />
              )}
              {!TERMINAL_VIDEO_STATES.includes(job.status) || job.status === 'completed' ? (
                <Button
                  variant="outline"
                  onClick={async () => {
                    if (pollRef.current) window.clearTimeout(pollRef.current)
                    try {
                      await apiFetch(`/v1/videos/${encodeURIComponent(job.id)}`, { method: 'DELETE' })
                      setJob({ ...job, status: 'deleted' })
                    } catch (err) {
                      setError(errorText(err))
                    }
                  }}
                >
                  {t('media.video.delete')}
                </Button>
              ) : null}
            </div>
          )}
        </div>
      )}
    </Panel>
  )
}

interface DeclaredMediaModel {
  id: number
  platform: string
  modelId: string
  capability: string
  enabled: boolean
}

const DECLARABLE = ['speech', 'transcription', 'translation', 'image_generation', 'image_edit', 'embeddings', 'video_generation'] as const

function DeclarePanel() {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  const { data: declared = [] } = useQuery<{ data: DeclaredMediaModel[] }, ApiError, DeclaredMediaModel[]>({
    queryKey: ['media-models-admin'],
    queryFn: () => apiFetch<{ data: DeclaredMediaModel[] }>('/api/media-models'),
    select: body => body.data,
  })
  const [platform, setPlatform] = useState('')
  const [modelId, setModelId] = useState('')
  const [capability, setCapability] = useState<(typeof DECLARABLE)[number]>('speech')
  const [error, setError] = useState<string | null>(null)

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['media-models-admin'] })
    DECLARABLE.forEach(c => queryClient.invalidateQueries({ queryKey: ['media-models', c] }))
  }

  return (
    <section className="rounded-lg border bg-card p-4 flex flex-col gap-4 lg:col-span-2">
      <div>
        <h2 className="text-base font-semibold">{t('media.declare.title')}</h2>
        <p className="text-sm text-muted-foreground">{t('media.declare.description')}</p>
      </div>
      <form
        className="flex flex-wrap gap-2 items-end"
        onSubmit={async (e: FormEvent) => {
          e.preventDefault()
          setError(null)
          try {
            await apiFetch('/api/media-models', {
              method: 'POST',
              body: JSON.stringify({ platform, modelId, capability }),
            })
            setModelId('')
            refresh()
          } catch (err) {
            setError(errorText(err))
          }
        }}
      >
        <input
          value={platform}
          onChange={e => setPlatform(e.target.value)}
          placeholder={t('media.declare.platform')}
          className="rounded-md border bg-background px-3 py-2 text-sm"
        />
        <input
          value={modelId}
          onChange={e => setModelId(e.target.value)}
          placeholder={t('media.declare.modelId')}
          className="rounded-md border bg-background px-3 py-2 text-sm font-mono"
        />
        <select
          value={capability}
          onChange={e => setCapability(e.target.value as (typeof DECLARABLE)[number])}
          className="rounded-md border bg-background px-3 py-2 text-sm"
        >
          {DECLARABLE.map(c => (
            <option key={c} value={c}>{c}</option>
          ))}
        </select>
        <Button type="submit" disabled={!platform.trim() || !modelId.trim()}>
          {t('media.declare.submit')}
        </Button>
      </form>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <ul className="flex flex-col gap-1 text-sm">
        {declared.length === 0 && <li className="text-muted-foreground">{t('media.declare.empty')}</li>}
        {declared.map(row => (
          <li key={row.id} className="flex items-center gap-2">
            <span className="font-mono">{row.modelId}</span>
            <span className="text-muted-foreground">{row.platform} · {row.capability}</span>
            <button
              type="button"
              className="ml-auto text-destructive underline"
              onClick={async () => {
                try {
                  await apiFetch(`/api/media-models/${row.id}`, { method: 'DELETE' })
                  refresh()
                } catch (err) {
                  setError(errorText(err))
                }
              }}
            >
              {t('media.declare.remove')}
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}

export default function MediaPlaygroundPage() {
  const { t } = useTranslations()
  const [tab, setTab] = useState<Tab>('speech')


  return (
    <div className="max-w-5xl">
      <PageHeader title={t('media.title')} description={t('media.description')} />
      <nav className="flex gap-2 flex-wrap mb-6" role="tablist">
        {TABS.map(id => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            type="button"
            onClick={() => setTab(id)}
            className={`rounded-md px-3 py-1.5 text-sm border ${tab === id ? 'bg-accent font-semibold' : 'text-muted-foreground'}`}
          >
            {t(`media.tabs.${id}`)}
          </button>
        ))}
      </nav>
      <div className="grid gap-4 lg:grid-cols-2">
        {tab === 'speech' && <SpeechPanel />}
        {tab === 'transcription' && <TranscriptionPanel />}
        {tab === 'image' && <ImagePanel />}
        {tab === 'embeddings' && <EmbeddingsPanel />}
        {tab === 'video' && <VideoPanel />}
        {tab === 'declare' && <DeclarePanel />}
      </div>
    </div>
  )
}
