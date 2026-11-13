<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { Valuation } from '@/api/types'
import MaskedValue from '@/components/MaskedValue.vue'
import { formatDate } from '@/i18n/format'

defineProps<{ valuation: Valuation | null }>()

const { t, locale } = useI18n()
</script>

<template>
  <VAlert v-if="valuation" type="info" variant="tonal" density="compact" class="mb-4">
    <div class="font-weight-medium">
      {{ t('common.currency') }}: {{ valuation.currency }} — {{ t('common.valuationDate') }}:
      {{ formatDate(valuation.date, locale) }}
    </div>
    <ul v-if="valuation.prices.length > 0" class="valuation-list">
      <li v-for="(price, index) in valuation.prices" :key="index">
         {{ price.base }} → {{ price.quote }} = <MaskedValue :text="price.rate" />
         ({{ formatDate(price.date, locale) }})
      </li>
    </ul>
    <div v-if="valuation.missing.length > 0" class="valuation-missing">
      {{ t('errors.report-error', { message: valuation.missing.join(', ') }) }}
    </div>
  </VAlert>
</template>

<style scoped>
.valuation-list {
  margin: 0.5rem 0 0;
  padding-left: 1.25rem;
}
.valuation-missing {
  margin-top: 0.5rem;
  font-weight: 500;
}
</style>
