<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { Diagnostic } from '@/api/types'

defineProps<{ diagnostics: readonly Diagnostic[] }>()

const { t } = useI18n()

function location(diagnostic: Diagnostic): string {
  if (!diagnostic.source) {
    return '—'
  }
  if (!diagnostic.span) {
    return diagnostic.source
  }
  return `${diagnostic.source}:${diagnostic.span.start.line}:${diagnostic.span.start.column}`
}

async function copyLocation(diagnostic: Diagnostic): Promise<void> {
  const value = location(diagnostic)
  if (value !== '—') {
    await navigator.clipboard?.writeText(value)
  }
}
</script>

<template>
  <div class="diagnostics-table-wrapper">
    <VTable density="compact" class="diagnostics-table">
      <thead>
        <tr>
          <th scope="col">{{ t('diagnostics.severity') }}</th>
          <th scope="col">{{ t('diagnostics.origin') }}</th>
          <th scope="col">{{ t('diagnostics.code') }}</th>
          <th scope="col">{{ t('diagnostics.message') }}</th>
          <th scope="col">{{ t('diagnostics.source') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="diagnostic in diagnostics" :key="`${location(diagnostic)}:${diagnostic.code}:${diagnostic.message}`">
          <td>
            <VChip
              size="x-small"
              :color="diagnostic.severity === 'error' ? 'error' : 'warning'"
              variant="tonal"
            >
              {{ t(`severities.${diagnostic.severity}`) }}
            </VChip>
          </td>
          <td>{{ t(`origins.${diagnostic.origin}`) }}</td>
          <td><code>{{ diagnostic.code }}</code></td>
          <td class="message">{{ diagnostic.message }}</td>
          <td class="source">
            <VBtn v-if="diagnostic.source" size="small" variant="text" @click="copyLocation(diagnostic)">
              {{ location(diagnostic) }}
            </VBtn>
            <span v-else>—</span>
          </td>
        </tr>
      </tbody>
    </VTable>
  </div>
</template>

<style scoped>
.diagnostics-table-wrapper {
  overflow-x: auto;
}
.diagnostics-table {
  min-width: 720px;
}
.message {
  min-width: 240px;
}
.source {
  white-space: nowrap;
  opacity: 0.8;
}
</style>
