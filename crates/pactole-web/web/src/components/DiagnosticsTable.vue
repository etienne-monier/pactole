<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import type { Diagnostic } from '@/api/types'

const props = defineProps<{ diagnostics: readonly Diagnostic[] }>()

const { t } = useI18n()

const keyedDiagnostics = computed(() => {
  const occurrences = new Map<string, number>()
  return props.diagnostics.map((diagnostic) => {
    const base = `${location(diagnostic)}:${diagnostic.code}:${diagnostic.message}`
    const occurrence = occurrences.get(base) ?? 0
    occurrences.set(base, occurrence + 1)
    return { diagnostic, key: `${base}:${occurrence}` }
  })
})

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
    try {
      await navigator.clipboard?.writeText(value)
    } catch {
      // Clipboard permissions are browser-controlled; the location remains visible.
    }
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
        <tr v-for="entry in keyedDiagnostics" :key="entry.key">
          <td>
            <VChip
              size="x-small"
              :color="entry.diagnostic.severity === 'error' ? 'error' : 'warning'"
              variant="tonal"
            >
              {{ t(`severities.${entry.diagnostic.severity}`) }}
            </VChip>
          </td>
          <td>{{ t(`origins.${entry.diagnostic.origin}`) }}</td>
          <td><code>{{ entry.diagnostic.code }}</code></td>
          <td class="message">{{ entry.diagnostic.message }}</td>
          <td class="source">
            <VBtn v-if="entry.diagnostic.source" size="small" variant="text" @click="copyLocation(entry.diagnostic)">
              {{ location(entry.diagnostic) }}
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
