import { describe, expect, it } from 'vitest'

import {
  isPositiveInteger,
  monthEnds,
  monthRange,
  monthStartIso,
  previousPeriod,
  todayIso,
  yearStartIso,
} from '@/utils/dates'

describe('ISO helpers', () => {
  it('formats local dates without UTC drift', () => {
    const date = new Date(2026, 0, 5, 23, 30)
    expect(todayIso(date)).toBe('2026-01-05')
    expect(monthStartIso(date)).toBe('2026-01-01')
    expect(yearStartIso(date)).toBe('2026-01-01')
  })
})

describe('monthEnds', () => {
  it('returns the last day of each month, oldest first', () => {
    expect(monthEnds(3, new Date(2026, 9, 30))).toEqual([
      '2026-08-31',
      '2026-09-30',
      '2026-10-30',
    ])
  })

  it('bounds the current month to today', () => {
    expect(monthEnds(1, new Date(2026, 1, 15))).toEqual(['2026-02-15'])
  })

  it('handles a leap year', () => {
    expect(monthEnds(2, new Date(2028, 2, 10))).toEqual(['2028-02-29', '2028-03-10'])
  })
})

describe('monthRange', () => {
  it('starts at the first day of the oldest month', () => {
    expect(monthRange(12, new Date(2026, 9, 30))).toEqual({
      from: '2025-11-01',
      to: '2026-10-30',
    })
  })
})

describe('isPositiveInteger', () => {
  it('accepts only positive decimal integers', () => {
    expect(isPositiveInteger('1')).toBe(true)
    expect(isPositiveInteger('12')).toBe(true)
    expect(isPositiveInteger('0')).toBe(false)
    expect(isPositiveInteger('-1')).toBe(false)
    expect(isPositiveInteger('1.5')).toBe(false)
    expect(isPositiveInteger('')).toBe(false)
  })
})

describe('previousPeriod', () => {
  it('returns an equally long period immediately before the current one', () => {
    expect(previousPeriod('2026-03-01', '2026-03-31')).toEqual({
      from: '2026-01-29',
      to: '2026-02-28',
    })
  })

  it('handles leap days and single-day periods', () => {
    expect(previousPeriod('2028-03-01', '2028-03-01')).toEqual({
      from: '2028-02-29',
      to: '2028-02-29',
    })
  })

  it('rejects invalid or inverted bounds', () => {
    expect(previousPeriod('2026-02-30', '2026-03-01')).toBeNull()
    expect(previousPeriod('2026-03-31', '2026-03-01')).toBeNull()
    expect(previousPeriod('', '2026-03-01')).toBeNull()
  })
})
