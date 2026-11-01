<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchExpenses } from '@/api/client'
import type { ComparisonPeriod } from '@/api/types'
import ChartCard from '@/components/ChartCard.vue'
import ComparisonTable from '@/components/ComparisonTable.vue'
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
import { formatDate, formatDecimal, formatMonth } from '@/i18n/format'
import {
  buildBarOption,
  buildLineOption,
  comparisonChartData,
  mainSectionRows,
  rowsToChartData,
  topRows,
} from '@/utils/charts'
import { dateRangeError, todayIso, previousPeriod, yearStartIso } from '@/utils/dates'
import {
  expensesGrouping,
  expensesParams,
  normalizeExpensesFilters,
} from '@/utils/expenses'
import { isTrue, optional } from '@/utils/params'
import { scopeText } from '@/utils/report'

const { t, locale } = useI18n()
const { refreshKey } = useRefresh()

const { filters, applied, apply, reset } = useFilters({
  from: yearStartIso(),
  to: todayIso(),
  by: 'account',
  depth: '',
  uncategorized: 'false',
  inactive: 'false',
  compare: '',
  compare_from: '',
  compare_to: '',
  diff: 'absolute',
  reverse_diff: 'false',
  convert_to: '',
  at: '',
  include_warnings: 'false',
})

// A bookmarked URL may combine options the application rejects. Normalizing the
// applied snapshot once rewrites the URL to the state the view actually sends.
normalizeExpensesFilters(applied.value)

const groupingItems = computed(() => [
  { title: t('common.account'), value: 'account' },
  { title: t('common.payee'), value: 'payee' },
  { title: t('common.tag'), value: 'tag' },
  { title: t('expenses.month'), value: 'month' },
])

const comparisonItems = computed(() => [
  { title: t('expenses.compareNone'), value: '' },
  { title: t('expenses.comparePrevious'), value: 'previous' },
  { title: t('expenses.compareAbsolute'), value: 'absolute' },
])

const diffItems = computed(() => [
  { title: t('expenses.diffAbsolute'), value: 'absolute' },
  { title: t('expenses.diffPercent'), value: 'percent' },
])

const uncategorized = computed({
  get: () => isTrue(filters.uncategorized),
  set: (value: boolean) => {
    filters.uncategorized = value ? 'true' : 'false'
  },
})
const inactive = computed({
  get: () => isTrue(filters.inactive),
  set: (value: boolean) => {
    filters.inactive = value ? 'true' : 'false'
  },
})
const reverseDiff = computed({
  get: () => isTrue(filters.reverse_diff),
  set: (value: boolean) => {
    filters.reverse_diff = value ? 'true' : 'false'
  },
})
const includeWarnings = computed({
  get: () => isTrue(filters.include_warnings),
  set: (value: boolean) => {
    filters.include_warnings = value ? 'true' : 'false'
  },
})

const comparisonDraft = computed(() => (optional(filters.compare) ?? '') !== '')
const absoluteDraft = computed(() => filters.compare === 'absolute')
const comparisonAvailable = computed(() => expensesGrouping(filters.by) !== 'month')

watch(
  () => filters.by,
  (by) => {
    if (by === 'month') {
      filters.compare = ''
    }
  },
  { immediate: true },
)

watch(
  () => filters.compare,
  (compare) => {
    if (!compare) {
      filters.compare_from = ''
      filters.compare_to = ''
      return
    }
    // Comparison and the extra expense options are mutually exclusive in the
    // application layer, so selecting one clears the other.
    filters.uncategorized = 'false'
    filters.inactive = 'false'
    if (compare === 'absolute' && (!filters.compare_from || !filters.compare_to)) {
      const previous = previousPeriod(
        optional(filters.from) ?? yearStartIso(),
        optional(filters.to) ?? todayIso(),
      )
      if (previous) {
        filters.compare_from ||= previous.from
        filters.compare_to ||= previous.to
      }
    }
  },
  { immediate: true },
)

const groupingLabel = computed(
  () => groupingItems.value.find((item) => item.value === filters.by)?.title ?? '',
)

const params = computed(() => expensesParams(applied.value))
const dateError = computed(() => dateRangeError(filters.from, filters.to))
const comparisonDateError = computed(() =>
  filters.compare === 'absolute'
    ? dateRangeError(filters.compare_from, filters.compare_to)
    : null,
)

const { data, error, loading, run } = useAsync((signal) =>
  fetchExpenses(params.value, { signal }),
)

watch([params, refreshKey], () => {
  if (!dateError.value && !comparisonDateError.value) void run()
}, { immediate: true })

const report = computed(() => data.value?.data.current ?? null)
const comparison = computed(() => data.value?.data.comparison ?? null)
const diagnostics = computed(() => data.value?.diagnostics ?? [])

