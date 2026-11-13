<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchIncome } from '@/api/client'
import type { IncomeParams } from '@/api/types'
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
import { dateRangeError, todayIso, yearStartIso } from '@/utils/dates'
import { isTrue, optional } from '@/utils/params'
import { scopeText } from '@/utils/report'

const { t, locale } = useI18n()
const { refreshKey } = useRefresh()

const { filters, applied, apply, reset } = useFilters({
  from: yearStartIso(),
  to: todayIso(),
  by: 'account',
  depth: '',
  convert_to: '',
  at: '',
  include_warnings: 'false',
})

const groupingItems = computed(() => [
  { title: t('common.account'), value: 'account' },
  { title: t('common.payee'), value: 'payee' },
  { title: t('common.tag'), value: 'tag' },
])

const includeWarnings = computed({
  get: () => isTrue(filters.include_warnings),
  set: (value: boolean) => {
    filters.include_warnings = value ? 'true' : 'false'
  },
})

const params = computed<IncomeParams>(() => {
  const value = applied.value
  const by = (optional(value.by) ?? 'account') as 'account' | 'payee' | 'tag'
  const result: IncomeParams = {
    from: optional(value.from),
    to: optional(value.to),
  }
  if (by !== 'account') {
    result.by = by
  } else {
    result.depth = optional(value.depth)
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
const dateError = computed(() => dateRangeError(filters.from, filters.to))

const { data, error, loading, run } = useAsync((signal) =>
  fetchIncome(params.value, { signal }),
)

watch([params, refreshKey], () => {
  if (!dateError.value) void run()
}, { immediate: true })

const report = computed(() => data.value?.data ?? null)
const diagnostics = computed(() => data.value?.diagnostics ?? [])
const chartData = computed(() =>
  report.value
    ? rowsToChartData(sectionRows(report.value, (label) => translateReportLabel(t, label)))
    : null,
)
const chartOption = computed(() =>
  chartData.value
    ? buildBarOption(chartData.value, { format: (value) => formatDecimal(value, locale.value) })
    : null,
)
</script>

<template>
  <div class="page-stack">
    <PageHeader :title="t('income.title')" :subtitle="t('income.subtitle')" />

    <FilterCard @apply="apply" @reset="reset">
      <DateField v-model="filters.from" :label="t('common.from')" />
      <DateField v-model="filters.to" :label="t('common.to')" :error="Boolean(dateError)" :error-messages="dateError ? t('common.invalidDateRange') : undefined" />
      <VSelect v-model="filters.by" :items="groupingItems" :label="t('common.groupedBy')" />
      <VTextField
        v-model="filters.depth"
        type="number"
        min="1"
        step="1"
        :label="t('common.depth')"
        :disabled="filters.by !== 'account'"
      />
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
        :title="t('income.composition')"
        :hint="t('income.compositionHint')"
        :option="chartOption"
        :data="chartData"
      />

      <DiagnosticsCard :diagnostics="diagnostics" />
    </template>
  </div>
</template>
