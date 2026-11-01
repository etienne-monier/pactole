import { describe, expect, it } from 'vitest'

import {
  expensesGrouping,
  expensesParams,
  normalizeExpensesFilters,
  resolveExpensesComparison,
} from '@/utils/expenses'

describe('expensesGrouping', () => {
  it('defaults to the account grouping', () => {
    expect(expensesGrouping(undefined)).toBe('account')
    expect(expensesGrouping('')).toBe('account')
    expect(expensesGrouping('unknown')).toBe('account')
  })

  it('keeps the supported groupings', () => {
    expect(expensesGrouping('account')).toBe('account')
    expect(expensesGrouping('payee')).toBe('payee')
    expect(expensesGrouping('tag')).toBe('tag')
    expect(expensesGrouping('month')).toBe('month')
  })
})

describe('resolveExpensesComparison', () => {
  it('drops a comparison combined with a monthly grouping', () => {
    expect(resolveExpensesComparison({ by: 'month', compare: 'previous' })).toEqual({
      compare: '',
    })
    expect(
      resolveExpensesComparison({
        by: 'month',
        compare: 'absolute',
        compare_from: '2025-01-01',
        compare_to: '2025-12-31',
      }),
    ).toEqual({ compare: '' })
  })

  it('accepts a previous-period comparison', () => {
    expect(resolveExpensesComparison({ compare: 'previous' })).toEqual({
      compare: 'previous',
    })
  })

  it('requires both bounds for an absolute comparison', () => {
    expect(resolveExpensesComparison({ compare: 'absolute' })).toEqual({ compare: '' })
    expect(
      resolveExpensesComparison({ compare: 'absolute', compare_from: '2025-01-01' }),
    ).toEqual({ compare: '' })
    expect(
      resolveExpensesComparison({
        compare: 'absolute',
        compare_from: '2025-01-01',
        compare_to: '2025-12-31',
      }),
    ).toEqual({
      compare: 'absolute',
      compare_from: '2025-01-01',
      compare_to: '2025-12-31',
    })
  })
})

describe('expensesParams', () => {
  it('sends the depth only for the account grouping', () => {
    expect(expensesParams({ depth: '2' })).toEqual({ from: undefined, to: undefined, depth: '2' })
    expect(expensesParams({ by: 'payee', depth: '2' })).toEqual({
      from: undefined,
      to: undefined,
      by: 'payee',
    })
    expect(expensesParams({ by: 'month', depth: '2' })).toEqual({
      from: undefined,
      to: undefined,
      by: 'month',
    })
  })

  it('trims the period and drops empty values', () => {
    expect(expensesParams({ from: ' 2026-01-01 ', to: '' })).toEqual({
      from: '2026-01-01',
      to: undefined,
    })
  })

  it('sends the extra options only without a comparison', () => {
    expect(expensesParams({ uncategorized: 'true', inactive: 'true' })).toMatchObject({
      uncategorized: true,
      inactive: true,
    })
    const compared = expensesParams({
      compare: 'previous',
      uncategorized: 'true',
      inactive: 'true',
    })
    expect(compared).toMatchObject({ compare: 'previous' })
    expect(compared).not.toHaveProperty('uncategorized')
    expect(compared).not.toHaveProperty('inactive')
  })

  it('sends the difference options only with a comparison', () => {
    expect(expensesParams({ diff: 'percent', reverse_diff: 'true' })).toEqual({
      from: undefined,
      to: undefined,
    })
    expect(expensesParams({ compare: 'previous', diff: 'percent', reverse_diff: 'true' })).toEqual({
      from: undefined,
      to: undefined,
      compare: 'previous',
      diff: 'percent',
      reverse_diff: true,
    })
    expect(expensesParams({ compare: 'previous', diff: 'unknown' })).toEqual({
      from: undefined,
      to: undefined,
      compare: 'previous',
    })
  })

  it('keeps an absolute comparison with its bounds', () => {
    expect(
      expensesParams({
        compare: 'absolute',
        compare_from: '2025-01-01',
        compare_to: '2025-12-31',
      }),
    ).toEqual({
      from: undefined,
      to: undefined,
      compare: 'absolute',
      compare_from: '2025-01-01',
      compare_to: '2025-12-31',
    })
  })

  it('sends a valuation date only with a target commodity', () => {
    expect(expensesParams({ convert_to: 'USD', at: '2026-10-30' })).toEqual({
      from: undefined,
      to: undefined,
      convert_to: 'USD',
      at: '2026-10-30',
    })
    expect(expensesParams({ at: '2026-10-30' })).toEqual({ from: undefined, to: undefined })
    expect(expensesParams({ convert_to: 'USD' })).toEqual({
      from: undefined,
      to: undefined,
      convert_to: 'USD',
    })
  })

  it('sends the warnings flag when enabled', () => {
    expect(expensesParams({ include_warnings: 'true' })).toMatchObject({ include_warnings: true })
    expect(expensesParams({ include_warnings: 'false' })).not.toHaveProperty('include_warnings')
  })
})

describe('normalizeExpensesFilters', () => {
  it('clears the extra options when a comparison is active', () => {
    const values = {
      compare: 'previous',
      uncategorized: 'true',
      inactive: 'true',
    }

    normalizeExpensesFilters(values)

    expect(values).toEqual({
      compare: 'previous',
      uncategorized: 'false',
      inactive: 'false',
    })
  })

  it('clears the comparison bounds when no comparison is active', () => {
    const values = {
      compare: '',
      compare_from: '2025-01-01',
      compare_to: '2025-12-31',
    }

    normalizeExpensesFilters(values)

    expect(values).toEqual({
      compare: '',
      compare_from: '',
      compare_to: '',
    })
  })
})
