/** Returns a trimmed filter value, or `undefined` when it is empty. */
export function optional(value: string | undefined): string | undefined {
  if (value === undefined) {
    return undefined
  }
  const trimmed = value.trim()
  return trimmed === '' ? undefined : trimmed
}

/** Returns `true` when the filter is enabled, `undefined` otherwise. */
export function optionalBoolean(value: boolean): true | undefined {
  return value ? true : undefined
}

/** Returns whether a string filter holds the `"true"` value. */
export function isTrue(value: string | undefined): boolean {
  return value === 'true'
}
