<script setup lang="ts">
import { useApiErrorMessage } from '@/composables/useApiErrorMessage'

const props = defineProps<{ error: unknown; showDetails?: boolean }>()

const message = useApiErrorMessage()

function details(value: unknown): string | null {
  if (typeof value !== 'object' || value === null) {
    return null
  }
  const candidate = (value as { details?: unknown }).details
  return candidate ? JSON.stringify(candidate, null, 2) : null
}
</script>

<template>
  <VAlert type="error" variant="tonal" class="mb-4">
    <div class="font-weight-medium">{{ message(props.error) }}</div>
    <pre v-if="props.showDetails && details(props.error)" class="error-details">{{
      details(props.error)
    }}</pre>
  </VAlert>
</template>

<style scoped>
.error-details {
  margin: 0.75rem 0 0;
  max-height: 16rem;
  overflow: auto;
  font-size: 0.8rem;
  white-space: pre-wrap;
}
</style>
