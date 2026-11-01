import { onScopeDispose, ref, shallowRef } from 'vue'

import { isAbortError, toApiError, type ApiError } from '@/api/client'

/**
 * Runs an async loader, cancelling the previous run when a new one starts.
 *
 * Aborted runs never surface an error and never overwrite newer data.
 */
export function useAsync<T>(loader: (signal: AbortSignal) => Promise<T>) {
  const data = shallowRef<T | null>(null)
  const error = shallowRef<ApiError | null>(null)
  const loading = ref(false)
  let controller: AbortController | null = null

  async function run(): Promise<void> {
    controller?.abort()
    const current = new AbortController()
    controller = current
    loading.value = true
    error.value = null
    try {
      const result = await loader(current.signal)
      if (!current.signal.aborted) {
        data.value = result
      }
    } catch (cause) {
      if (!isAbortError(cause) && !current.signal.aborted) {
        error.value = toApiError(cause)
      }
    } finally {
      if (!current.signal.aborted) {
        loading.value = false
      }
    }
  }

  onScopeDispose(() => controller?.abort())

  return { data, error, loading, run }
}
