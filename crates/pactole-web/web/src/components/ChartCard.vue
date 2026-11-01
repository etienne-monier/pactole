<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { EChartsOption } from 'echarts'
import { use } from 'echarts/core'
import { BarChart, LineChart } from 'echarts/charts'
import { AriaComponent, GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import VChart from 'vue-echarts'

import type { ChartData } from '@/utils/charts'
import { hasChartData } from '@/utils/charts'
import { formatDecimal } from '@/i18n/format'

use([
  BarChart,
  LineChart,
  AriaComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  CanvasRenderer,
])

const props = defineProps<{
  title: string
  hint?: string
  option: EChartsOption | null
  data: ChartData | null
}>()

const { t, locale } = useI18n()
const mode = ref<'chart' | 'table'>('chart')

const drawable = computed(() => hasChartData(props.data))
const rows = computed(() => {
  const occurrences = new Map<string, number>()
  return (props.data?.categories ?? []).map((category, index) => {
    const base = `${category}:${props.data?.series.map((series) => series.name).join('|')}`
    const occurrence = occurrences.get(base) ?? 0
    occurrences.set(base, occurrence + 1)
    return {
      key: `${base}:${occurrence}`,
      category,
      values: (props.data?.series ?? []).map((series) => series.exact[index] ?? ''),
    }
  })
})
</script>

<template>
  <VCard class="chart-card">
    <VCardItem>
      <VCardTitle>{{ title }}</VCardTitle>
      <VCardSubtitle v-if="hint">{{ hint }}</VCardSubtitle>
      <template #append>
        <VBtnToggle v-model="mode" density="compact" variant="text" mandatory>
          <VBtn value="chart" icon="mdi-chart-line" size="small" :aria-label="t('common.chart')" />
          <VBtn value="table" icon="mdi-table" size="small" :aria-label="t('common.table')" />
        </VBtnToggle>
      </template>
    </VCardItem>
    <VCardText>
      <VAlert v-if="!drawable" type="info" variant="tonal">{{ t('charts.empty') }}</VAlert>
      <VChart v-else-if="mode === 'chart'" :option="option ?? {}" autoresize class="chart" />
      <div v-else class="chart-table-wrapper">
        <VTable density="compact">
          <thead>
            <tr>
              <th scope="col">{{ t('common.label') }}</th>
              <th
                v-for="series in data?.series ?? []"
                :key="series.name"
                scope="col"
                class="text-right"
              >
                {{ series.name }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in rows" :key="row.key">
              <td>{{ row.category }}</td>
                <td v-for="(value, column) in row.values" :key="`${row.category}-${data?.series[column]?.name ?? column}`" class="text-right">
                {{ value === '' ? t('charts.noValue') : formatDecimal(value, locale) }}
              </td>
            </tr>
          </tbody>
        </VTable>
      </div>
    </VCardText>
  </VCard>
</template>

<style scoped>
.chart {
  height: 320px;
}
.chart-table-wrapper {
  overflow-x: auto;
}
</style>
