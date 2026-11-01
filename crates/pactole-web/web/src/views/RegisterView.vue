<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchBalance, fetchRegister } from '@/api/client'
import type { RegisterParams } from '@/api/types'
import AmountText from '@/components/AmountText.vue'
import ChartCard from '@/components/ChartCard.vue'
import DiagnosticsCard from '@/components/DiagnosticsCard.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import FilterCard from '@/components/FilterCard.vue'
import LoadingBlock from '@/components/LoadingBlock.vue'
import PageHeader from '@/components/PageHeader.vue'
import ValuationDetails from '@/components/ValuationDetails.vue'
import { useAsync } from '@/composables/useAsync'
import { useFilters } from '@/composables/useFilters'
import { useRefresh } from '@/composables/useRefresh'
import { formatDate, formatDecimal } from '@/i18n/format'
import { todayIso } from '@/utils/dates'
import { buildLineOption, rowsToChartData } from '@/utils/charts'
import { isTrue, optional } from '@/utils/params'

const { t, locale } = useI18n()
const { refreshKey } = useRefresh()

const PAGE_SIZE = 50

const { filters, applied, apply, reset } = useFilters({
  account: '',
  from: '',
  to: '',
  payee: '',
  narration: '',
  tag: '',
  status: '',
  convert_to: '',
  at: '',
  include_warnings: 'false',
})

const statusItems = computed(() => [
  { title: t('common.all'), value: '' },
  { title: t('status.cleared'), value: 'cleared' },
  { title: t('status.pending'), value: 'pending' },
  { title: t('status.uncleared'), value: 'uncleared' },
])

const includeWarnings = computed({
  get: () => isTrue(filters.include_warnings),
  set: (value: boolean) => {
    filters.include_warnings = value ? 'true' : 'false'
  },
})

