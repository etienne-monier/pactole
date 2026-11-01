<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <div class="loading-block">
    <VProgressCircular indeterminate color="primary" size="32" />
    <span>{{ t('common.loading') }}</span>
  </div>
</template>

<style scoped>
.loading-block {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1.5rem 0;
  opacity: 0.8;
}
</style>
