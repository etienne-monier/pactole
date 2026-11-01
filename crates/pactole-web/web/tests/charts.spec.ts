import { describe, expect, it } from 'vitest'

import type { ComparisonReport, Report } from '@/api/types'
import {
  comparisonChartData,
  hasChartData,
  historyChartData,
  rowsToChartData,
  sectionRows,
  topRows,
} from '@/utils/charts'

function report(overrides: Partial<Report> = {}): Report {
  return {
    title: 'Balance sheet',
    scope: { kind: 'as_of', date: '2026-10-30' },
    sections: [
      {
        title: 'Assets',
        lines: [
          { label: 'Assets:Checking', depth: 2, amounts: [{ commodity: 'EUR', number: '100.00' }] },
          { label: 'Assets:Savings', depth: 2, amounts: [{ commodity: 'EUR', number: '50.00' }] },
        ],
        subtotal: [{ commodity: 'EUR', number: '150.00' }],
      },
      {
        title: 'Liabilities',
        lines: [
          { label: 'Liabilities:Card', depth: 2, amounts: [{ commodity: 'USD', number: '-20.00' }] },
        ],
        subtotal: [{ commodity: 'USD', number: '-20.00' }],
      },
    ],
    totals: [
      { commodity: 'EUR', number: '150.00' },
      { commodity: 'USD', number: '-20.00' },
    ],
    total_label: 'Total',
    valuation: null,
    ...overrides,
  }
}

describe('rowsToChartData', () => {
  it('creates one series per commodity without aggregating', () => {
    const data = rowsToChartData([
      { label: 'A', amounts: [{ commodity: 'EUR', number: '10.50' }] },
      {
        label: 'B',
        amounts: [
          { commodity: 'EUR', number: '20' },
          { commodity: 'USD', number: '5' },
        ],
      },
    ])

    expect(data.categories).toEqual(['A', 'B'])
    expect(data.series.map((series) => series.name)).toEqual(['EUR', 'USD'])
    expect(data.series[0].exact).toEqual(['10.50', '20'])
    expect(data.series[0].values).toEqual([10.5, 20])
    expect(data.series[1].exact).toEqual(['', '5'])
    expect(data.series[1].values).toEqual([null, 5])
  })

  it('applies the category label callback', () => {
    const data = rowsToChartData([{ label: '2026-10', amounts: [] }], (label) => `M:${label}`)
    expect(data.categories).toEqual(['M:2026-10'])
  })
})

describe('report rows', () => {
  it('uses section subtotals for composition charts', () => {
    expect(sectionRows(report()).map((row) => row.label)).toEqual(['Assets', 'Liabilities'])
  })

  it('keeps the largest absolute rows', () => {
    const rows = [
      { label: 'small', amounts: [{ commodity: 'EUR', number: '1' }] },
      { label: 'big', amounts: [{ commodity: 'EUR', number: '-100' }] },
      { label: 'medium', amounts: [{ commodity: 'EUR', number: '20' }] },
    ]
    expect(topRows(rows, 2).map((row) => row.label)).toEqual(['big', 'medium'])
  })
})

describe('historyChartData', () => {
  it('builds one series per section and commodity', () => {
    const data = historyChartData(
      [
        { date: '2026-09-30', report: report() },
        { date: '2026-10-30', report: report() },
      ],
      (date) => date.slice(0, 7),
    )

    expect(data.categories).toEqual(['2026-09', '2026-10'])
    expect(data.series.map((series) => series.name)).toEqual([
      'Assets · EUR',
      'Liabilities · USD',
    ])
    expect(data.series[0].exact).toEqual(['150.00', '150.00'])
  })
})

describe('comparisonChartData', () => {
  it('keeps lines from different sections distinct', () => {
    const comparison: ComparisonReport = {
      title: 'Comparison',
      direction: 'current_minus_previous',
      mode: 'absolute',
      current_period: { start: '2026-10-01', end: '2026-10-31' },
      previous_period: { start: '2026-09-01', end: '2026-09-30' },
      lines: [
        {
          label: 'Same',
          depth: 2,
          section: 'Assets',
          commodity: 'EUR',
          current: '10',
          previous: '8',
          difference: '2',
        },
        {
          label: 'Same',
          depth: 2,
          section: 'Liabilities',
          commodity: 'EUR',
          current: '4',
          previous: '3',
          difference: '1',
        },
      ],
      totals: [],
      valuation: null,
    }

    const data = comparisonChartData(comparison, { current: 'Current', previous: 'Previous' })
    expect(data.categories).toEqual(['Assets · Same', 'Liabilities · Same'])
    expect(data.series.map((series) => series.name)).toEqual([
      'Current · EUR',
      'Previous · EUR',
    ])
    expect(data.series[0].exact).toEqual(['10', '4'])
    expect(data.series[1].exact).toEqual(['8', '3'])
  })
})

describe('hasChartData', () => {
  it('requires a drawable value', () => {
    expect(hasChartData(null)).toBe(false)
    expect(hasChartData({ categories: [], series: [] })).toBe(false)
    expect(
      hasChartData({ categories: ['A'], series: [{ name: 'EUR', values: [null], exact: [''] }] }),
    ).toBe(false)
    expect(
      hasChartData({ categories: ['A'], series: [{ name: 'EUR', values: [0], exact: ['0'] }] }),
    ).toBe(true)
  })
})
