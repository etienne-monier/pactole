<script setup lang="ts">
defineProps<{ title: string; subtitle?: string }>()
</script>

<template>
  <header class="page-header">
    <div>
      <h1 class="text-h5">{{ title }}</h1>
      <p v-if="subtitle" class="text-body-2 text-medium-emphasis">{{ subtitle }}</p>
    </div>
    <div class="page-header-actions">
      <slot name="actions" />
    </div>
  </header>
</template>

<style scoped>
.page-header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
}
.page-header h1 {
  margin: 0;
}
.page-header p {
  margin: 0.25rem 0 0;
}
.page-header-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}
</style>
