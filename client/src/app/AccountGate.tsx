import { lazy, Suspense, useEffect } from 'react'
import { Navigate, useLocation } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ApiError, UNAUTHORIZED_EVENT, apiFetch, type AccountStatus } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'

// Only shown to signed-out visitors, so signed-in sessions never download it.
const AuthPanel = lazy(() => import('./AuthPanel').then((m) => ({ default: m.AuthPanel })))

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

/**
 * Shows the sign-up form on first run (no owner account yet), the sign-in form
 * afterwards, and the dashboard once a session cookie is accepted.
 */
export function AccountGate({ children }: { children: React.ReactNode }) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()

  const account = useQuery<AccountStatus>({
    queryKey: ['account-status'],
    queryFn: () => apiFetch('/api/account-status'),
    retry: false,
  })

  const setup = useQuery<SetupStatus>({
    queryKey: ['setup-status'],
    queryFn: () => apiFetch('/api/settings/setup-status'),
    enabled: account.isSuccess,
    staleTime: 30_000,
    retry: false,
  })

  // A request refused with 401 anywhere in the dashboard means the session is
  // gone. Reset only while signed-in data is cached: the reset refetch is itself
  // refused with 401 and must not trigger another reset.
  useEffect(() => {
    const onUnauthorized = () => {
      if (queryClient.getQueryData(['setup-status'])) {
        queryClient.resetQueries({ queryKey: ['setup-status'] })
      }
    }
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized)
  }, [queryClient])

  if (account.isError) {
    return <CenteredMessage text={t('auth.unreachable')} />
  }
  if (account.isLoading || !account.data) {
    return <CenteredMessage text="Loading…" />
  }
  if (setup.isError && setup.error instanceof ApiError && setup.error.status === 401) {
    return (
      <Suspense fallback={<CenteredMessage text="Loading…" />}>
        <AuthPanel hasAccount={account.data.hasAccount} />
      </Suspense>
    )
  }
  if (setup.isError) {
    return <CenteredMessage text={t('auth.unreachable')} />
  }
  if (setup.isLoading || !setup.data) {
    return <CenteredMessage text="Loading…" />
  }

  return <SetupGuard status={setup.data}>{children}</SetupGuard>
}

function CenteredMessage({ text }: { text: string }) {
  return (
    <div className="min-h-screen bg-background flex items-center justify-center">
      <div className="text-sm text-muted-foreground">{text}</div>
    </div>
  )
}
