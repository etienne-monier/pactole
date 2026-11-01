import type { ExpensesParams } from '@/api/types'
import { isTrue, optional } from '@/utils/params'

/** Grouping accepted by the expense report. */
export type ExpensesGrouping = 'account' | 'payee' | 'tag' | 'month'

/** Period comparison resolved from the expense filters. */
export interface ExpensesComparison {
  compare: '' | 'previous' | 'absolute'
  compare_from?: string
  compare_to?: string
}

/** Returns the effective grouping of a filter value. */
export function expensesGrouping(value: string | undefined): ExpensesGrouping {
  const grouping = optional(value)
  return grouping === 'payee' || grouping === 'tag' || grouping === 'month' ? grouping : 'account'
}

/**
 * Resolves the period comparison of the expense filters.
 *
 * The application layer rejects a comparison combined with a monthly grouping,
 * and an absolute comparison needs both bounds. Invalid combinations are
 * dropped here rather than sent, so a bookmarked URL cannot produce a request
 * the interface would not build itself.
 */
export function resolveExpensesComparison(values: Record<string, string>): ExpensesComparison {
  if (expensesGrouping(values.by) === 'month') {
    return { compare: '' }
  }
  const compare = optional(values.compare)
  if (compare === 'previous') {
    return { compare: 'previous' }
  }
  if (compare === 'absolute') {
    const from = optional(values.compare_from)
    const to = optional(values.compare_to)
    if (from && to) {
      return { compare: 'absolute', compare_from: from, compare_to: to }
    }
  }
  return { compare: '' }
}

/** Builds the expense request from an applied filter snapshot. */
export function expensesParams(values: Record<string, string>): ExpensesParams {
  const grouping = expensesGrouping(values.by)
  const comparison = resolveExpensesComparison(values)
  const result: ExpensesParams = {
    from: optional(values.from),
    to: optional(values.to),
  }
  if (grouping !== 'account') {
    result.by = grouping
  } else {
    result.depth = optional(values.depth)
  }
  // Comparison and the extra expense options are mutually exclusive in the
  // application layer, so only one of them may be sent.
  if (!comparison.compare && isTrue(values.uncategorized)) {
    result.uncategorized = true
  }
  if (!comparison.compare && isTrue(values.inactive)) {
    result.inactive = true
  }
  if (comparison.compare === 'previous') {
    result.compare = 'previous'
  } else if (comparison.compare === 'absolute') {
    result.compare = 'absolute'
    result.compare_from = comparison.compare_from
    result.compare_to = comparison.compare_to
  }
  if (result.compare) {
    const diff = optional(values.diff)
    if (diff === 'absolute' || diff === 'percent') {
      result.diff = diff
    }
    if (isTrue(values.reverse_diff)) {
      result.reverse_diff = true
    }
  }
  const convertTo = optional(values.convert_to)
  if (convertTo) {
    result.convert_to = convertTo
    const at = optional(values.at)
    if (at) {
      result.at = at
    }
  }
  if (isTrue(values.include_warnings)) {
    result.include_warnings = true
  }
  return result
}

/**
 * Clears the applied options that the chosen grouping and comparison make
 * invalid, so the URL and the request always describe the same state.
 */
export function normalizeExpensesFilters(values: Record<string, string>): void {
  const comparison = resolveExpensesComparison(values)
  if (comparison.compare) {
    values.uncategorized = 'false'
    values.inactive = 'false'
    return
  }
  values.compare = ''
  values.compare_from = ''
  values.compare_to = ''
}