const chartRows = computed(() => (report.value ? topRows(mainSectionRows(report.value), 15) : []))
const chartData = computed(() => {
  if (!report.value) {
    return null
  }
  const by = expensesGrouping(applied.value.by)
  return rowsToChartData(chartRows.value, (label) =>
    by === 'month' ? formatMonth(label, locale.value, false) : label,
  )
})
const chartOption = computed(() =>
  chartData.value
    ? buildBarOption(chartData.value, { format: (value) => formatDecimal(value, locale.value) })
    : null,
)

const comparisonData = computed(() =>
  comparison.value
    ? comparisonChartData(comparison.value, {
        current: t('expenses.current'),
        previous: t('expenses.previous'),
      })
    : null,
)
const comparisonOption = computed(() =>
  comparisonData.value
    ? buildLineOption(comparisonData.value, {
        format: (value) => formatDecimal(value, locale.value),
      })
    : null,
)

function periodText(period: ComparisonPeriod): string {
  return `${formatDate(period.start, locale.value)} – ${formatDate(period.end, locale.value)}`
}
</script>

<template>
  <div class="page-stack">
    <PageHeader :title="t('expenses.title')" :subtitle="t('expenses.subtitle')" />

    <FilterCard @apply="apply" @reset="reset">
      <VTextField v-model="filters.from" type="date" :label="t('common.from')" />
      <VTextField v-model="filters.to" type="date" :label="t('common.to')" :error="Boolean(dateError)" :error-messages="dateError ? t('common.invalidDateRange') : undefined" />
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
      <VTextField
        v-model="filters.at"
        type="date"
        :label="t('common.valuationDate')"
        :disabled="!filters.convert_to"
      />
      <VSelect
        v-model="filters.compare"
        :items="comparisonItems"
        :label="t('expenses.compare')"
        :disabled="!comparisonAvailable"
        :hint="comparisonAvailable ? undefined : t('expenses.compareMonthUnavailable')"
        persistent-hint
      />
      <VTextField
        v-if="absoluteDraft"
        v-model="filters.compare_from"
        type="date"
        :label="t('expenses.previousPeriod') + ' — ' + t('common.from')"
        :error="Boolean(comparisonDateError)"
        :error-messages="comparisonDateError ? t('common.invalidDateRange') : undefined"
      />
      <VTextField
        v-if="absoluteDraft"
        v-model="filters.compare_to"
        type="date"
        :label="t('expenses.previousPeriod') + ' — ' + t('common.to')"
        :error="Boolean(comparisonDateError)"
        :error-messages="comparisonDateError ? t('common.invalidDateRange') : undefined"
      />
      <VSelect
        v-if="comparisonDraft"
        v-model="filters.diff"
        :items="diffItems"
        :label="t('expenses.diffMode')"
      />
      <VSwitch
        v-model="uncategorized"
        :label="t('expenses.uncategorized')"
        :disabled="comparisonDraft"
        hide-details
      />
      <VSwitch
        v-model="inactive"
        :label="t('expenses.inactive')"
        :disabled="comparisonDraft"
        hide-details
      />
      <VSwitch
        v-if="comparisonDraft"
        v-model="reverseDiff"
        :label="t('expenses.reverseDiff')"
        hide-details
      />
      <VSwitch v-model="includeWarnings" :label="t('common.showWarnings')" hide-details />
    </FilterCard>

    <ErrorAlert v-if="error" :error="error" />
    <LoadingBlock v-if="loading && !report" />

    <template v-if="report">
      <VCard>
        <VCardItem>
          <VCardTitle>{{ report.title }}</VCardTitle>
          <VCardSubtitle>{{ scopeText(report.scope, locale, t('common.asOf')) }}</VCardSubtitle>
        </VCardItem>
        <VCardText>
          <ValuationDetails :valuation="report.valuation" />
          <ReportTable :report="report" />
        </VCardText>
      </VCard>

      <ChartCard
        :title="t('expenses.title')"
        :hint="t('common.groupedBy') + ' · ' + groupingLabel"
        :option="chartOption"
        :data="chartData"
      />

      <template v-if="comparison">
        <VCard>
          <VCardItem>
            <VCardTitle>{{ t('expenses.comparison') }}</VCardTitle>
            <VCardSubtitle>
              {{ periodText(comparison.current_period) }} ·
              {{ periodText(comparison.previous_period) }}
            </VCardSubtitle>
          </VCardItem>
          <VCardText>
            <ValuationDetails :valuation="comparison.valuation?.current ?? null" />
            <ValuationDetails :valuation="comparison.valuation?.previous ?? null" />
            <ComparisonTable :comparison="comparison" />
          </VCardText>
        </VCard>

        <ChartCard
          :title="t('expenses.comparison')"
          :hint="t('expenses.current') + ' / ' + t('expenses.previous')"
          :option="comparisonOption"
          :data="comparisonData"
        />
      </template>

      <DiagnosticsCard :diagnostics="diagnostics" />
    </template>
  </div>
</template>
