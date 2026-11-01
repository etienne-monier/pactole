<script setup lang="ts">
import type { Amount } from '@/api/types'
import AmountText from '@/components/AmountText.vue'

defineProps<{ label: string; amounts: readonly Amount[]; hint?: string }>()
</script>

<template>
  <VCard class="stat-card">
    <VCardText>
      <div class="stat-label">{{ label }}</div>
      <div class="stat-value"><AmountText :amounts="amounts" /></div>
      <div v-if="hint" class="stat-hint">{{ hint }}</div>
    </VCardText>
  </VCard>
</template>

<style scoped>
.stat-card {
  height: 100%;
}
.stat-label {
  font-size: 0.85rem;
  font-weight: 500;
  letter-spacing: 0.02em;
  opacity: 0.75;
  text-transform: uppercase;
}
.stat-value {
  margin-top: 0.35rem;
  font-size: 1.4rem;
  font-weight: 600;
}
.stat-hint {
  margin-top: 0.25rem;
  font-size: 0.8rem;
  opacity: 0.7;
}
</style>
