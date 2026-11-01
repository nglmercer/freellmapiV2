const BASE = import.meta.env.BASE_URL.replace(/\/$/, '')

/** Fired on the window when a dashboard request is refused with 401. */
export const UNAUTHORIZED_EVENT = 'freellmapi:unauthorized'

/** Account routes that answer 401 for a normal reason, not an expired session. */
const PUBLIC_ACCOUNT_ROUTES = ['/api/account-status', '/api/account/create', '/api/account/join']

export class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const headers = new Headers(options?.headers)
  if (!headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
  // Same-origin requests carry the Better Auth session cookie automatically.
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers,
    credentials: 'same-origin',
  })
  if (!res.ok) {
    const body = await res.json().catch(() => null)
    const message =
      typeof body?.error === 'string'
        ? body.error
        : (body?.error?.message as string | undefined) ?? res.statusText
    const isAccountRoute =
      path.startsWith('/api/auth/') ||
      PUBLIC_ACCOUNT_ROUTES.includes(path)
    if (res.status === 401 && path.startsWith('/api/') && !isAccountRoute) {
      window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
    }
    throw new ApiError(message || `HTTP ${res.status}`, res.status)
  }
  return res.json()
}

export interface AccountStatus {
  hasAccount: boolean
}

export function createAccount(input: { username: string; password: string }) {
  return apiFetch('/api/account/create', { method: 'POST', body: JSON.stringify(input) })
}

export function signInWithPassword(input: { username: string; password: string }) {
  return apiFetch('/api/auth/sign-in/username', { method: 'POST', body: JSON.stringify(input) })
}

export function joinWithCode(code: string) {
  return apiFetch('/api/account/join', { method: 'POST', body: JSON.stringify({ code }) })
}

export function signOut() {
  return apiFetch('/api/auth/sign-out', { method: 'POST', body: JSON.stringify({}) })
}

export interface JoinCodeStatus {
  enabled: boolean
}

export function getJoinCodeStatus() {
  return apiFetch<JoinCodeStatus>('/api/account/join-code')
}

/** Generates a join code and returns it once. Replaces any previous code. */
export function generateJoinCode() {
  return apiFetch<JoinCodeStatus & { code: string }>('/api/account/join-code', {
    method: 'POST',
    body: JSON.stringify({}),
  })
}

export function revokeJoinCode() {
  return apiFetch<JoinCodeStatus>('/api/account/join-code', { method: 'DELETE' })
}
