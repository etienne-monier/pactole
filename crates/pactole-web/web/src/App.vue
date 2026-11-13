<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useDisplay, useTheme } from 'vuetify'

import { clearApiCache } from '@/api/client'
import { useCapabilities } from '@/composables/useCapabilities'
import { useDiagnosticsSummary } from '@/composables/useDiagnosticsSummary'
import { usePrivacyMode } from '@/composables/usePrivacyMode'
import { useRefresh } from '@/composables/useRefresh'
import {
  applyDocumentLocale,
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
  setLocale,
  type AppLocale,
} from '@/i18n'
import { storeTheme, type ThemeName } from '@/theme'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const { mdAndUp } = useDisplay()
const theme = useTheme()

const drawer = ref(mdAndUp.value)
watch(mdAndUp, (value) => {
  drawer.value = value
})

const { capabilities, error: capabilitiesError, load: loadCapabilities } = useCapabilities()
const { errors, warnings, load: loadDiagnostics } = useDiagnosticsSummary()
const { requestRefresh } = useRefresh()
const { amountsHidden, toggleAmountsHidden } = usePrivacyMode()

const diagnosticCount = computed(() => errors.value + warnings.value)
const diagnosticColor = computed(() => (errors.value > 0 ? 'error' : 'warning'))
const privacyLabel = computed(() =>
  amountsHidden.value ? t('common.showAmounts') : t('common.hideAmounts'),
)

const navItems = computed(() => [
  { route: 'dashboard', icon: 'mdi-view-dashboard-outline', title: t('nav.dashboard') },
  { route: 'balance', icon: 'mdi-scale-balance', title: t('nav.balance') },
  { route: 'income', icon: 'mdi-chart-timeline-variant', title: t('nav.income') },
  { route: 'expenses', icon: 'mdi-cash-minus', title: t('nav.expenses') },
  { route: 'register', icon: 'mdi-format-list-bulleted', title: t('nav.register') },
  { route: 'diagnostics', icon: 'mdi-alert-circle-outline', title: t('nav.diagnostics') },
])

const localeItems = SUPPORTED_LOCALES.map((value) => ({
  value,
  title: LOCALE_LABELS[value],
}))

function changeLocale(value: AppLocale): void {
  setLocale(value)
}

function toggleTheme(): void {
  const next: ThemeName = theme.global.name.value === 'dark' ? 'light' : 'dark'
  theme.global.name.value = next
  storeTheme(next)
}

function refresh(): void {
  clearApiCache()
  requestRefresh()
  void loadCapabilities(true)
  void loadDiagnostics(true)
}

function goToDiagnostics(): void {
  void router.push({ name: 'diagnostics' })
}

function updateTitle(): void {
  const key = typeof route.meta.titleKey === 'string' ? route.meta.titleKey : 'common.appName'
  document.title = `${t(key)} · ${t('common.appName')}`
}

watch(locale, () => {
  applyDocumentLocale()
  updateTitle()
})
watch([() => route.meta.titleKey, locale], updateTitle, { immediate: true })

onMounted(() => {
  applyDocumentLocale()
  void loadCapabilities()
  void loadDiagnostics()
})
</script>

<template>
  <VApp>
    <VNavigationDrawer v-model="drawer" :permanent="mdAndUp" :temporary="!mdAndUp" width="260">
      <div class="drawer-brand">
        <img class="brand-logo" src="/pactole-logo.svg" alt="" aria-hidden="true" />
        <div>
          <div class="brand-name">{{ t('common.appName') }}</div>
          <div class="brand-tagline">{{ t('common.readOnly') }}</div>
        </div>
      </div>
      <VDivider />
      <VList nav class="py-2">
        <VListItem
          v-for="item in navItems"
          :key="item.route"
          :to="{ name: item.route }"
          :prepend-icon="item.icon"
          :title="item.title"
          :exact="item.route === 'dashboard'"
          rounded="lg"
        />
      </VList>
      <template #append>
        <div class="drawer-footer">
          <VChip
            v-if="capabilities"
            size="small"
            variant="tonal"
            :color="capabilities.write ? 'success' : 'default'"
            :prepend-icon="capabilities.write ? 'mdi-pencil-outline' : 'mdi-eye-outline'"
          >
            {{ capabilities.write ? t('capabilities.write') : t('common.readOnly') }}
          </VChip>
        </div>
      </template>
    </VNavigationDrawer>

    <VAppBar flat border="b">
      <VAppBarNavIcon :aria-label="t('common.menu')" @click="drawer = !drawer" />
      <VAppBarTitle class="app-title">{{ t('common.appName') }}</VAppBarTitle>
      <VSpacer />

      <VBtn
        :icon="amountsHidden ? 'mdi-eye-off-outline' : 'mdi-eye-outline'"
        :aria-label="privacyLabel"
        :title="privacyLabel"
        :aria-pressed="amountsHidden"
        :color="amountsHidden ? 'primary' : undefined"
        @click="toggleAmountsHidden"
      />

      <VBtn
        :icon="theme.global.name.value === 'dark' ? 'mdi-white-balance-sunny' : 'mdi-weather-night'"
        :aria-label="t('common.theme')"
        @click="toggleTheme"
      />

      <VMenu>
        <template #activator="{ props }">
          <VBtn v-bind="props" icon="mdi-translate" :aria-label="t('common.language')" />
        </template>
        <VList density="compact">
          <VListItem
            v-for="item in localeItems"
            :key="item.value"
            :active="locale === item.value"
            :title="item.title"
            @click="changeLocale(item.value)"
          />
        </VList>
      </VMenu>

      <VBtn :aria-label="t('common.diagnostics')" @click="goToDiagnostics">
        <VBadge
          :content="diagnosticCount"
          :color="diagnosticColor"
          :model-value="diagnosticCount > 0"
          offset-x="6"
          offset-y="6"
        >
          <VIcon icon="mdi-alert-circle-outline" />
        </VBadge>
      </VBtn>

      <VBtn :aria-label="t('common.refresh')" icon="mdi-refresh" @click="refresh" />
    </VAppBar>

    <VMain>
      <VContainer fluid class="app-container">
        <VAlert
          v-if="capabilitiesError"
          type="warning"
          variant="tonal"
          density="compact"
          class="mb-4"
        >
          {{ t('errors.network-error') }}
        </VAlert>
        <VAlert
          v-else-if="capabilities && !capabilities.read"
          type="warning"
          variant="tonal"
          class="mb-4"
        >
          <div class="font-weight-medium">{{ t('capabilities.unavailableTitle') }}</div>
          <div>{{ t('capabilities.unavailableBody') }}</div>
        </VAlert>

        <RouterView />
      </VContainer>
    </VMain>
  </VApp>
</template>

<style scoped>
.app-title {
  font-weight: 600;
  letter-spacing: 0.02em;
}
.drawer-brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 20px 20px 16px;
}
.brand-logo {
  width: 32px;
  height: 32px;
  object-fit: contain;
  flex: 0 0 auto;
}
.brand-name {
  font-size: 1.15rem;
  font-weight: 700;
}
.brand-tagline {
  font-size: 0.75rem;
  opacity: 0.65;
}
.drawer-footer {
  padding: 16px;
}
</style>
