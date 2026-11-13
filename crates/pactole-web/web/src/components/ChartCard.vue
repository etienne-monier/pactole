<script setup lang="ts">
import type { EChartsOption } from 'echarts'
import { BarChart, LineChart } from 'echarts/charts'
import { AriaComponent, GridComponent, LegendComponent, TooltipComponent } from 'echarts/components'
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { computed, ref } from 'vue'
import VChart from 'vue-echarts'
import { useI18n } from 'vue-i18n'

import MaskedValue from '@/components/MaskedValue.vue'
import { usePrivacyMode } from '@/composables/usePrivacyMode'
import { formatDecimal } from '@/i18n/format'
import type { ChartData } from '@/utils/charts'
import { hasChartData } from '@/utils/charts'

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
const { amountsHidden } = usePrivacyMode()
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
                <VBtnToggle v-model="mode" size="small" color="primary" variant="outlined" divided mandatory
                    selected-class="bg-primary">
                    <VBtn value="chart" rounded="0" icon="mdi-chart-line" :aria-label="t('common.chart')" />
                    <VBtn value="table" rounded="0" icon="mdi-table" :aria-label="t('common.table')" />
                </VBtnToggle>
            </template>
        </VCardItem>
        <VCardText>
            <VAlert v-if="!drawable" type="info" variant="tonal">{{ t('charts.empty') }}</VAlert>
            <VChart v-else-if="mode === 'chart'" :option="option ?? {}" autoresize class="chart"
                :class="{ 'chart--hidden': amountsHidden }" />
            <div v-else class="chart-table-wrapper">
                <VTable density="compact">
                    <thead>
                        <tr>
                            <th scope="col">{{ t('common.label') }}</th>
                            <th v-for="series in data?.series ?? []" :key="series.name" scope="col" class="text-right">
                                {{ series.name }}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr v-for="row in rows" :key="row.key">
                            <td>{{ row.category }}</td>
                            <td v-for="(value, column) in row.values"
                                :key="`${row.category}-${data?.series[column]?.name ?? column}`" class="text-right">
                                <span v-if="value === ''">{{ t('charts.noValue') }}</span>
                                <MaskedValue v-else :text="formatDecimal(value, locale)" />
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

.chart--hidden {
    border-radius: 4px;
    filter: blur(8px);
    user-select: none;
}

.chart--hidden:hover,
.chart--hidden:focus-within {
    filter: none;
}
</style>
