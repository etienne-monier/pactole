import { effectScope } from 'vue'
import { describe, expect, it } from 'vitest'

import { useAsync } from '@/composables/useAsync'

/** Creates a promise with externally controlled resolution. */
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise
    reject = rejectPromise
  })
  return { promise, resolve, reject }
}

/** Runs `useAsync` inside an effect scope, as a component would. */
function setup<T>(loader: (signal: AbortSignal) => Promise<T>) {
  const scope = effectScope()
  const composable = scope.run(() => useAsync(loader))
  if (!composable) {
    throw new Error('useAsync was not created')
  }
  return { ...composable, scope }
}

describe('useAsync', () => {
  it('keeps the newest result when an older run resolves last', async () => {
    const first = deferred<string>()
    const second = deferred<string>()
    const pending = [first, second]
    const { data, run, scope } = setup(() => {
      const next = pending.shift()
      if (!next) {
        throw new Error('unexpected run')
      }
      return next.promise
    })

    const firstRun = run()
    const secondRun = run()

    second.resolve('second')
    await secondRun
    first.resolve('first')
    await firstRun

    expect(data.value).toBe('second')
    scope.stop()
  })

  it('suppresses the abort of a superseded run', async () => {
    const first = deferred<string>()
    const second = deferred<string>()
    const pending = [first, second]
    const { data, error, loading, run, scope } = setup(() => {
      const next = pending.shift()
      if (!next) {
        throw new Error('unexpected run')
      }
      return next.promise
    })

    const firstRun = run()
    const secondRun = run()
    first.reject(new DOMException('aborted', 'AbortError'))
    await firstRun
    second.resolve('second')
    await secondRun

    expect(error.value).toBeNull()
    expect(data.value).toBe('second')
    expect(loading.value).toBe(false)
    scope.stop()
  })

  it('exposes the failure of the current run', async () => {
    const { error, loading, run, scope } = setup(() => Promise.reject(new Error('offline')))

    await run()

    expect(error.value?.code).toBe('network-error')
    expect(error.value?.message).toBe('offline')
    expect(loading.value).toBe(false)
    scope.stop()
  })

  it('stays loading until the current run settles', async () => {
    const result = deferred<string>()
    const { data, loading, run, scope } = setup(() => result.promise)

    const pendingRun = run()
    expect(loading.value).toBe(true)

    result.resolve('done')
    await pendingRun

    expect(loading.value).toBe(false)
    expect(data.value).toBe('done')
    scope.stop()
  })
})
