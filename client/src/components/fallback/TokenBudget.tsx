import { useTranslations } from '@/hooks/useTranslations'
import { formatTokens } from '@/lib/format'
import type { TokenUsageData } from './chain-model'

/** Platform colors for the budget bar. Unknown platforms fall back to slate. */
const PLATFORM_COLORS: Record<string, string> = {
  google: '#4285f4',
  groq: '#f55036',
  cerebras: '#8b5cf6',
  sambanova: '#14b8a6',
  nvidia: '#76b900',
  mistral: '#f59e0b',
  openrouter: '#ec4899',
  github: '#6e7b8b',
  cohere: '#d946ef',
  cloudflare: '#f38020',
  zhipu: '#06b6d4',
  ollama: '#000000',
  kilo: '#7c3aed',
  pollinations: '#a855f7',
  llm7: '#0ea5e9',
}
const FALLBACK_COLOR = '#94a3b8'

/** How the monthly token budget is split across models, and what is left. */
export function TokenBudget({ data }: { data: TokenUsageData }) {
  const { t } = useTranslations()
  const { totalBudget, totalUsed, models } = data
  const remaining = Math.max(0, totalBudget - totalUsed)
  const remainingPct = totalBudget > 0 ? Math.round((remaining / totalBudget) * 100) : 0
  const usedPct = totalBudget > 0 ? (totalUsed / totalBudget) * 100 : 0

  const segments = models.map((m) => {
    const share = totalBudget > 0 ? m.budget / totalBudget : 0
    return {
      ...m,
      color: PLATFORM_COLORS[m.platform] ?? FALLBACK_COLOR,
      remainingTokens: share * remaining,
      widthPct: share * (remaining / totalBudget) * 100,
    }
  })

  return (
    <section className="rounded-xl border bg-card p-5">
      <header className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium">{t('fallback.tokenBudget.title')}</h2>
        <p className="text-xs tabular-nums text-muted-foreground">
          <span className="font-medium text-foreground">
            {t('fallback.tokenBudget.remaining', { remaining: formatTokens(remaining) })}
          </span>
          {' · '}
          {t('fallback.tokenBudget.of', { remainingPct, total: formatTokens(totalBudget) })}
        </p>
      </header>

      <div className="flex h-2.5 overflow-hidden rounded-full bg-muted">
        {segments.map((m) => (
          <div
            key={`${m.platform}-${m.displayName}`}
            title={t('fallback.tokenBudget.segment', {
              name: m.displayName,
              platform: m.platform,
              tokens: formatTokens(m.remainingTokens),
            })}
            style={{ width: `${m.widthPct}%`, backgroundColor: m.color }}
          />
        ))}
        {totalUsed > 0 && (
          <div
            title={t('fallback.tokenBudget.usedTitle', { tokens: formatTokens(totalUsed) })}
            className="bg-muted-foreground/30"
            style={{ width: `${usedPct}%` }}
          />
        )}
      </div>

      <ul className="mt-4 grid grid-cols-1 gap-x-5 gap-y-1.5 text-xs tabular-nums sm:grid-cols-2 lg:grid-cols-3">
        {segments.map((m) => (
          <li key={`${m.platform}-${m.displayName}`} className="flex min-w-0 items-center gap-2">
            <span className="size-2 shrink-0 rounded-sm" style={{ backgroundColor: m.color }} aria-hidden />
            <span className="truncate">{m.displayName}</span>
            <span className="flex-1" />
            <span className="font-mono text-muted-foreground">{formatTokens(m.remainingTokens)}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}
