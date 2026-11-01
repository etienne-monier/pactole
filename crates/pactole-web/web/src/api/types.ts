/** One exact per-commodity amount. `number` is a decimal string, never a float. */
export interface Amount {
  commodity: string
  number: string
}

/** Inclusive date span of a report. */
export type ReportScope =
  | { kind: 'as_of'; date: string }
  | { kind: 'period'; start: string; end: string }

/** One line of a report. */
export interface ReportLine {
  label: string
  depth: number
  amounts: Amount[]
}

/** One named section of a report. */
export interface ReportSection {
  title: string | null
  lines: ReportLine[]
  subtotal: Amount[]
}

/** One market price used for report valuation. */
export interface ValuationPrice {
  base: string
  quote: string
  rate: string
  date: string
}

/** Valuation provenance and missing conversions. */
export interface Valuation {
  currency: string
  date: string
  prices: ValuationPrice[]
  missing: string[]
}

/** A rendered report, as returned by the web adapter. */
export interface Report {
  title: string
  scope: ReportScope
  sections: ReportSection[]
  totals: Amount[]
  total_label: string
  valuation: Valuation | null
}

/** One compared line. `section` is absent for flat comparisons. */
export interface ComparisonLine {
  label: string
  depth: number
  section?: string | null
  commodity: string
  current: string
  previous: string
  difference: string | null
}

/** One compared total. */
export interface ComparisonTotal {
  commodity: string
  current: string
  previous: string
  difference: string | null
}

/** One period of a comparison. */
export interface ComparisonPeriod {
  start: string
  end: string
}

/** Valuation provenance for both compared periods. */
export interface ComparisonValuation {
  current: Valuation | null
  previous: Valuation | null
}

/** A period-to-period comparison. */
export interface ComparisonReport {
  title: string
  direction: 'current_minus_previous' | 'previous_minus_current'
  mode: 'absolute' | 'percent'
  current_period: ComparisonPeriod
  previous_period: ComparisonPeriod
  lines: ComparisonLine[]
  totals: ComparisonTotal[]
  valuation: ComparisonValuation | null
}

/** One posting of the register. */
export interface RegisterEntry {
  date: string
  status: 'cleared' | 'pending' | 'uncleared'
  payee: string
  narration: string | null
  account: string
  amount: Amount | null
  balance: Amount[]
}

/** One application diagnostic. */
export interface Diagnostic {
  severity: 'error' | 'warning'
  origin: 'syntax' | 'lowering' | 'include' | 'analysis' | 'business'
  code: string
  message: string
  source: string | null
  span: {
    start: { line: number; column: number }
    end: { line: number; column: number }
    column_encoding: string
  } | null
}

/** Backend capabilities. */
export interface Capabilities {
  read: boolean
  write: boolean
  transactions: boolean
  budgets: boolean
}

/** Response envelope of the report endpoints. */
export interface ReportResponse {
  data: Report
  diagnostics: Diagnostic[]
}

/** Response of the expenses endpoint. */
export interface ExpensesResponse {
  data: {
    current: Report
    comparison: ComparisonReport | null
  }
  diagnostics: Diagnostic[]
}

/** Response of the register endpoint. */
export interface RegisterResponse {
  data: {
    entries: RegisterEntry[]
    valuation: Valuation | null
  }
  diagnostics: Diagnostic[]
}

/** Response of the diagnostics endpoint. */
export interface DiagnosticsResponse {
  data: {
    diagnostics: Diagnostic[]
  }
  diagnostics: Diagnostic[]
}

/** Response of the capabilities endpoint. */
export interface CapabilitiesResponse {
  capabilities: Capabilities
}

/** Parameters of the balance endpoint. */
export interface BalanceParams {
  as_of?: string
  depth?: string
  convert_to?: string
  at?: string
  include_warnings?: boolean
}

/** Parameters of the income endpoint. */
export interface IncomeParams {
  from?: string
  to?: string
  by?: 'account' | 'payee' | 'tag'
  depth?: string
  convert_to?: string
  at?: string
  include_warnings?: boolean
}

/** Parameters of the expenses endpoint. */
export interface ExpensesParams {
  from?: string
  to?: string
  by?: 'account' | 'payee' | 'tag' | 'month'
  depth?: string
  uncategorized?: boolean
  inactive?: boolean
  compare?: 'previous' | 'absolute'
  compare_from?: string
  compare_to?: string
  diff?: 'absolute' | 'percent'
  reverse_diff?: boolean
  convert_to?: string
  at?: string
  include_warnings?: boolean
}

/** Parameters of the register endpoint. */
export interface RegisterParams {
  account?: string
  from?: string
  to?: string
  payee?: string
  narration?: string
  tag?: string
  status?: 'cleared' | 'pending' | 'uncleared'
  convert_to?: string
  at?: string
  include_warnings?: boolean
}

/** Parameters of the diagnostics endpoint. */
export interface DiagnosticsParams {
  include_warnings?: boolean
}
