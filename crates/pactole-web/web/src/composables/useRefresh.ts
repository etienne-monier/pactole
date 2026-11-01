import { ref } from 'vue'

const refreshKey = ref(0)

/** Global refresh signal shared by every view. */
export function useRefresh() {
  function requestRefresh(): void {
    refreshKey.value += 1
  }

  return { refreshKey, requestRefresh }
}
