<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  fetchBalance,
  fetchBalanceSeries,
  fetchExpenses,
  fetchIncome,
  type BalanceSeriesPoint,
} from '@/api/client'
import type { Report } from '@/api/types'
import ChartCard from '@/components/ChartCard.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import LoadingBlock from '@/components/LoadingBlock.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import { useAsync } from '@/composables/useAsync'
import { useRefresh } from '@/composables/useRefresh'
import { formatDate, formatDecimal, formatMonth } from '@/i18n/format'
import {
  buildBarOption,
  buildLineOption,
  historyChartData,
  mainSectionRows,
  rowsToChartData,
} from '@/utils/charts'
import { monthEnds, monthRange, monthStartIso, todayIso } from '@/utils/dates'

const { t, locale } = useI18n()
const { refreshKey } = useRefresh()

const today = todayIso()
const monthStart = monthStartIso()
const trendPeriod = monthRange(12)

function sectionAmounts(report: Report, title: string, fallbackIndex: number) {
  const section = report.sections.find((candidate) => candidate.title === title)
  return section?.subtotal ?? report.sections[fallbackIndex]?.subtotal ?? []
}

const summary = useAsync(async (signal) => {
  const [balance, income, trend] = await Promise.all([
    fetchBalance({ as_of: today, depth: '2' }, { signal }),
    fetchIncome({ from: monthStart, to: today }, { signal }),
    fetchExpenses(
      { from: trendPeriod.from, to: trendPeriod.to, by: 'month' },
      { signal },
    ),
  ])
  return { balance, income, trend }
})

const historyPoints = ref<BalanceSeriesPoint[]>([])
const history = useAsync(async (signal) => {
  historyPoints.value = []
  const result = await fetchBalanceSeries(monthEnds(12), { depth: '2' }, {
    signal,
    onPoint: (point) => {
      // A superseded run must not add its points to the current series.
      if (signal.aborted) {
        return
      }
      historyPoints.value = [...historyPoints.value, point].sort((left, right) =>
        left.date.localeCompare(right.date),
      )
    },
  })
  if (!signal.aborted) {
    // The resolved series is authoritative and already sorted.
    historyPoints.value = result.points
  }
  return result
})

function load(): void {
  void summary.run()
  void history.run()
}

watch(refreshKey, load, { immediate: true })

const balanceCards = computed(() => {
  const report = summary.data.value?.balance.data
  if (!report) {
    return []
  }
  return report.sections.map((section) => ({
    label: section.title ?? report.title,
    amounts: section.subtotal,
  }))
})

const incomeReport = computed(() => summary.data.value?.income.data ?? null)
const monthIncome = computed(() =>
  incomeReport.value ? sectionAmounts(incomeReport.value, 'Income', 0) : [],
)
const monthExpenses = computed(() =>
  incomeReport.value ? sectionAmounts(incomeReport.value, 'Expenses', 1) : [],
)
const monthNet = computed(() => incomeReport.value?.totals ?? [])

const historyData = computed(() =>
  historyChartData(
    historyPoints.value.map((point) => ({ date: point.date, report: point.response.data })),
    (date) => formatMonth(date.slice(0, 7), locale.value),
  ),
)
const historyOption = computed(() =>
  buildLineOption(historyData.value, { format: (value) => formatDecimal(value, locale.value) }),
)
const historyFailures = computed(() => history.data.value?.failures.length ?? 0)

const trendReport = computed(() => summary.data.value?.trend.data.current ?? null)
const trendData = computed(() =>
  trendReport.value
    ? rowsToChartData(mainSectionRows(trendReport.value), (label) =>
        formatMonth(label, locale.value),
      )
    : null,
)
const trendOption = computed(() =>
  trendData.value
    ? buildBarOption(trendData.value, { format: (value) => formatDecimal(value, locale.value) })
    : null,
)
</script>

<template>
  <div class="page-stack">
    <PageHeader
      :title="t('dashboard.title')"
      :subtitle="t('dashboard.subtitle', { date: formatDate(today, locale) })"
    >
      <template #actions>
        <VChip variant="tonal" prepend-icon="mdi-calendar-today">
          {{ formatDate(today, locale) }}
        </VChip>
      </template>
    </PageHeader>

    <ErrorAlert v-if="summary.error.value" :error="summary.error.value" />
    <LoadingBlock v-if="summary.loading.value && !summary.data.value" />

    <template v-if="summary.data.value">
      <section>
        <h2 class="text-subtitle-1 font-weight-medium mb-3">{{ t('nav.balance') }}</h2>
        <div class="card-grid">
          <StatCard
            v-for="card in balanceCards"
            :key="card.label"
            :label="card.label"
            :amounts="card.amounts"
            :hint="t('common.asOf') + ' ' + formatDate(today, locale)"
          />
        </div>
      </section>

      <section>
        <h2 class="text-subtitle-1 font-weight-medium mb-3">{{ t('dashboard.periodResult') }}</h2>
        <div class="card-grid">
          <StatCard :label="t('dashboard.monthIncome')" :amounts="monthIncome" />
          <StatCard :label="t('dashboard.monthExpenses')" :amounts="monthExpenses" />
          <StatCard :label="t('dashboard.netResult')" :amounts="monthNet" />
        </div>
        <p class="text-caption text-medium-emphasis mt-2">{{ t('dashboard.periodResultHint') }}</p>
      </section>
    </template>

    <VAlert v-if="historyFailures > 0" type="warning" variant="tonal" density="compact">
      {{ t('dashboard.historyFailed', { count: historyFailures }) }}
    </VAlert>
    <ErrorAlert v-if="history.error.value" :error="history.error.value" />

    <div class="chart-grid">
      <ChartCard
        :title="t('dashboard.balanceHistory')"
        :hint="t('dashboard.balanceHistoryHint')"
        :option="historyOption"
        :data="historyData"
      />
      <ChartCard
        :title="t('dashboard.expensesTrend')"
        :hint="t('dashboard.expensesTrendHint')"
        :option="trendOption"
        :data="trendData"
      />
    </div>
  </div>
</template>
