<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineEmits<{ apply: []; reset: [] }>()
</script>

<template>
  <VCard class="filter-card">
    <VCardText>
      <div class="filter-grid">
        <slot />
      </div>
      <div class="filter-actions">
        <slot name="actions">
          <VBtn variant="text" prepend-icon="mdi-filter-remove-outline" @click="$emit('reset')">
            {{ t('common.reset') }}
          </VBtn>
          <VBtn color="primary" prepend-icon="mdi-filter-check-outline" @click="$emit('apply')">
            {{ t('common.apply') }}
          </VBtn>
        </slot>
      </div>
    </VCardText>
  </VCard>
</template>

<style scoped>
.filter-card {
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
}
.filter-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
  gap: 12px;
  align-items: start;
}
.filter-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}
</style>
