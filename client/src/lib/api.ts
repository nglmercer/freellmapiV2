const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');
const ADMIN_KEY_STORAGE = 'freellmapi.adminApiKey';
let runtimeAdminApiKey = '';

function consumeBrowserBootstrapKey(): string {
  if (typeof window === 'undefined' || !window.location.hash) return '';

  const params = new URLSearchParams(window.location.hash.slice(1));
  const key = params.get('adminKey')?.trim() ?? '';
  if (!key) return '';

  runtimeAdminApiKey = key;
  try {
    window.sessionStorage.setItem(ADMIN_KEY_STORAGE, key);
  } catch {
    // Keep the key in memory when browser storage is unavailable.
  }

  // The tray launcher passes the credential in a fragment, which is not sent
  // to the server. Remove it immediately so it does not remain in browser
  // history or be copied as part of the dashboard URL.
  window.history.replaceState(
    window.history.state,
    document.title,
    window.location.pathname + window.location.search,
  );
  return key;
}

export function getAdminApiKey(): string {
  const bootstrapKey = consumeBrowserBootstrapKey();
  if (bootstrapKey) return bootstrapKey;
  if (runtimeAdminApiKey) return runtimeAdminApiKey;

  if (typeof window !== 'undefined') {
    try {
      const sessionKey = window.sessionStorage.getItem(ADMIN_KEY_STORAGE)?.trim();
      if (sessionKey) {
        runtimeAdminApiKey = sessionKey;
        return sessionKey;
      }
    } catch {
      // Session storage can be unavailable in privacy-restricted browsers.
    }
  }
  return import.meta.env.VITE_ADMIN_API_KEY?.trim() ?? '';
}

export function setAdminApiKey(key: string): void {
  runtimeAdminApiKey = key.trim();
  if (typeof window === 'undefined') return;
  try {
    if (runtimeAdminApiKey) {
      window.sessionStorage.setItem(ADMIN_KEY_STORAGE, runtimeAdminApiKey);
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
