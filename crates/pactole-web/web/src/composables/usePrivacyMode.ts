import { ref } from 'vue'

const amountsHidden = ref(false)

/** Shared in-memory privacy state for monetary values. */
export function usePrivacyMode() {
  function toggleAmountsHidden(): void {
    amountsHidden.value = !amountsHidden.value
  }

  return { amountsHidden, toggleAmountsHidden }
}
