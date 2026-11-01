import type {
  BalanceParams,
  CapabilitiesResponse,
  DiagnosticsParams,
  DiagnosticsResponse,
  ExpensesParams,
  ExpensesResponse,
  IncomeParams,
  RegisterParams,
  RegisterResponse,
  ReportResponse,
} from './types'

/** One query string value accepted by the API client. */
export type QueryValue = string | number | boolean | null | undefined

/** Query parameters accepted by the API client. */
export type QueryParams = object

/** Structured error returned by the web API. */
export class ApiError extends Error {
  readonly code: string
  readonly details: unknown

  constructor(code: string, message: string, details: unknown = null) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.details = details
  }
}

const API_ROOT = '/api/v1'
const CACHE_TTL_MS = 15_000

interface CacheEntry {
  expires: number
  value: unknown
}

const cache = new Map<string, CacheEntry>()

/** Drops every cached response, used by the refresh action. */
export function clearApiCache(): void {
  cache.clear()
}

/** Builds a request path with a deterministic, encoded query string. */
export function buildUrl(path: string, params: QueryParams = {}): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') {
      continue
    }
    search.set(key, String(value))
  }
  const query = search.toString()
  return `${API_ROOT}${path}${query === '' ? '' : `?${query}`}`
}

/** Returns whether an error comes from an aborted request. */
export function isAbortError(error: unknown): boolean {
  return (
    (error instanceof DOMException && error.name === 'AbortError') ||
    (typeof error === 'object' && error !== null && (error as { name?: string }).name === 'AbortError')
  )
}

/** Converts an unknown failure into an `ApiError`. */
export function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) {
    return error
  }
  if (error instanceof Error) {
    return new ApiError('network-error', error.message)
  }
  return new ApiError('network-error', String(error))
}

interface RequestOptions {
  signal?: AbortSignal
  cacheTtl?: number
}

/** JSON body of an API response, including the structured error envelope. */
type ApiPayload = { error?: { code?: string; message?: string; details?: unknown } } | null

async function getJson<T>(path: string, params: QueryParams, options: RequestOptions): Promise<T> {
  const url = buildUrl(path, params)
  const cacheTtl = options.cacheTtl ?? CACHE_TTL_MS
  if (cacheTtl > 0) {
    const cached = cache.get(url)
    if (cached && cached.expires > Date.now()) {
      return cached.value as T
    }
  }

  let response: Response
  try {
    response = await fetch(url, {
      signal: options.signal,
      headers: { accept: 'application/json' },
    })
  } catch (error) {
    if (isAbortError(error)) {
      throw error
    }
    throw toApiError(error)
  }

  let payload: ApiPayload
  try {
    payload = (await response.json()) as ApiPayload
  } catch (error) {
    // An abort while the body is being read must stay an abort: swallowing it
    // into `null` would cache an empty response for the next caller.
    if (isAbortError(error)) {
      throw error
    }
    payload = null
  }
  if (!response.ok) {
    throw new ApiError(
      payload?.error?.code ?? 'http-error',
      payload?.error?.message ?? `HTTP ${response.status}`,
      payload?.error?.details ?? null,
    )
  }
  if (payload === null) {
    // A successful status with an unreadable body is an error, not empty data.
    throw new ApiError('http-error', `HTTP ${response.status} with an invalid JSON body`)
  }
  if (cacheTtl > 0) {
    cache.set(url, { expires: Date.now() + cacheTtl, value: payload })
  }
  return payload as T
}

/** Reads the backend capabilities. */
export function fetchCapabilities(options: RequestOptions = {}): Promise<CapabilitiesResponse> {
  return getJson<CapabilitiesResponse>('/capabilities', {}, options)
}

/** Builds a balance sheet. */
export function fetchBalance(
  params: BalanceParams,
  options: RequestOptions = {},
): Promise<ReportResponse> {
  return getJson<ReportResponse>('/balance', params, options)
}

/** Builds an income statement. */
export function fetchIncome(
  params: IncomeParams,
  options: RequestOptions = {},
): Promise<ReportResponse> {
  return getJson<ReportResponse>('/income', params, options)
}

/** Builds an expense breakdown. */
export function fetchExpenses(
  params: ExpensesParams,
  options: RequestOptions = {},
): Promise<ExpensesResponse> {
  return getJson<ExpensesResponse>('/expenses', params, options)
}

/** Reads the register. */
export function fetchRegister(
  params: RegisterParams,
  options: RequestOptions = {},
): Promise<RegisterResponse> {
  return getJson<RegisterResponse>('/register', params, options)
}

/** Reads the project diagnostics. */
export function fetchDiagnostics(
  params: DiagnosticsParams,
  options: RequestOptions = {},
): Promise<DiagnosticsResponse> {
  return getJson<DiagnosticsResponse>('/diagnostics', params, options)
}

/**
 * Loads balance points with a bounded concurrency.
 *
 * Each point is an independent `balance` request, cached by the client. A
 * failing point does not abort the others: the caller receives the successful
 * points and the failures.
 */
export interface BalanceSeriesPoint {
  date: string
  response: ReportResponse
}

export interface BalanceSeriesResult {
  points: BalanceSeriesPoint[]
  failures: { date: string; error: ApiError }[]
}

export async function fetchBalanceSeries(
  dates: readonly string[],
  params: Omit<BalanceParams, 'as_of' | 'at'> = {},
  options: RequestOptions & {
    concurrency?: number
    onPoint?: (point: BalanceSeriesPoint) => void
  } = {},
): Promise<BalanceSeriesResult> {
  const batch = await getJson<{ points: BalanceSeriesPoint[]; failures: { date: string; error: ApiError }[] }>(
    '/balance-series',
    { ...params, dates: dates.join(',') },
    { signal: options.signal, cacheTtl: options.cacheTtl },
  )
  for (const point of batch.points) {
    try { options.onPoint?.(point) } catch { /* progressive rendering is best effort */ }
  }
  return batch
}
