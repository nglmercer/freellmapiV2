import { Suspense } from 'react'
import { Navigate, NavLink, Route, Routes, useLocation } from 'react-router-dom'
import { useTranslations } from '@/hooks/useTranslations'
import PageSkeleton from '@/components/PageSkeleton'
import { DarkModeToggle, LanguageToggle, SignOutButton } from './header-controls'
import { pageLoaders, pages } from './routes'
import { RouteErrorBoundary } from './RouteErrorBoundary'

/** Starts downloading a page chunk early so the click renders without a wait. */
function preload(load: () => Promise<unknown>) {
  return () => {
    void load().catch(() => {
      // Ignored: the page's own lazy() call retries and reports the failure.
    })
  }
}

function NavItem({
  to,
  preloadPage,
  children,
}: {
  to: string
  preloadPage: () => Promise<unknown>
  children: React.ReactNode
}) {
  const onPreload = preload(preloadPage)
  return (
    <NavLink
      to={to}
      onMouseEnter={onPreload}
      onFocus={onPreload}
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
      <RouteErrorBoundary key={location.pathname}>
        <Suspense fallback={<PageSkeleton />}>
          <pages.setup />
        </Suspense>
      </RouteErrorBoundary>
    )
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-40 bg-background/80 backdrop-blur border-b">
        <div className="max-w-6xl mx-auto px-4 flex flex-wrap items-center">
          <Brand />
          <nav className="flex items-center gap-4 ml-4 overflow-x-auto max-w-full order-3 sm:order-0">
            <NavItem to="/playground" preloadPage={pageLoaders.playground}>{t('app.nav.playground')}</NavItem>
            <NavItem to="/media" preloadPage={pageLoaders.media}>{t('app.nav.media')}</NavItem>
            <NavItem to="/keys" preloadPage={pageLoaders.keys}>{t('app.nav.keys')}</NavItem>
            <NavItem to="/fallback" preloadPage={pageLoaders.fallback}>{t('app.nav.fallback')}</NavItem>
            <NavItem to="/models" preloadPage={pageLoaders.models}>{t('metadata.explorer')}</NavItem>
            <NavItem to="/analytics" preloadPage={pageLoaders.analytics}>{t('app.nav.analytics')}</NavItem>
          </nav>
          <div className="ml-auto py-2 flex items-center gap-2">
            <LanguageToggle />
            <DarkModeToggle />
            <SignOutButton />
          </div>
        </div>
      </header>
      <main className="max-w-6xl mx-auto px-6 py-8">
        <RouteErrorBoundary key={location.pathname}>
          <Suspense fallback={<PageSkeleton />}>
            <Routes>
              <Route path="/" element={<Navigate to="/playground" replace />} />
              <Route path="/providers" element={<pages.providers />} />
              <Route path="/providers/:id/models" element={<pages.providerModels />} />
              <Route path="/playground" element={<pages.playground />} />
              <Route path="/media" element={<pages.media />} />
              <Route path="/keys" element={<pages.keys />} />
              <Route path="/fallback" element={<pages.fallback />} />
              <Route path="/models" element={<pages.models />} />
              <Route path="/analytics" element={<pages.analytics />} />
              <Route path="/test" element={<Navigate to="/playground" replace />} />
              <Route path="/health" element={<Navigate to="/keys" replace />} />
            </Routes>
          </Suspense>
        </RouteErrorBoundary>
      </main>
    </div>
  )
}
