import { lazy, Suspense, useState } from 'react'
import { BrowserRouter, Routes, Route, Navigate, NavLink, useLocation } from 'react-router-dom'
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { apiFetch, getAdminApiKey, setAdminApiKey } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'
import { switchLanguage, getCurrentLanguage } from '@/i18n'
import PageSkeleton from '@/components/PageSkeleton'

const KeysPage = lazy(() => import('./pages/KeysPage'))
const PlaygroundPage = lazy(() => import('./pages/PlaygroundPage'))
const FallbackPage = lazy(() => import('./pages/FallbackPage'))
const AnalyticsPage = lazy(() => import('./pages/AnalyticsPage'))
const ProvidersPage = lazy(() => import('./pages/ProvidersPage'))
const ProviderModelsPage = lazy(() => import('./pages/ProviderModelsPage'))
const SetupPage = lazy(() => import('./pages/SetupPage'))

const queryClient = new QueryClient()

function NavItem({ to, children }: { to: string; children: React.ReactNode }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        `relative text-sm px-1 py-4 transition-colors ${
          isActive
            ? 'text-foreground after:absolute after:inset-x-0 after:-bottom-px after:h-px after:bg-foreground'
            : 'text-muted-foreground hover:text-foreground'
        }`
      }
    >
      {children}
    </NavLink>
  )
}

function getInitialDark(): boolean {
  if (typeof window === 'undefined') return false
  const stored = localStorage.getItem('theme')
  if (stored === 'dark') return true
  if (stored === 'light') return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function DarkModeToggle() {
  const [dark, setDark] = useState(() => {
    const initial = getInitialDark()
    if (initial) document.documentElement.classList.add('dark')
    return initial
  })

  function toggle() {
    const next = !dark
    setDark(next)
    document.documentElement.classList.toggle('dark', next)
    localStorage.setItem('theme', next ? 'dark' : 'light')
  }

  return (
    <Button variant="ghost" size="sm" onClick={toggle} aria-label="Toggle theme">
      {dark ? (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>
      ) : (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
      )}
    </Button>
  )
}

function Brand() {
  return (
    <div className="flex items-center gap-2">
      <span className="inline-block size-2 rounded-full bg-foreground" />
      <span className="font-semibold tracking-tight text-sm">FreeLLMAPI</span>
    </div>
  )
}

interface SetupStatus {
  isSetup: boolean
}

function SetupGuard({ children, status }: { children: React.ReactNode; status: SetupStatus }) {
  const location = useLocation()

  if (!status.isSetup && location.pathname !== '/setup') {
    return <Navigate to="/setup" replace />
  }

  if (status.isSetup && location.pathname === '/setup') {
    return <Navigate to="/playground" replace />
  }

  return <>{children}</>
}

function AdminKeyPrompt({ initialKey, onSubmit, hasError }: {
  initialKey: string
  onSubmit: (key: string) => void
  hasError: boolean
}) {
  const { t } = useTranslations()
  const [key, setKey] = useState(initialKey)

  function submit(event: React.FormEvent) {
    event.preventDefault()
    const trimmed = key.trim()
    if (trimmed) onSubmit(trimmed)
  }

  return (
    <div className="min-h-screen bg-background flex items-center justify-center px-6">
      <form onSubmit={submit} className="w-full max-w-md rounded-lg border bg-card p-6 space-y-5">
        <div>
          <div className="flex items-center gap-2 mb-3">
            <span className="inline-block size-2 rounded-full bg-foreground" />
            <span className="font-semibold tracking-tight text-sm">FreeLLMAPI</span>
          </div>
          <h1 className="text-lg font-semibold tracking-tight">{t('adminAuth.title')}</h1>
          <p className="text-sm text-muted-foreground mt-2">{t('adminAuth.description')}</p>
        </div>
        <div className="space-y-1.5">
          <label htmlFor="admin-api-key" className="text-xs font-medium">{t('adminAuth.label')}</label>
          <input
            id="admin-api-key"
            type="password"
            value={key}
            onChange={event => setKey(event.target.value)}
            placeholder={t('adminAuth.placeholder')}
            autoComplete="current-password"
            autoFocus
            className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm outline-none focus-visible:ring-1 focus-visible:ring-ring"
          />
        </div>
        {hasError && <p className="text-sm text-destructive">{t('adminAuth.invalid')}</p>}
        <Button type="submit" className="w-full" disabled={!key.trim()}>
          {t('adminAuth.continue')}
        </Button>
      </form>
    </div>
  )
}

function AdminKeyGate({ children }: { children: React.ReactNode }) {
  const [adminKey, setAdminKey] = useState(() => getAdminApiKey())
  const setup = useQuery<SetupStatus>({
    queryKey: ['setup-status', adminKey],
    queryFn: () => apiFetch('/api/settings/setup-status'),
    enabled: Boolean(adminKey),
    staleTime: 30_000,
    retry: false,
  })

  if (!adminKey || setup.isError) {
    return (
      <AdminKeyPrompt
        initialKey={adminKey}
        hasError={Boolean(adminKey && setup.isError)}
        onSubmit={key => {
          setAdminApiKey(key)
          setAdminKey(key)
        }}
      />
    )
  }

  if (setup.isLoading || !setup.data) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-sm text-muted-foreground">Loading…</div>
      </div>
    )
  }

  return <SetupGuard status={setup.data}>{children}</SetupGuard>
}

function LanguageToggle() {
  const current = getCurrentLanguage()

  function toggle() {
    switchLanguage(current === 'en' ? 'es' : 'en')
  }

  return (
    <Button variant="ghost" size="sm" onClick={toggle}>
      {current === 'en' ? 'ES' : 'EN'}
    </Button>
  )
}

function AppLayout() {
  const { t } = useTranslations()
  const location = useLocation()
  const isSetup = location.pathname === '/setup'

  if (isSetup) {
    return (
      <Suspense fallback={<PageSkeleton />}>
        <SetupPage />
      </Suspense>
    )
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-40 bg-background/80 backdrop-blur border-b">
        <div className="max-w-6xl mx-auto px-6 flex items-center">
          <Brand />
          <nav className="flex items-center gap-6 ml-10">
            <NavItem to="/playground">{t('app.nav.playground')}</NavItem>
            <NavItem to="/keys">{t('app.nav.keys')}</NavItem>
            <NavItem to="/fallback">{t('app.nav.fallback')}</NavItem>
            <NavItem to="/analytics">{t('app.nav.analytics')}</NavItem>
          </nav>
          <div className="ml-auto py-2 flex items-center gap-2">
            <LanguageToggle />
            <DarkModeToggle />
          </div>
        </div>
      </header>
      <main className="max-w-6xl mx-auto px-6 py-8">
        <Suspense fallback={<PageSkeleton />}>
          <Routes>
            <Route path="/" element={<Navigate to="/playground" replace />} />
            <Route path="/providers" element={<ProvidersPage />} />
            <Route path="/providers/:id/models" element={<ProviderModelsPage />} />
            <Route path="/playground" element={<PlaygroundPage />} />
            <Route path="/keys" element={<KeysPage />} />
            <Route path="/fallback" element={<FallbackPage />} />
            <Route path="/analytics" element={<AnalyticsPage />} />
            <Route path="/test" element={<Navigate to="/playground" replace />} />
            <Route path="/health" element={<Navigate to="/keys" replace />} />
          </Routes>
        </Suspense>
      </main>
    </div>
  )
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter basename={import.meta.env.BASE_URL}>
        <AdminKeyGate>
          <AppLayout />
        </AdminKeyGate>
      </BrowserRouter>
    </QueryClientProvider>
  )
}

export default App
