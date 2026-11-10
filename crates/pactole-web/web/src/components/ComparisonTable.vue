<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import type { ComparisonReport } from '@/api/types'
import { formatDecimal, formatPercent } from '@/i18n/format'
import { translateReportLabel } from '@/i18n'

const props = defineProps<{ comparison: ComparisonReport }>()

const { t, locale } = useI18n()

const percent = computed(() => props.comparison.mode === 'percent')

function renderAmount(value: string, commodity: string): string {
  return formatDecimal(value, locale.value, commodity)
}

function renderDifference(value: string | null, commodity: string): string {
  if (value === null) {
    return '—'
  }
  return percent.value
    ? formatPercent(value, locale.value)
    : formatDecimal(value, locale.value, commodity)
}
</script>

<template>
  <VTable density="compact" class="comparison-table">
    <thead>
      <tr>
        <th scope="col">{{ t('common.label') }}</th>
        <th scope="col">{{ t('common.commodity') }}</th>
        <th scope="col" class="text-right">{{ t('expenses.current') }}</th>
        <th scope="col" class="text-right">{{ t('expenses.previous') }}</th>
        <th scope="col" class="text-right">{{ t('expenses.difference') }}</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="(line, index) in comparison.lines" :key="index">
        <td :style="{ paddingLeft: `${16 + line.depth * 16}px` }">
          <span v-if="line.section" class="section-label">{{ translateReportLabel(t, line.section) }} · </span>{{ line.label }}
        </td>
        <td>{{ line.commodity }}</td>
        <td class="text-right">{{ renderAmount(line.current, line.commodity) }}</td>
        <td class="text-right">{{ renderAmount(line.previous, line.commodity) }}</td>
        <td class="text-right">{{ renderDifference(line.difference, line.commodity) }}</td>
      </tr>
    </tbody>
    <tfoot v-if="comparison.totals.length > 0">
      <tr v-for="(total, index) in comparison.totals" :key="`total-${index}`" class="total-row">
        <td class="text-right" colspan="2">{{ t('common.total') }}</td>
        <td class="text-right">{{ renderAmount(total.current, total.commodity) }}</td>
        <td class="text-right">{{ renderAmount(total.previous, total.commodity) }}</td>
        <td class="text-right">{{ renderDifference(total.difference, total.commodity) }}</td>
      </tr>
    </tfoot>
  </VTable>
</template>

<style scoped>
.section-label {
  opacity: 0.7;
}
.total-row td {
  font-weight: 600;
  border-top: 2px solid rgba(var(--v-theme-on-surface), 0.2);
}
</style>
