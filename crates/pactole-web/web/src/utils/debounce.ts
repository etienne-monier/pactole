/** Runs a function after a delay, cancelling the previous call. */
export function debounce<T extends (...args: never[]) => void>(
  callback: T,
  delay = 250,
): (...args: Parameters<T>) => void {
  let handle: ReturnType<typeof setTimeout> | undefined
  return (...args: Parameters<T>) => {
    if (handle !== undefined) {
      clearTimeout(handle)
    }
    handle = setTimeout(() => {
      handle = undefined
      callback(...args)
    }, delay)
  }
}
