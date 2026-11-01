import { computed, ref } from 'vue'

import { fetchDiagnostics } from '@/api/client'
import type { Diagnostic } from '@/api/types'

const diagnostics = ref<Diagnostic[]>([])
const loading = ref(false)
let loaded = false

/** Shared diagnostic summary used by the navigation badges. */
export function useDiagnosticsSummary() {
  const errors = computed(() => diagnostics.value.filter((item) => item.severity === 'error').length)
  const warnings = computed(() =>
    diagnostics.value.filter((item) => item.severity === 'warning').length,
  )

  async function load(force = false): Promise<void> {
    if (loaded && !force) {
      return
    }
    loading.value = true
    try {
      const response = await fetchDiagnostics({ include_warnings: true })
      diagnostics.value = response.data.diagnostics
      loaded = true
    } catch {
      // A failing ledger is reported by the views; the summary stays empty.
      loaded = true
    } finally {
      loading.value = false
    }
  }

  return { diagnostics, errors, warnings, loading, load }
}
