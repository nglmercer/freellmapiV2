import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@/components/ui/button'
import { ApiError, createAccount, joinWithCode, signInWithPassword } from '@/lib/api'
import { useTranslations } from '@/hooks/useTranslations'

type AuthMode = 'password' | 'join'

export function AuthPanel({ hasAccount }: { hasAccount: boolean }) {
  const { t } = useTranslations()
  const queryClient = useQueryClient()
  // A join link (?join=CODE) is read once and removed from the address bar.
  const [linkedCode] = useState(
    () => new URLSearchParams(window.location.search).get('join')?.trim() ?? '',
  )
  const [mode, setMode] = useState<AuthMode>(linkedCode && hasAccount ? 'join' : 'password')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [code, setCode] = useState(linkedCode)
  const [error, setError] = useState('')
  const [pending, setPending] = useState(false)
  const linkHandled = useRef(false)

  useEffect(() => {
    const url = new URL(window.location.href)
    if (url.searchParams.has('join')) {
      url.searchParams.delete('join')
      window.history.replaceState(window.history.state, '', url.pathname + url.search + url.hash)
    }
    // StrictMode runs effects twice in development; submit the link only once.
    if (linkedCode && hasAccount && !linkHandled.current) {
      linkHandled.current = true
      void runJoin(linkedCode)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function describe(err: unknown, fallback: string): string {
    if (err instanceof ApiError) return err.message
    if (err instanceof TypeError) return t('auth.unreachable')
    return fallback
  }

  async function runJoin(value: string) {
    setPending(true)
    setError('')
    try {
      await joinWithCode(value)
      setCode('')
      await queryClient.invalidateQueries()
    } catch (err) {
      setError(err instanceof ApiError && err.status === 401 ? t('auth.invalidCode') : describe(err, t('auth.failed')))
    } finally {
      setPending(false)
    }
  }

  async function submitPassword(event: React.FormEvent) {
    event.preventDefault()
    if (!hasAccount && password !== confirm) {
      setError(t('auth.passwordMismatch'))
      return
    }
    setPending(true)
    setError('')
    try {
      if (hasAccount) {
        await signInWithPassword({ username: username.trim(), password })
      } else {
        await createAccount({ username: username.trim(), password })
      }
      setPassword('')
      setConfirm('')
      await queryClient.invalidateQueries()
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 401
          ? t('auth.invalidCredentials')
          : describe(err, t('auth.failed')),
      )
    } finally {
      setPending(false)
    }
  }

  function submitJoin(event: React.FormEvent) {
    event.preventDefault()
    const trimmed = code.trim()
    if (trimmed) void runJoin(trimmed)
  }

  const inputClass =
    'flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm outline-none focus-visible:ring-1 focus-visible:ring-ring'
  const tabClass = (active: boolean) =>
    `text-sm px-1 pb-1 border-b-2 ${active ? 'border-foreground text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'}`

  return (
    <div className="min-h-screen bg-background flex items-center justify-center px-6">
      <div className="w-full max-w-md rounded-lg border bg-card p-6 space-y-5">
        <div>
          <div className="flex items-center gap-2 mb-3">
            <span className="inline-block size-2 rounded-full bg-foreground" />
            <span className="font-semibold tracking-tight text-sm">FreeLLMAPI</span>
          </div>
          <h1 className="text-lg font-semibold tracking-tight">
            {!hasAccount ? t('auth.createTitle') : t('auth.signInTitle')}
          </h1>
          <p className="text-sm text-muted-foreground mt-2">
            {!hasAccount ? t('auth.createDescription') : t('auth.signInDescription')}
          </p>
        </div>

        {hasAccount && (
          <div role="tablist" className="flex gap-4 border-b">
            <button type="button" role="tab" aria-selected={mode === 'password'} className={tabClass(mode === 'password')} onClick={() => { setMode('password'); setError('') }}>
              {t('auth.tabPassword')}
            </button>
            <button type="button" role="tab" aria-selected={mode === 'join'} className={tabClass(mode === 'join')} onClick={() => { setMode('join'); setError('') }}>
              {t('auth.tabJoin')}
            </button>
          </div>
        )}

        {!hasAccount && linkedCode && (
          <p className="text-sm text-muted-foreground">{t('auth.joinNeedsAccount')}</p>
        )}

        {mode === 'password' ? (
          <form onSubmit={submitPassword} className="space-y-5">
            <div className="space-y-1.5">
              <label htmlFor="auth-username" className="text-xs font-medium">{t('auth.username')}</label>
              <input
                id="auth-username"
                value={username}
                onChange={event => setUsername(event.target.value)}
                autoComplete="username"
                autoFocus
                className={inputClass}
              />
              {!hasAccount && <p className="text-xs text-muted-foreground">{t('auth.usernameHint')}</p>}
            </div>
            <div className="space-y-1.5">
              <label htmlFor="auth-password" className="text-xs font-medium">{t('auth.password')}</label>
              <input
                id="auth-password"
                type="password"
                value={password}
                onChange={event => setPassword(event.target.value)}
                autoComplete={hasAccount ? 'current-password' : 'new-password'}
                className={inputClass}
              />
              {!hasAccount && <p className="text-xs text-muted-foreground">{t('auth.passwordHint')}</p>}
            </div>
            {!hasAccount && (
              <div className="space-y-1.5">
                <label htmlFor="auth-confirm" className="text-xs font-medium">{t('auth.confirmPassword')}</label>
                <input
                  id="auth-confirm"
                  type="password"
                  value={confirm}
                  onChange={event => setConfirm(event.target.value)}
                  autoComplete="new-password"
                  className={inputClass}
                />
              </div>
            )}
            {error && <p className="text-sm text-destructive">{error}</p>}
            <Button
              type="submit"
              className="w-full"
              disabled={pending || !username.trim() || !password || (!hasAccount && !confirm)}
            >
              {hasAccount ? t('auth.signInButton') : t('auth.createButton')}
            </Button>
          </form>
        ) : (
          <form onSubmit={submitJoin} className="space-y-5">
            <div className="space-y-1.5">
              <label htmlFor="auth-code" className="text-xs font-medium">{t('auth.joinCode')}</label>
              <input
                id="auth-code"
                value={code}
                onChange={event => setCode(event.target.value)}
                placeholder="XXXX-XXXX-XXXX-XXXX"
                autoComplete="off"
                autoFocus
                spellCheck={false}
                className={`${inputClass} font-mono tracking-wider`}
              />
              <p className="text-xs text-muted-foreground">{t('auth.joinHint')}</p>
            </div>
            {error && <p className="text-sm text-destructive">{error}</p>}
            <Button type="submit" className="w-full" disabled={pending || !code.trim()}>
              {t('auth.joinButton')}
            </Button>
          </form>
        )}
      </div>
    </div>
  )
}
