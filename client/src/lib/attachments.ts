/**
 * Files a user can attach to a playground message.
 *
 * - Images are sent as `image_url` parts (data URLs).
 * - Audio is sent as `input_audio` parts (base64 plus a container format).
 * - Plain-text documents are read in the browser and added to the message text.
 *
 * PDF, Word and other binary documents need text extraction, which the server
 * does not perform. They are rejected here with a clear message rather than
 * being sent in a form no model can read.
 */

export type Attachment =
  | { kind: 'image'; name: string; dataUrl: string }
  | { kind: 'audio'; name: string; base64: string; format: string; dataUrl: string }
  | { kind: 'document'; name: string; text: string }

export const LIMITS_MB = { image: 5, audio: 10, document: 1 } as const

const MB = 1024 * 1024

const IMAGE_EXT = new Set(['png', 'jpg', 'jpeg', 'webp', 'gif'])
const AUDIO_FORMAT_BY_EXT: Record<string, string> = {
  wav: 'wav',
  mp3: 'mp3',
  flac: 'flac',
  ogg: 'ogg',
  opus: 'opus',
  m4a: 'm4a',
  aac: 'aac',
  webm: 'webm',
}
const TEXT_EXT = new Set([
  'txt', 'md', 'csv', 'tsv', 'json', 'log', 'xml', 'yaml', 'yml', 'html', 'css',
  'js', 'ts', 'tsx', 'jsx', 'py', 'rs', 'go', 'java', 'c', 'h', 'cpp', 'sql', 'sh',
])

export class AttachmentError extends Error {
  /** i18n key, with `params` for interpolation. */
  key: string
  params: Record<string, string | number>

  constructor(key: string, params: Record<string, string | number> = {}) {
    super(key)
    this.key = key
    this.params = params
  }
}

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot >= 0 ? name.slice(dot + 1).toLowerCase() : ''
}

function imageMimeFor(ext: string): string {
  return ext === 'jpg' ? 'image/jpeg' : `image/${ext}`
}

function readAsDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new AttachmentError('playground.attachReadFailed'))
    reader.readAsDataURL(file)
  })
}

/**
 * Reads a file into an attachment. Throws [`AttachmentError`] for unsupported
 * types and oversized files.
 */
export async function readAttachment(file: File): Promise<Attachment> {
  const ext = extensionOf(file.name)
  const name = file.name

  if (IMAGE_EXT.has(ext) || file.type.startsWith('image/')) {
    if (file.size > LIMITS_MB.image * MB) {
      throw new AttachmentError('playground.attachTooLarge', { name, limit: LIMITS_MB.image })
    }
    const dataUrl = await readAsDataUrl(file)
    const sent = dataUrl.replace(/^data:[^;]*/, `data:${imageMimeFor(ext) || file.type}`)
    return { kind: 'image', name, dataUrl: sent }
  }

  if (ext in AUDIO_FORMAT_BY_EXT || file.type.startsWith('audio/')) {
    if (file.size > LIMITS_MB.audio * MB) {
      throw new AttachmentError('playground.attachTooLarge', { name, limit: LIMITS_MB.audio })
    }
    const format = AUDIO_FORMAT_BY_EXT[ext] ?? (file.type.split('/')[1] || 'wav')
    const dataUrl = await readAsDataUrl(file)
    const base64 = dataUrl.slice(dataUrl.indexOf(',') + 1)
    return { kind: 'audio', name, base64, format, dataUrl }
  }

  if (TEXT_EXT.has(ext) || file.type.startsWith('text/')) {
    if (file.size > LIMITS_MB.document * MB) {
      throw new AttachmentError('playground.attachTooLarge', { name, limit: LIMITS_MB.document })
    }
    return { kind: 'document', name, text: await file.text() }
  }

  throw new AttachmentError('playground.attachUnsupported', { name })
}

/** `accept` value for the file picker: the types readAttachment understands. */
export const ACCEPT_ATTRIBUTE = [
  'image/png,image/jpeg,image/webp,image/gif',
  'audio/*',
  ...Array.from(TEXT_EXT).map(e => `.${e}`),
].join(',')

/** Minimal content-part shapes the chat API accepts. */
export type ContentPart =
  | { type: 'text'; text: string }
  | { type: 'image_url'; image_url: { url: string } }
  | { type: 'input_audio'; input_audio: { data: string; format: string } }

/**
 * Builds the `content` of a user message. Plain text stays a string; a message
 * with images or audio becomes content parts. Documents are appended to the text.
 */
export function buildContent(text: string, attachments: Attachment[]): string | ContentPart[] {
  const docs = attachments.filter((a): a is Extract<Attachment, { kind: 'document' }> => a.kind === 'document')
  const combinedText = [text, ...docs.map(d => `--- ${d.name} ---\n${d.text}`)]
    .filter(Boolean)
    .join('\n\n')

  const media = attachments.filter(a => a.kind !== 'document')
  if (media.length === 0) return combinedText

  const parts: ContentPart[] = []
  if (combinedText) parts.push({ type: 'text', text: combinedText })
  for (const a of attachments) {
    if (a.kind === 'image') parts.push({ type: 'image_url', image_url: { url: a.dataUrl } })
    if (a.kind === 'audio') parts.push({ type: 'input_audio', input_audio: { data: a.base64, format: a.format } })
  }
  return parts
}
