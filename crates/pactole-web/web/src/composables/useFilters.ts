import { computed, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

/**
 * Keeps a set of string filters in sync with the URL query string.
 *
 * `filters` is the editable draft. `applied` is the snapshot used by the views
 * to build their API request, so changing a field does not trigger a request
 * until `apply()` is called. Values equal to their default are omitted from
 * the URL.
 */
export function useFilters(defaults: Record<string, string>) {
  const route = useRoute()
  const router = useRouter()

  const filters = reactive<Record<string, string>>({ ...defaults })
  for (const key of Object.keys(defaults)) {
    const value = route.query[key]
    if (typeof value === 'string' && value !== '') {
      filters[key] = value
    }
  }

  const applied = ref<Record<string, string>>({ ...filters })

  const query = computed(() => {
    const result: Record<string, string> = {}
    for (const key of Object.keys(defaults)) {
      const value = applied.value[key] ?? defaults[key]
      if (value !== '' && value !== defaults[key]) {
        result[key] = value
      }
    }
    return result
  })

  watch(
    query,
    (value) => {
      void router.replace({ query: value })
    },
    { deep: true },
  )

  watch(
    () => route.query,
    (value) => {
      const next: Record<string, string> = { ...defaults }
      for (const key of Object.keys(defaults)) {
        const queryValue = value[key]
        if (typeof queryValue === 'string' && queryValue !== '') {
          next[key] = queryValue
        }
      }
      if (JSON.stringify(next) !== JSON.stringify(applied.value)) {
        Object.assign(filters, next)
        applied.value = next
      }
    },
    { deep: true },
  )

  function apply(): void {
    applied.value = { ...filters }
  }

  function reset(): void {
    for (const key of Object.keys(defaults)) {
      filters[key] = defaults[key]
    }
    applied.value = { ...defaults }
  }

  return { filters, applied, query, apply, reset }
}
