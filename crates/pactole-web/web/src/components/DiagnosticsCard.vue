<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { Diagnostic } from '@/api/types'
import DiagnosticsTable from '@/components/DiagnosticsTable.vue'

defineProps<{ diagnostics: readonly Diagnostic[] }>()

const { t } = useI18n()
</script>

<template>
  <VCard v-if="diagnostics.length > 0" class="mb-4">
    <VCardTitle>{{ t('common.diagnostics') }}</VCardTitle>
    <VCardText>
      <DiagnosticsTable :diagnostics="diagnostics" />
    </VCardText>
  </VCard>
</template>
