<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchBalance } from '@/api/client'
import type { BalanceParams } from '@/api/types'
import ChartCard from '@/components/ChartCard.vue'
import DateField from '@/components/DateField.vue'
import DiagnosticsCard from '@/components/DiagnosticsCard.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import FilterCard from '@/components/FilterCard.vue'
import LoadingBlock from '@/components/LoadingBlock.vue'
import PageHeader from '@/components/PageHeader.vue'
import ReportTable from '@/components/ReportTable.vue'
import ValuationDetails from '@/components/ValuationDetails.vue'
import { useAsync } from '@/composables/useAsync'
import { useFilters } from '@/composables/useFilters'
import { useRefresh } from '@/composables/useRefresh'
import { translateReportLabel } from '@/i18n'
import { formatDecimal } from '@/i18n/format'
import { buildBarOption, rowsToChartData, sectionRows } from '@/utils/charts'
import { todayIso } from '@/utils/dates'
import { optional, isTrue } from '@/utils/params'
import { scopeText } from '@/utils/report'

const { t, locale } = useI18n()
const { refreshKey } = useRefresh()

const { filters, applied, apply, reset } = useFilters({
  as_of: todayIso(),
  depth: '',
  convert_to: '',
  at: '',
  include_warnings: 'false',
})

const includeWarnings = computed({
  get: () => isTrue(filters.include_warnings),
  set: (value: boolean) => {
    filters.include_warnings = value ? 'true' : 'false'
  },
})

const params = computed<BalanceParams>(() => {
  const value = applied.value
  const result: BalanceParams = {
    as_of: optional(value.as_of),
    depth: optional(value.depth),
  }
  const convertTo = optional(value.convert_to)
  if (convertTo) {
    result.convert_to = convertTo
    const at = optional(value.at)
    if (at) {
      result.at = at
    }
  }
  if (isTrue(value.include_warnings)) {
    result.include_warnings = true
  }
  return result
})

const { data, error, loading, run } = useAsync((signal) =>
  fetchBalance(params.value, { signal }),
)

watch([params, refreshKey], () => void run(), { immediate: true })

const report = computed(() => data.value?.data ?? null)
const diagnostics = computed(() => data.value?.diagnostics ?? [])
const chartData = computed(() =>
  report.value ? rowsToChartData(sectionRows(report.value)) : null,
)
const chartOption = computed(() =>
  chartData.value
    ? buildBarOption(chartData.value, { format: (value) => formatDecimal(value, locale.value) })
    : null,
)
</script>

<template>
  <div class="page-stack">
    <PageHeader :title="t('balance.title')" :subtitle="t('balance.subtitle')" />

    <FilterCard @apply="apply" @reset="reset">
      <DateField v-model="filters.as_of" :label="t('common.asOf')" />
      <VTextField v-model="filters.depth" type="number" min="1" step="1" :label="t('common.depth')" />
      <VTextField v-model="filters.convert_to" :label="t('common.currency')" />
      <DateField
        v-model="filters.at"
        :label="t('common.valuationDate')"
        :disabled="!filters.convert_to"
      />
      <VSwitch v-model="includeWarnings" :label="t('common.showWarnings')" hide-details />
    </FilterCard>

    <ErrorAlert v-if="error" :error="error" />
    <LoadingBlock v-if="loading && !report" />

    <template v-if="report">
      <VCard>
        <VCardItem>
          <VCardTitle>{{ translateReportLabel(t, report.title) }}</VCardTitle>
          <VCardSubtitle>{{ scopeText(report.scope, locale, t('common.asOf')) }}</VCardSubtitle>
        </VCardItem>
        <VCardText>
          <ValuationDetails :valuation="report.valuation" />
          <ReportTable :report="report" />
        </VCardText>
      </VCard>

      <ChartCard
        :title="t('balance.composition')"
        :hint="t('balance.compositionHint')"
        :option="chartOption"
        :data="chartData"
      />

      <DiagnosticsCard :diagnostics="diagnostics" />
    </template>
  </div>
</template>
