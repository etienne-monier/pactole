import { describe, expect, it } from 'vitest'

import { detectTheme, isThemeName } from '@/theme'

describe('detectTheme', () => {
  it('prefers a stored theme', () => {
    expect(detectTheme({ getItem: () => 'dark' }, false)).toBe('dark')
    expect(detectTheme({ getItem: () => 'light' }, true)).toBe('light')
  })

  it('falls back to the system preference', () => {
    expect(detectTheme(null, true)).toBe('dark')
    expect(detectTheme(null, false)).toBe('light')
  })

  it('ignores invalid stored values and unavailable storage', () => {
    expect(detectTheme({ getItem: () => 'sepia' }, true)).toBe('dark')
    const storage = {
      getItem: () => {
        throw new Error('denied')
      },
    }
    expect(detectTheme(storage, false)).toBe('light')
  })
})

describe('isThemeName', () => {
  it('only accepts supported themes', () => {
    expect(isThemeName('light')).toBe(true)
    expect(isThemeName('dark')).toBe(true)
    expect(isThemeName('system')).toBe(false)
  })
})