const params = computed<RegisterParams>(() => {
  const value = applied.value
  const result: RegisterParams = {
    account: optional(value.account),
    from: optional(value.from),
    to: optional(value.to),
    payee: optional(value.payee),
    narration: optional(value.narration),
    tag: optional(value.tag),
  }
  const status = optional(value.status)
  if (status === 'cleared' || status === 'pending' || status === 'uncleared') {
    result.status = status
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
  fetchRegister(params.value, { signal }),
)
const accountSuggestions = useAsync((signal) => fetchBalance({ as_of: todayIso() }, { signal }))

watch([params, refreshKey], () => void run(), { immediate: true })
watch(refreshKey, () => void accountSuggestions.run(), { immediate: true })

const entries = computed(() => data.value?.data.entries ?? [])
const diagnostics = computed(() => data.value?.diagnostics ?? [])
const valuation = computed(() => data.value?.data.valuation ?? null)
const accountFilter = computed(() => optional(applied.value.account) ?? '')
const accountOptions = computed(() => {
  const labels = new Set<string>()
  for (const section of accountSuggestions.data.value?.data.sections ?? []) {
    for (const line of section.lines) labels.add(line.label)
  }
  return [...labels].sort()
})
const accountError = computed(() =>
  filters.account && accountOptions.value.length > 0 && !accountOptions.value.includes(filters.account)
    ? t('register.unknownAccount')
    : undefined,
)

function applyFilters(): void {
  if (!accountError.value) apply()
}

const page = ref(1)
const pageCount = computed(() => Math.max(1, Math.ceil(entries.value.length / PAGE_SIZE)))
const pagedEntries = computed(() =>
  entries.value.slice((page.value - 1) * PAGE_SIZE, page.value * PAGE_SIZE),
)
watch(entries, () => {
  page.value = 1
})

const balanceData = computed(() => {
  if (!accountFilter.value) {
    return null
  }
  return rowsToChartData(
    entries.value.map((entry) => ({
      label: `${entry.date} · ${entry.payee}`,
      amounts: entry.balance,
    })),
  )
})
const balanceOption = computed(() =>
  balanceData.value
    ? buildLineOption(balanceData.value, {
        format: (value) => formatDecimal(value, locale.value),
      })
    : null,
)
</script>

<template>
  <div class="page-stack">
    <PageHeader :title="t('register.title')" :subtitle="t('register.subtitle')" />

    <FilterCard @apply="applyFilters" @reset="reset">
      <VAutocomplete
        v-model="filters.account"
        :items="accountOptions"
        :label="t('common.account')"
        :error-messages="accountError"
        clearable
        hide-no-data
      />
      <VTextField v-model="filters.from" type="date" :label="t('common.from')" />
      <VTextField v-model="filters.to" type="date" :label="t('common.to')" />
      <VTextField v-model="filters.payee" :label="t('common.payee')" />
      <VTextField v-model="filters.narration" :label="t('common.narration')" />
      <VTextField v-model="filters.tag" :label="t('common.tag')" />
      <VSelect v-model="filters.status" :items="statusItems" :label="t('common.status')" />
      <VTextField v-model="filters.convert_to" :label="t('common.currency')" />
      <VTextField
        v-model="filters.at"
        type="date"
        :label="t('common.valuationDate')"
        :disabled="!filters.convert_to"
      />
      <VSwitch v-model="includeWarnings" :label="t('common.showWarnings')" hide-details />
    </FilterCard>

    <ErrorAlert v-if="error" :error="error" />
    <LoadingBlock v-if="loading && entries.length === 0" />

    <template v-if="data">
      <VCard>
        <VCardItem>
          <VCardTitle>{{ t('register.entries') }}</VCardTitle>
          <VCardSubtitle>{{ t('common.rows', { count: entries.length }) }}</VCardSubtitle>
        </VCardItem>
        <VCardText>
          <ValuationDetails :valuation="valuation" />
          <VAlert v-if="entries.length === 0" type="info" variant="tonal">
            {{ t('common.noData') }}
          </VAlert>
          <div v-else class="table-wrapper">
            <VTable density="compact">
              <thead>
                <tr>
                  <th scope="col">{{ t('common.date') }}</th>
                  <th scope="col">{{ t('common.status') }}</th>
                  <th scope="col">{{ t('common.payee') }}</th>
                  <th scope="col">{{ t('common.narration') }}</th>
                  <th scope="col">{{ t('common.account') }}</th>
                  <th scope="col" class="text-right">{{ t('common.amount') }}</th>
                  <th scope="col" class="text-right">{{ t('register.balanceHistory') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(entry, index) in pagedEntries" :key="index">
                  <td class="nowrap">{{ formatDate(entry.date, locale) }}</td>
                  <td>
                    <VChip size="x-small" variant="tonal">{{ t(`status.${entry.status}`) }}</VChip>
                  </td>
                  <td>{{ entry.payee }}</td>
                  <td>{{ entry.narration ?? '—' }}</td>
                  <td>{{ entry.account }}</td>
                  <td class="text-right">
                    <AmountText :amounts="entry.amount ? [entry.amount] : []" />
                  </td>
                  <td class="text-right"><AmountText :amounts="entry.balance" /></td>
                </tr>
              </tbody>
            </VTable>
          </div>
          <VPagination
            v-if="pageCount > 1"
            v-model="page"
            :length="pageCount"
            :total-visible="7"
            class="mt-4"
          />
        </VCardText>
      </VCard>

      <ChartCard
        v-if="accountFilter"
        :title="t('register.balanceHistory')"
        :hint="t('register.balanceHistoryHint')"
        :option="balanceOption"
        :data="balanceData"
      />
      <VAlert v-else type="info" variant="tonal">{{ t('register.accountRequired') }}</VAlert>

      <DiagnosticsCard :diagnostics="diagnostics" />
    </template>
  </div>
</template>

<style scoped>
.table-wrapper {
  overflow-x: auto;
}
.nowrap {
  white-space: nowrap;
}
</style>
