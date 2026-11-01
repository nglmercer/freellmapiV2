import type { Platform } from '../../../shared/types'

export interface PlatformInfo {
  value: Platform
  label: string
  /** Works without a key; enabling it stores an "anonymous" entry. */
  anonymous: boolean
  /** Needs an account identifier in addition to the token. */
  needsAccountId: boolean
}

/** Every built-in provider, in the order the dashboard lists them. */
export const PLATFORMS: PlatformInfo[] = [
  { value: 'google', label: 'Google AI Studio', anonymous: false, needsAccountId: false },
  { value: 'groq', label: 'Groq', anonymous: false, needsAccountId: false },
  { value: 'cerebras', label: 'Cerebras', anonymous: false, needsAccountId: false },
  { value: 'sambanova', label: 'SambaNova', anonymous: false, needsAccountId: false },
  { value: 'nvidia', label: 'NVIDIA NIM', anonymous: false, needsAccountId: false },
  { value: 'mistral', label: 'Mistral', anonymous: false, needsAccountId: false },
  { value: 'openrouter', label: 'OpenRouter', anonymous: false, needsAccountId: false },
  { value: 'github', label: 'GitHub Models', anonymous: false, needsAccountId: false },
  { value: 'cohere', label: 'Cohere', anonymous: false, needsAccountId: false },
  { value: 'cloudflare', label: 'Cloudflare Workers AI', anonymous: false, needsAccountId: true },
  { value: 'zhipu', label: 'Zhipu AI (Z.ai)', anonymous: false, needsAccountId: false },
  { value: 'ollama', label: 'Ollama Cloud', anonymous: false, needsAccountId: false },
  { value: 'kilo', label: 'Kilo Gateway', anonymous: true, needsAccountId: false },
  { value: 'pollinations', label: 'Pollinations', anonymous: true, needsAccountId: false },
  { value: 'llm7', label: 'LLM7', anonymous: true, needsAccountId: false },
]

export function platformInfo(value: string): PlatformInfo | undefined {
  return PLATFORMS.find((p) => p.value === value)
}
