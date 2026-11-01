const BASE = import.meta.env.BASE_URL.replace(/\/$/, '')

/** Fired on the window when a dashboard request is refused with 401. */
export const UNAUTHORIZED_EVENT = 'freellmapi:unauthorized'

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
    const isAccountRoute = path.startsWith('/api/auth/') || path === '/api/account-status'
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

export function signUp(input: { name: string; email: string; password: string }) {
  return apiFetch('/api/auth/sign-up/email', { method: 'POST', body: JSON.stringify(input) })
}

export function signIn(input: { email: string; password: string }) {
  return apiFetch('/api/auth/sign-in/email', { method: 'POST', body: JSON.stringify(input) })
}

export function signOut() {
  return apiFetch('/api/auth/sign-out', { method: 'POST', body: JSON.stringify({}) })
}
