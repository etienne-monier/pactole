import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  ApiError,
  buildUrl,
  clearApiCache,
  fetchBalance,
  isAbortError,
  toApiError,
} from '@/api/client'

describe('buildUrl', () => {
  it('prefixes the API root and encodes the query string', () => {
    expect(buildUrl('/balance', { as_of: '2026-10-30' })).toBe(
      '/api/v1/balance?as_of=2026-10-30',
    )
    expect(buildUrl('/register', { payee: 'Coffee shop' })).toBe(
      '/api/v1/register?payee=Coffee+shop',
    )
  })

  it('omits empty and undefined parameters', () => {
    expect(buildUrl('/income', { from: '', to: undefined, depth: null })).toBe(
      '/api/v1/income',
    )
  })

  it('keeps false booleans, which are meaningful query values', () => {
    expect(buildUrl('/balance', { include_warnings: false })).toBe(
      '/api/v1/balance?include_warnings=false',
    )
  })
})

describe('getJson', () => {
  const report = { data: { title: 'Balance' }, diagnostics: [] }

  beforeEach(() => {
    clearApiCache()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('caches a successful response', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(report), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    await expect(fetchBalance({})).resolves.toEqual(report)
    await expect(fetchBalance({})).resolves.toEqual(report)

    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('keeps the structured error of a failed response and caches nothing', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: { code: 'invalid-parameter', message: 'bad date' } }), {
        status: 400,
        headers: { 'content-type': 'application/json' },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const error = await fetchBalance({ as_of: 'nope' }).catch((cause: unknown) => cause)
    expect(error).toBeInstanceOf(ApiError)
    expect((error as ApiError).code).toBe('invalid-parameter')
    expect((error as ApiError).message).toBe('bad date')

    await fetchBalance({ as_of: 'nope' }).catch(() => undefined)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('rejects a successful response with an unreadable body instead of caching it', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('<html>proxy error</html>', {
        status: 200,
        headers: { 'content-type': 'text/html' },
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const error = await fetchBalance({}).catch((cause: unknown) => cause)
    expect(error).toBeInstanceOf(ApiError)
    expect((error as ApiError).code).toBe('http-error')

    await fetchBalance({}).catch(() => undefined)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('rethrows an abort raised while reading the body', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.reject(new DOMException('aborted', 'AbortError')),
    } satisfies Partial<Response>)
    vi.stubGlobal('fetch', fetchMock)

    const error = await fetchBalance({}).catch((cause: unknown) => cause)
    expect(isAbortError(error)).toBe(true)

    await fetchBalance({}).catch(() => undefined)
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})

describe('errors', () => {
  it('keeps structured API errors', () => {
    const error = new ApiError('invalid-parameter', 'bad date', { field: 'as_of' })
    expect(toApiError(error)).toBe(error)
    expect(error.code).toBe('invalid-parameter')
  })

  it('wraps unknown failures as network errors', () => {
    const error = toApiError(new Error('offline'))
    expect(error.code).toBe('network-error')
    expect(error.message).toBe('offline')
  })

  it('recognises aborted requests', () => {
    expect(isAbortError(new DOMException('aborted', 'AbortError'))).toBe(true)
    expect(isAbortError(new Error('aborted'))).toBe(false)
  })
})
