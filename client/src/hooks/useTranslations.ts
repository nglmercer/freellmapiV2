import { useTranslation } from 'react-i18next'

export function useTranslations() {
  const { t } = useTranslation()
  return { t }
}
