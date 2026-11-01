/** Formats a local date as `YYYY-MM-DD`. */
export function isoDate(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

/** Returns today's local date as `YYYY-MM-DD`. */
export function todayIso(today: Date = new Date()): string {
  return isoDate(today)
}

/** Returns the first day of the current month as `YYYY-MM-DD`. */
export function monthStartIso(today: Date = new Date()): string {
  return isoDate(new Date(today.getFullYear(), today.getMonth(), 1))
}

/** Returns January 1 of the current year as `YYYY-MM-DD`. */
export function yearStartIso(today: Date = new Date()): string {
  return `${today.getFullYear()}-01-01`
}

/**
 * Returns the last day of each of the last `count` months, oldest first.
 *
 * The current month is bounded to today, so the series never contains a future
 * date. Dates are computed in local time and serialized manually, which avoids
 * the UTC offset drift of `Date#toISOString`.
 */
export function monthEnds(count: number, today: Date = new Date()): string[] {
  const dates: string[] = []
  for (let offset = count - 1; offset >= 0; offset -= 1) {
    const first = new Date(today.getFullYear(), today.getMonth() - offset, 1)
    const last = new Date(first.getFullYear(), first.getMonth() + 1, 0)
    const day = offset === 0 ? Math.min(today.getDate(), last.getDate()) : last.getDate()
    dates.push(isoDate(new Date(first.getFullYear(), first.getMonth(), day)))
  }
  return dates
}

/** Returns a period covering the last `count` months, oldest first. */
export function monthRange(count: number, today: Date = new Date()): { from: string; to: string } {
  const first = new Date(today.getFullYear(), today.getMonth() - (count - 1), 1)
  return { from: isoDate(first), to: todayIso(today) }
}

/** Returns whether a value is a positive integer, used for `depth`. */
export function isPositiveInteger(value: string): boolean {
  return /^[1-9]\d*$/.test(value)
}

/**
 * Returns the period of the same length immediately before `from`–`to`.
 *
 * Both bounds are inclusive `YYYY-MM-DD` dates. Returns `null` when a bound is
 * not a valid date or when `from` is after `to`. Arithmetic uses UTC so that
 * daylight-saving transitions cannot shift a day.
 */
export function previousPeriod(from: string, to: string): { from: string; to: string } | null {
  const start = parseIsoDate(from)
  const end = parseIsoDate(to)
  if (start === null || end === null || start > end) {
    return null
  }
  const day = 86_400_000
  const length = Math.round((end - start) / day) + 1
  const previousEnd = start - day
  const previousStart = previousEnd - (length - 1) * day
  return { from: formatUtcDate(previousStart), to: formatUtcDate(previousEnd) }
}

function parseIsoDate(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
  if (!match) {
    return null
  }
  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  const time = Date.UTC(year, month - 1, day)
  const date = new Date(time)
  if (
    date.getUTCFullYear() !== year ||
    date.getUTCMonth() !== month - 1 ||
    date.getUTCDate() !== day
  ) {
    return null
  }
  return time
}

function formatUtcDate(time: number): string {
  const date = new Date(time)
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${date.getUTCFullYear()}-${month}-${day}`
}
