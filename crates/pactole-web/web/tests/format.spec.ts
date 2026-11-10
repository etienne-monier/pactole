import { describe, expect, it } from 'vitest'

import {
  chartNumber,
  formatDate,
  formatDecimal,
  formatMonth,
  formatPercent,
  separatorsFor,
} from '@/i18n/format'

describe('formatDecimal', () => {
  it('groups the integer part and preserves every decimal digit', () => {
    expect(formatDecimal('1234567.8900', 'en')).toBe('1,234,567.8900')
    expect(formatDecimal('0.50', 'en')).toBe('0.50')
    expect(formatDecimal('-42', 'en')).toBe('-42.00')
  })

  it('uses the locale separators', () => {
    const separators = separatorsFor('fr')
    expect(formatDecimal('1234567.89', 'fr')).toBe(`1${separators.group}234${separators.group}567,89`)
  })

  it('never parses or rounds the value', () => {
    expect(formatDecimal('999999999999999999.999999999999999999', 'en')).toBe(
      '999,999,999,999,999,999.999999999999999999',
    )
  })

  it('handles empty and signed values', () => {
    expect(formatDecimal('', 'en')).toBe('')
    expect(formatDecimal('+12.5', 'en')).toBe('12.50')
  })
})

describe('formatPercent', () => {
  it('keeps the server percentage exact', () => {
    expect(formatPercent('12.345%', 'en')).toBe('12.345%')
    expect(formatPercent('12.345%', 'fr')).toBe('12,345%')
  })

  it('falls back to decimal formatting without a percent sign', () => {
    expect(formatPercent('1234.5', 'en')).toBe('1,234.5')
  })
})

describe('date formatting', () => {
  it('formats ISO dates without timezone drift', () => {
    const formatted = formatDate('2026-10-30', 'en')
    expect(formatted).toContain('2026')
    expect(formatted).toContain('30')
  })

  it('formats ISO months', () => {
    expect(formatMonth('2026-10', 'fr')).toContain('2026')
  })

  it('returns invalid input unchanged', () => {
    expect(formatDate('not-a-date', 'en')).toBe('not-a-date')
    expect(formatMonth('2026', 'en')).toBe('2026')
  })
})

describe('chartNumber', () => {
  it('converts exact strings for geometry only', () => {
    expect(chartNumber('12.50')).toBe(12.5)
    expect(chartNumber('not-a-number')).toBeNull()
  })
})
