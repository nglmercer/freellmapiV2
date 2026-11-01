import type { Platform } from '../../../../shared/types'

/** A provider key added during setup, shown in the list on the keys step. */
export interface AddedKey {
  platform: string
  label: string
  maskedKey: string
}

export interface CreateKeyResponse {
  id: number
  platform: string
  label: string
  maskedKey: string
  status: string
  enabled: boolean
}

export const PLATFORMS: { value: Platform; label: string; description: string }[] = [
  { value: 'google', label: 'Google AI Studio', description: 'Gemini models (recommended)' },
  { value: 'groq', label: 'Groq', description: 'Fast inference, Llama/Mixtral' },
  { value: 'cerebras', label: 'Cerebras', description: 'Ultra-fast inference' },
  { value: 'sambanova', label: 'SambaNova', description: 'Fast inference' },
  { value: 'nvidia', label: 'NVIDIA NIM', description: 'NVIDIA hosted models' },
  { value: 'mistral', label: 'Mistral', description: 'Mistral/Pixtral models' },
  { value: 'openrouter', label: 'OpenRouter', description: 'Aggregator, many free models' },
  { value: 'github', label: 'GitHub Models', description: 'GitHub hosted models' },
  { value: 'cohere', label: 'Cohere', description: 'Command/R+ models' },
  { value: 'cloudflare', label: 'Cloudflare Workers AI', description: 'Edge AI models' },
  { value: 'zhipu', label: 'Zhipu AI', description: 'GLM models' },
  { value: 'ollama', label: 'Ollama Cloud', description: 'Cloud hosted Ollama' },
  { value: 'kilo', label: 'Kilo Gateway', description: 'Anonymous access OK' },
  { value: 'pollinations', label: 'Pollinations', description: 'Anonymous access OK' },
  { value: 'llm7', label: 'LLM7', description: 'Anonymous access OK' },
]

export const ANONYMOUS_PLATFORMS = new Set(['kilo', 'pollinations', 'llm7'])
