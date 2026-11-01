import { createI18n } from 'vue-i18n'

import en from './locales/en'
import fr from './locales/fr'

export const SUPPORTED_LOCALES = ['en', 'fr'] as const

export type AppLocale = (typeof SUPPORTED_LOCALES)[number]

/** Human-readable names of the supported locales, in their own language. */
export const LOCALE_LABELS: Record<AppLocale, string> = {
  en: 'English',
  fr: 'Français',
}

export type MessageSchema = typeof en

const STORAGE_KEY = 'pactole.locale'

/** Minimal storage surface, so the detection logic stays testable. */
export interface LocaleStorage {
  getItem(key: string): string | null
  setItem?(key: string, value: string): void
}

/** Returns whether a value is one of the supported locales. */
export function isSupportedLocale(value: unknown): value is AppLocale {
  return typeof value === 'string' && (SUPPORTED_LOCALES as readonly string[]).includes(value)
}

/**
 * Resolves the initial locale.
 *
 * The stored preference wins, then the first supported browser language, then
 * English. Storage and navigator failures are ignored.
 */
export function detectLocale(
  storage?: LocaleStorage | null,
  languages?: readonly string[],
): AppLocale {
  try {
    const stored = storage?.getItem(STORAGE_KEY)
    if (isSupportedLocale(stored)) {
      return stored
    }
  } catch {
    // Storage can be unavailable (private mode, disabled cookies).
  }
  for (const language of languages ?? []) {
    const base = language.toLowerCase().split('-')[0]
    if (isSupportedLocale(base)) {
      return base
    }
  }
  return 'en'
}

function defaultStorage(): LocaleStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

function defaultLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') {
    return []
  }
  if (navigator.languages && navigator.languages.length > 0) {
    return navigator.languages
  }
  return navigator.language ? [navigator.language] : []
}

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(defaultStorage(), defaultLanguages()),
  fallbackLocale: 'en',
  messages: { en, fr },
})

/** Returns the active application locale. */
export function currentLocale(): AppLocale {
  return i18n.global.locale.value as AppLocale
}

/** Applies a locale, persists it and keeps the document language in sync. */
export function setLocale(locale: AppLocale): void {
  i18n.global.locale.value = locale
  try {
    defaultStorage()?.setItem?.(STORAGE_KEY, locale)
  } catch {
    // Persisting the preference is best-effort.
  }
  applyDocumentLocale()
}

/** Reflects the active locale on the root `<html>` element. */
export function applyDocumentLocale(): void {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = currentLocale()
  }
}
