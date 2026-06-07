import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

const saved =
  typeof window !== 'undefined' ? localStorage.getItem('i18n_lng') : null
const initialLng = (saved ?? 'en') as 'en' | 'es'

async function loadLocale(lng: 'en' | 'es') {
  const mod = await import(`./locales/${lng}/translation.json`)

  return {
    translation: mod.default ?? mod,
  } as const
}

void i18n.use(initReactI18next).init({
  lng: initialLng,
  fallbackLng: 'en',
  supportedLngs: ['en', 'es'],
  resources: {},
  interpolation: { escapeValue: false },
})

loadLocale(initialLng).then((resources) => {
  i18n.addResourceBundle(initialLng, 'translation', resources.translation, true, true)
})

export function switchLanguage(lng: 'en' | 'es') {
  i18n.changeLanguage(lng)

  if (lng !== initialLng) {
    loadLocale(lng).then((resources) => {
      i18n.addResourceBundle(lng, 'translation', resources.translation, true, true)
    })
  }

  localStorage.setItem('i18n_lng', lng)
}

export function getCurrentLanguage() {
  return (i18n.language ?? 'en') as 'en' | 'es'
}

export default i18n
