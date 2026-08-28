import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

const saved =
  typeof window !== 'undefined' ? localStorage.getItem('i18n_lng') : null
const initialLng: 'en' | 'es' = saved === 'es' ? 'es' : 'en'

async function loadLocale(lng: 'en' | 'es') {
  const mod = await import(`./locales/${lng}/translation.json`)

  return {
    translation: mod.default ?? mod,
  } as const
}

const initialized = i18n.use(initReactI18next).init({
  lng: initialLng,
  fallbackLng: 'en',
  supportedLngs: ['en', 'es'],
  resources: {},
  interpolation: { escapeValue: false },
})

/**
 * The application must not render before the selected locale is registered.
 * Otherwise the first paint contains raw keys (for example `adminAuth.title`)
 * while the dynamic JSON chunk is still loading.
 */
export const i18nReady = initialized.then(() => loadLocale(initialLng)).then((resources) => {
  i18n.addResourceBundle(initialLng, 'translation', resources.translation, true, true)
})

export async function switchLanguage(lng: 'en' | 'es') {
  if (!i18n.hasResourceBundle(lng, 'translation')) {
    const resources = await loadLocale(lng)
    i18n.addResourceBundle(lng, 'translation', resources.translation, true, true)
  }

  await i18n.changeLanguage(lng)
  localStorage.setItem('i18n_lng', lng)
}

export function getCurrentLanguage() {
  return (i18n.language ?? 'en') as 'en' | 'es'
}

export default i18n
