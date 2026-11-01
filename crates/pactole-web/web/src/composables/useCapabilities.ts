import { ref } from 'vue'

import { fetchCapabilities } from '@/api/client'
import type { Capabilities } from '@/api/types'

const capabilities = ref<Capabilities | null>(null)
const loading = ref(false)
const error = ref<unknown>(null)
let loaded = false

/** Shared backend capabilities, loaded once per page. */
export function useCapabilities() {
  async function load(force = false): Promise<void> {
    if (loaded && !force) {
      return
    }
    loading.value = true
    error.value = null
    try {
      const response = await fetchCapabilities()
      capabilities.value = response.capabilities
      loaded = true
    } catch (cause) {
      error.value = cause
    } finally {
      loading.value = false
    }
  }

  return { capabilities, loading, error, load }
}
