import { describe, expect, it } from 'vitest'

import { detectLocale, isSupportedLocale, LOCALE_LABELS, SUPPORTED_LOCALES } from '@/i18n'
import en from '@/i18n/locales/en'
import fr from '@/i18n/locales/fr'

function flatten(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) {
    return [prefix]
  }
  return Object.entries(value).flatMap(([key, child]) =>
    flatten(child, prefix === '' ? key : `${prefix}.${key}`),
  )
}

describe('locale messages', () => {
  it('has the same application keys in every locale', () => {
    const reference = flatten(en)
      .filter((key) => !key.startsWith('$vuetify'))
      .sort()
    const translated = flatten(fr)
      .filter((key) => !key.startsWith('$vuetify'))
      .sort()
    expect(translated).toEqual(reference)
  })

  it('has a label for every supported locale', () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(LOCALE_LABELS[locale]).not.toBe('')
    }
  })
})

describe('detectLocale', () => {
  it('prefers the stored locale', () => {
    expect(detectLocale({ getItem: () => 'fr' }, ['en-US'])).toBe('fr')
  })

  it('falls back to the first supported browser language', () => {
    expect(detectLocale(null, ['de-DE', 'fr-CA'])).toBe('fr')
  })

  it('defaults to English', () => {
    expect(detectLocale(null, ['de-DE'])).toBe('en')
    expect(detectLocale(null, [])).toBe('en')
  })

  it('ignores unavailable storage', () => {
    const storage = {
      getItem: () => {
        throw new Error('denied')
      },
    }
    expect(detectLocale(storage, ['fr'])).toBe('fr')
  })
})

describe('isSupportedLocale', () => {
  it('only accepts known locales', () => {
    expect(isSupportedLocale('en')).toBe(true)
    expect(isSupportedLocale('fr')).toBe(true)
    expect(isSupportedLocale('de')).toBe(false)
    expect(isSupportedLocale(null)).toBe(false)
  })
})
