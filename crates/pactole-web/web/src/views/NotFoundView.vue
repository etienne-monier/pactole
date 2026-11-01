<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <div class="not-found">
    <VIcon icon="mdi-map-marker-question-outline" size="56" color="primary" />
    <h1 class="text-h5">{{ t('errors.not-found') }}</h1>
    <VBtn :to="{ name: 'dashboard' }" color="primary" variant="tonal">
      {{ t('nav.dashboard') }}
    </VBtn>
  </div>
</template>

<style scoped>
.not-found {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 64px 16px;
  text-align: center;
}
</style>
