import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './locales/en/translation.json' with { type: 'json' }
import es from './locales/es/translation.json' with { type: 'json' }

const saved = typeof window !== 'undefined' ? localStorage.getItem('i18n_lng') : null

void i18n.use(initReactI18next).init({
  lng: saved ?? 'en',
  fallbackLng: 'en',
  supportedLngs: ['en', 'es'],
  resources: {
    en: { translation: en },
    es: { translation: es },
  },
  interpolation: { escapeValue: false },
})

export function switchLanguage(lng: 'en' | 'es') {
  i18n.changeLanguage(lng)
  localStorage.setItem('i18n_lng', lng)
}

export function getCurrentLanguage() {
  return (i18n.language ?? 'en') as 'en' | 'es'
}

export default i18n
