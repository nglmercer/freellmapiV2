const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');
const ADMIN_KEY_STORAGE = 'freellmapi.adminApiKey';

export function getAdminApiKey(): string {
  if (typeof window !== 'undefined') {
    try {
      const sessionKey = window.sessionStorage.getItem(ADMIN_KEY_STORAGE)?.trim();
      if (sessionKey) return sessionKey;
    } catch {
      // Session storage can be unavailable in privacy-restricted browsers.
    }
  }
  return import.meta.env.VITE_ADMIN_API_KEY?.trim() ?? '';
}

export function setAdminApiKey(key: string): void {
  if (typeof window === 'undefined') return;
  try {
    if (key.trim()) {
      window.sessionStorage.setItem(ADMIN_KEY_STORAGE, key.trim());
    } else {
      window.sessionStorage.removeItem(ADMIN_KEY_STORAGE);
    }
  } catch {
    // The build-time VITE_ADMIN_API_KEY remains available as a fallback.
  }
}

export async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const needsAdminAuth = path.startsWith('/api/') && path !== '/api/ping';
  const adminKey = getAdminApiKey();
  const headers = new Headers(options?.headers);
  if (!headers.has('Content-Type')) headers.set('Content-Type', 'application/json');
  if (needsAdminAuth && adminKey) headers.set('Authorization', 'Bearer ' + adminKey);
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: { message: res.statusText } }));
    throw new Error(body.error?.message ?? `HTTP ${res.status}`);
  }
  return res.json();
}
