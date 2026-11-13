<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { usePrivacyMode } from '@/composables/usePrivacyMode'

defineProps<{ text: string }>()

const { t } = useI18n()
const { amountsHidden } = usePrivacyMode()
</script>

<template>
  <span
    class="masked-value"
    :class="{ 'masked-value--hidden': amountsHidden }"
    :tabindex="amountsHidden ? 0 : undefined"
    :aria-label="amountsHidden ? t('common.hiddenAmount') : undefined"
  >
    <span :aria-hidden="amountsHidden">{{ text }}</span>
  </span>
</template>

<style scoped>
.masked-value {
  display: inline-block;
  transition: filter 120ms ease;
}

.masked-value--hidden {
  border-radius: 3px;
  filter: blur(0.35em);
  user-select: none;
}

.masked-value--hidden:hover,
.masked-value--hidden:focus {
  filter: none;
  outline: 2px solid rgba(var(--v-theme-primary), 0.45);
  outline-offset: 2px;
}
</style>
