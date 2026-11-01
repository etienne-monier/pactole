<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchDiagnostics } from '@/api/client'
import DiagnosticsTable from '@/components/DiagnosticsTable.vue'
import ErrorAlert from '@/components/ErrorAlert.vue'
import FilterCard from '@/components/FilterCard.vue'
import LoadingBlock from '@/components/LoadingBlock.vue'
import PageHeader from '@/components/PageHeader.vue'
import { useAsync } from '@/composables/useAsync'
import { useFilters } from '@/composables/useFilters'
import { useRefresh } from '@/composables/useRefresh'
import { isTrue } from '@/utils/params'

const { t } = useI18n()
const { refreshKey } = useRefresh()

const { filters, applied, apply, reset } = useFilters({ include_warnings: 'true' })

const showWarnings = computed({
  get: () => isTrue(filters.include_warnings),
  set: (value: boolean) => {
    filters.include_warnings = value ? 'true' : 'false'
  },
})

const params = computed(() => ({
  include_warnings: isTrue(applied.value.include_warnings) ? true : undefined,
}))

const { data, error, loading, run } = useAsync((signal) =>
  fetchDiagnostics(params.value, { signal }),
)

watch([params, refreshKey], () => void run(), { immediate: true })

const diagnostics = computed(() => data.value?.data.diagnostics ?? [])
const errors = computed(
  () => diagnostics.value.filter((diagnostic) => diagnostic.severity === 'error').length,
)
const warnings = computed(
  () => diagnostics.value.filter((diagnostic) => diagnostic.severity === 'warning').length,
)
</script>

<template>
  <div class="page-stack">
    <PageHeader :title="t('diagnostics.title')" :subtitle="t('diagnostics.subtitle')">
      <template #actions>
        <VChip v-if="errors > 0" color="error" variant="tonal" prepend-icon="mdi-alert-circle">
          {{ t('diagnostics.errorCount', { count: errors }) }}
        </VChip>
        <VChip v-if="warnings > 0" color="warning" variant="tonal" prepend-icon="mdi-alert">
          {{ t('diagnostics.warningCount', { count: warnings }) }}
        </VChip>
      </template>
    </PageHeader>

    <FilterCard @apply="apply" @reset="reset">
      <VSwitch
        v-model="showWarnings"
        :label="t('diagnostics.showWarnings')"
        hide-details
      />
    </FilterCard>

    <ErrorAlert v-if="error" :error="error" />
    <LoadingBlock v-if="loading && !data" />

    <VCard v-if="data">
      <VCardText>
        <VAlert v-if="diagnostics.length === 0" type="success" variant="tonal">
          {{ t('diagnostics.empty') }}
        </VAlert>
        <DiagnosticsTable v-else :diagnostics="diagnostics" />
      </VCardText>
    </VCard>
  </div>
</template>
