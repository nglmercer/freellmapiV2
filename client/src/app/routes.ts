import { lazy } from 'react'

/**
 * Single source of truth for lazily loaded pages. Each loader backs both
 * `lazy()` (rendering) and the nav's hover/focus preload, so a page that was
 * preloaded is never fetched twice.
 */
export const pageLoaders = {
  playground: () => import('@/pages/PlaygroundPage'),
  keys: () => import('@/pages/KeysPage'),
  fallback: () => import('@/pages/FallbackPage'),
  models: () => import('@/pages/ModelExplorerPage'),
  analytics: () => import('@/pages/AnalyticsPage'),
  providers: () => import('@/pages/ProvidersPage'),
  providerModels: () => import('@/pages/ProviderModelsPage'),
  setup: () => import('@/pages/SetupPage'),
} as const

export const pages = {
  playground: lazy(pageLoaders.playground),
  keys: lazy(pageLoaders.keys),
  fallback: lazy(pageLoaders.fallback),
  models: lazy(pageLoaders.models),
  analytics: lazy(pageLoaders.analytics),
  providers: lazy(pageLoaders.providers),
  providerModels: lazy(pageLoaders.providerModels),
  setup: lazy(pageLoaders.setup),
} as const
