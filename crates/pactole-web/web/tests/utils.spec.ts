import { describe, expect, it } from 'vitest'

import { formatDate } from '@/i18n/format'
import { isTrue, optional, optionalBoolean } from '@/utils/params'
import { scopeText, sectionAmounts } from '@/utils/report'
import type { Report } from '@/api/types'

describe('query parameter helpers', () => {
  it('drops empty values', () => {
    expect(optional('')).toBeUndefined()
    expect(optional('   ')).toBeUndefined()
    expect(optional(undefined)).toBeUndefined()
    expect(optional(' 2026-10-30 ')).toBe('2026-10-30')
  })

  it('only sends enabled booleans', () => {
    expect(optionalBoolean(true)).toBe(true)
    expect(optionalBoolean(false)).toBeUndefined()
    expect(isTrue('true')).toBe(true)
    expect(isTrue('false')).toBe(false)
  })
})

describe('scopeText', () => {
  it('renders an as-of scope', () => {
    expect(scopeText({ kind: 'as_of', date: '2026-10-30' }, 'en', 'As of')).toBe(
      `As of ${formatDate('2026-10-30', 'en')}`,
    )
  })

  it('renders a period scope', () => {
    expect(
      scopeText({ kind: 'period', start: '2026-10-01', end: '2026-10-30' }, 'en', 'As of'),
    ).toBe(`${formatDate('2026-10-01', 'en')} – ${formatDate('2026-10-30', 'en')}`)
  })
})

describe('sectionAmounts', () => {
  it('finds a section by title and falls back by index', () => {
    const report: Report = {
      title: 'Income statement',
      scope: { kind: 'period', start: '2026-10-01', end: '2026-10-30' },
      sections: [
        {
          title: 'Income',
          lines: [],
          subtotal: [{ commodity: 'EUR', number: '10' }],
        },
        {
          title: 'Expenses',
          lines: [],
          subtotal: [{ commodity: 'EUR', number: '4' }],
        },
      ],
      totals: [{ commodity: 'EUR', number: '6' }],
      total_label: 'Net result',
      valuation: null,
    }

    expect(sectionAmounts(report, 'Expenses', 0)[0].number).toBe('4')
    expect(sectionAmounts(report, 'Missing', 0)[0].number).toBe('10')
    expect(sectionAmounts(report, 'Missing', 9)).toEqual([])
  })
})
