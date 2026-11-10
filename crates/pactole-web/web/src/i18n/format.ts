/** Locale-specific separators used to format exact decimal strings. */
export interface DecimalSeparators {
  group: string
  decimal: string
  minus: string
}

const separatorCache = new Map<string, DecimalSeparators>()
let defaultDecimalPlaces = 2
let currencyDecimalPlaces: Record<string, number> = {}

/** Sets the exact decimal display policy received from the backend. */
export function setDecimalPlaces(defaultPlaces: number, currencies: Record<string, number>): void {
  defaultDecimalPlaces = defaultPlaces
  currencyDecimalPlaces = { ...currencies }
}

/**
 * Extracts the group, decimal and minus separators of a locale.
 *
 * `Intl.NumberFormat` is only used to discover the separators: amounts are
 * never parsed as floating-point numbers.
 */
export function separatorsFor(locale: string): DecimalSeparators {
  const cached = separatorCache.get(locale)
  if (cached) {
    return cached
  }
  const parts = new Intl.NumberFormat(locale).formatToParts(-1234567.89)
  const find = (type: Intl.NumberFormatPartTypes, fallback: string): string =>
    parts.find((part) => part.type === type)?.value ?? fallback
  const separators: DecimalSeparators = {
    group: find('group', ','),
    decimal: find('decimal', '.'),
    minus: find('minusSign', '-'),
  }
  separatorCache.set(locale, separators)
  return separators
}

function groupDigits(digits: string, separator: string): string {
  if (digits.length <= 3) {
    return digits
  }
  let output = ''
  for (let index = 0; index < digits.length; index += 1) {
    const remaining = digits.length - index
    if (index > 0 && remaining % 3 === 0) {
      output += separator
    }
    output += digits[index]
  }
  return output
}

/**
 * Formats an exact decimal string for display in a locale.
 *
 * The decimal part is preserved verbatim: this function never rounds, never
 * parses the value as a `Number`, and never performs arithmetic.
 */
export function formatDecimal(
  value: string,
  locale: string,
  commodity?: string,
  padMinimumPlaces = true,
): string {
  const trimmed = value.trim()
  if (trimmed === '') {
    return ''
  }
  const separators = separatorsFor(locale)
  const negative = trimmed.startsWith('-')
  const unsigned = trimmed.startsWith('-') || trimmed.startsWith('+') ? trimmed.slice(1) : trimmed
  const [integerPart = '', fraction] = unsigned.split('.')
  const integer = groupDigits(integerPart === '' ? '0' : integerPart, separators.group)
  const places = commodity === undefined
    ? defaultDecimalPlaces
    : currencyDecimalPlaces[commodity] ?? defaultDecimalPlaces
  const paddedFraction = !padMinimumPlaces
    ? fraction
    : fraction === undefined
      ? ''.padEnd(places, '0')
      : fraction.padEnd(places, '0')
  if (paddedFraction === undefined) {
    return `${negative ? separators.minus : ''}${integer}`
  }
  const fractionPart = separators.decimal + paddedFraction
  return `${negative ? separators.minus : ''}${integer}${fractionPart}`
}

/** Formats a server percentage such as `12.34%` for display. */
export function formatPercent(value: string, locale: string): string {
  const trimmed = value.trim()
  if (!trimmed.endsWith('%')) {
    return formatDecimal(trimmed, locale, undefined, false)
  }
  return `${formatDecimal(trimmed.slice(0, -1), locale, undefined, false)}%`
}

function parseIsoDate(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (!match) {
    return null
  }
  const [, year, month, day] = match
  return new Date(Date.UTC(Number(year), Number(month) - 1, Number(day)))
}

/** Formats an ISO date (`YYYY-MM-DD`) for display, without timezone drift. */
export function formatDate(value: string, locale: string): string {
  const date = parseIsoDate(value)
  if (!date) {
    return value
  }
  return new Intl.DateTimeFormat(locale, {
    timeZone: 'UTC',
    year: 'numeric',
    month: 'short',
    day: '2-digit',
  }).format(date)
}

/** Formats an ISO month (`YYYY-MM`) for display, without timezone drift. */
export function formatMonth(value: string, locale: string, includeYear = true): string {
  const match = /^(\d{4})-(\d{2})$/.exec(value)
  if (!match) {
    return value
  }
  const [, year, month] = match
  const date = new Date(Date.UTC(Number(year), Number(month) - 1, 1))
  return new Intl.DateTimeFormat(locale, {
    timeZone: 'UTC',
    ...(includeYear ? { year: 'numeric' as const } : {}),
    month: 'long',
  }).format(date)
}

/**
 * Converts an exact decimal string into a number for chart geometry only.
 *
 * Never use the result for display or accounting: tooltips and tables must keep
 * the original string.
 */
export function chartNumber(value: string): number | null {
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : null
}
