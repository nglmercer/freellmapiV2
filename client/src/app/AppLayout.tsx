import { lazy, Suspense } from 'react'
import { Navigate, NavLink, Route, Routes, useLocation } from 'react-router-dom'
import { useTranslations } from '@/hooks/useTranslations'
import PageSkeleton from '@/components/PageSkeleton'
import { DarkModeToggle, LanguageToggle, SignOutButton } from './header-controls'

const KeysPage = lazy(() => import('@/pages/KeysPage'))
const PlaygroundPage = lazy(() => import('@/pages/PlaygroundPage'))
const FallbackPage = lazy(() => import('@/pages/FallbackPage'))
const AnalyticsPage = lazy(() => import('@/pages/AnalyticsPage'))
const ProvidersPage = lazy(() => import('@/pages/ProvidersPage'))
const ProviderModelsPage = lazy(() => import('@/pages/ProviderModelsPage'))
const ModelExplorerPage = lazy(() => import('@/pages/ModelExplorerPage'))
const SetupPage = lazy(() => import('@/pages/SetupPage'))

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

function Brand() {
  return (
    <div className="flex items-center gap-2">
      <span className="inline-block size-2 rounded-full bg-foreground" />
      <span className="font-semibold tracking-tight text-sm">FreeLLMAPI</span>
    </div>
  )
}

export function AppLayout() {
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
        <div className="max-w-6xl mx-auto px-4 flex flex-wrap items-center">
          <Brand />
          <nav className="flex items-center gap-4 ml-4 overflow-x-auto max-w-full order-3 sm:order-0">
            <NavItem to="/playground">{t('app.nav.playground')}</NavItem>
            <NavItem to="/keys">{t('app.nav.keys')}</NavItem>
            <NavItem to="/fallback">{t('app.nav.fallback')}</NavItem>
            <NavItem to="/models">{t('metadata.explorer')}</NavItem>
            <NavItem to="/analytics">{t('app.nav.analytics')}</NavItem>
          </nav>
          <div className="ml-auto py-2 flex items-center gap-2">
            <LanguageToggle />
            <DarkModeToggle />
            <SignOutButton />
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
            <Route path="/models" element={<ModelExplorerPage />} />
            <Route path="/analytics" element={<AnalyticsPage />} />
            <Route path="/test" element={<Navigate to="/playground" replace />} />
            <Route path="/health" element={<Navigate to="/keys" replace />} />
          </Routes>
        </Suspense>
      </main>
    </div>
  )
}
