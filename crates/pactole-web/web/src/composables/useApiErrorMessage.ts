import { useI18n } from 'vue-i18n'

import { ApiError, toApiError } from '@/api/client'

/** Returns a translator for structured API errors, keyed by their stable code. */
export function useApiErrorMessage() {
  const { t, te } = useI18n()

  return (error: unknown): string => {
    const apiError = error instanceof ApiError ? error : toApiError(error)
    const key = `errors.${apiError.code}`
    const params = { message: apiError.message }
    return te(key) ? t(key, params) : t('errors.fallback', params)
  }
}
