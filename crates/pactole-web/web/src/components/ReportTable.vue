<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { Report } from '@/api/types'
import AmountText from '@/components/AmountText.vue'

defineProps<{ report: Report; showTotal?: boolean }>()

const { t } = useI18n()
</script>

<template>
  <VTable density="compact" class="report-table">
    <thead>
      <tr>
        <th scope="col">{{ t('common.label') }}</th>
        <th scope="col" class="text-right">{{ t('common.amount') }}</th>
      </tr>
    </thead>
    <tbody>
      <template v-for="(section, sectionIndex) in report.sections" :key="sectionIndex">
        <tr v-if="section.title" class="section-row">
          <td colspan="2"><strong>{{ section.title }}</strong></td>
        </tr>
        <tr v-for="(line, lineIndex) in section.lines" :key="`${sectionIndex}-${lineIndex}`">
          <td :style="{ paddingLeft: `${16 + line.depth * 16}px` }">{{ line.label }}</td>
          <td class="text-right"><AmountText :amounts="line.amounts" /></td>
        </tr>
        <tr v-if="section.title" class="subtotal-row">
          <td class="text-right">{{ t('common.subtotal') }}</td>
          <td class="text-right"><AmountText :amounts="section.subtotal" /></td>
        </tr>
      </template>
    </tbody>
    <tfoot v-if="showTotal !== false && report.totals.length > 0">
      <tr class="total-row">
        <td class="text-right">{{ report.total_label }}</td>
        <td class="text-right"><AmountText :amounts="report.totals" /></td>
      </tr>
    </tfoot>
  </VTable>
</template>

<style scoped>
.section-row td {
  background: rgba(var(--v-theme-on-surface), 0.04);
}
.subtotal-row td,
.total-row td {
  font-weight: 600;
}
.total-row td {
  border-top: 2px solid rgba(var(--v-theme-on-surface), 0.2);
}
</style>
