import { Component, type ReactNode } from 'react'
import { useTranslations } from '@/hooks/useTranslations'
import { Button } from '@/components/ui/button'

interface Props {
  children: ReactNode
}

interface State {
  failed: boolean
}

/**
 * Catches failures while loading or rendering a lazily loaded page. The usual
 * cause is a stale tab: after a deploy the old chunk hashes no longer exist.
 * Remount it per route (via `key`) so navigating elsewhere retries.
 */
export class RouteErrorBoundary extends Component<Props, State> {
  state: State = { failed: false }

  static getDerivedStateFromError(): State {
    return { failed: true }
  }

  componentDidCatch(error: unknown) {
    console.error('Page failed to load', error)
  }

  render() {
    return this.state.failed ? <LoadFailed /> : this.props.children
  }
}

function LoadFailed() {
  const { t } = useTranslations()
  return (
    <div className="flex flex-col items-center justify-center gap-4 h-[60vh] text-center">
      <p className="text-sm text-muted-foreground max-w-sm">{t('app.pageLoadError')}</p>
      <Button variant="outline" onClick={() => window.location.reload()}>
        {t('app.reload')}
      </Button>
    </div>
  )
}
