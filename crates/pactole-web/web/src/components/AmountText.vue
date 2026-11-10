<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import type { Amount } from '@/api/types'
import { formatDecimal } from '@/i18n/format'

const props = defineProps<{ amounts: readonly Amount[] }>()

const { locale } = useI18n()

const parts = computed(() =>
  props.amounts.map((amount) => ({
    commodity: amount.commodity,
    text: formatDecimal(amount.number, locale.value, amount.commodity),
  })),
)
</script>

<template>
  <span class="amount-text">
    <template v-for="(part, index) in parts" :key="`${part.commodity}-${index}`">
      <span v-if="index > 0" class="amount-separator"> · </span>
      <span class="amount-number">{{ part.text }}</span>
      <span class="amount-commodity">{{ part.commodity }}</span>
    </template>
    <span v-if="parts.length === 0" class="amount-empty">—</span>
  </span>
</template>

<style scoped>
.amount-text {
  white-space: nowrap;
}
.amount-commodity {
  margin-left: 0.25em;
  opacity: 0.7;
  font-size: 0.85em;
}
.amount-empty {
  opacity: 0.5;
}
</style>
