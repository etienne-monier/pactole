import type { EChartsOption } from 'echarts'

import type { Amount, ComparisonReport, Report } from '@/api/types'
import { chartNumber } from '@/i18n/format'

/** One chart series. `exact` keeps the decimal strings used in tooltips. */
export interface ChartSeries {
  name: string
  values: (number | null)[]
  exact: string[]
}

/** Chart-ready data: categories on the x axis, one series per commodity. */
export interface ChartData {
  categories: string[]
  tooltipCategories?: string[]
  series: ChartSeries[]
}

/** One row of a report or comparison, reduced to what a chart needs. */
export interface ChartRow {
  label: string
  amounts: Amount[]
}

/** One point of the balance history. */
export interface HistoryPoint {
  date: string
  report: Report
}

/** Formatting options shared by the chart builders. */
export interface ChartOptions {
  format: (value: string) => string
}

function amountFor(amounts: readonly Amount[], commodity: string): Amount | undefined {
  return amounts.find((amount) => amount.commodity === commodity)
}

function commoditiesOf(rows: readonly ChartRow[]): string[] {
  const commodities: string[] = []
  for (const row of rows) {
    for (const amount of row.amounts) {
      if (!commodities.includes(amount.commodity)) {
        commodities.push(amount.commodity)
      }
    }
  }
  return commodities
}

/** Turns report rows into one series per commodity, without any aggregation. */
export function rowsToChartData(
  rows: readonly ChartRow[],
  categoryLabel: (label: string) => string = (label) => label,
): ChartData {
  const commodities = commoditiesOf(rows)
  return {
    categories: rows.map((row) => categoryLabel(row.label)),
    series: commodities.map((commodity) => ({
      name: commodity,
      values: rows.map((row) => {
        const amount = amountFor(row.amounts, commodity)
        return amount ? chartNumber(amount.number) : null
      }),
      exact: rows.map((row) => amountFor(row.amounts, commodity)?.number ?? ''),
    })),
  }
}

/** Builds the section-subtotal rows of a report. */
export function sectionRows(report: Report): ChartRow[] {
  return report.sections.map((section) => ({
    label: section.title ?? report.title,
    amounts: section.subtotal,
  }))
}

/** Builds the line rows of the first (main) section of a report. */
export function mainSectionRows(report: Report): ChartRow[] {
  return (report.sections[0]?.lines ?? []).map((line) => ({
    label: line.label,
    amounts: line.amounts,
  }))
}

/** Builds the rows of a named section, matched by title. */
export function titledSectionRows(report: Report, title: string): ChartRow[] {
  const section = report.sections.find((candidate) => candidate.title === title)
  if (!section) {
    return []
  }
  return section.lines.map((line) => ({ label: line.label, amounts: line.amounts }))
}

/** Keeps the `limit` rows with the largest absolute value, for readability. */
export function topRows(rows: readonly ChartRow[], limit: number): ChartRow[] {
  const score = (row: ChartRow): number =>
    row.amounts.reduce((best, amount) => {
      const value = Math.abs(chartNumber(amount.number) ?? 0)
      return Math.max(best, value)
    }, 0)
  return [...rows].sort((left, right) => score(right) - score(left)).slice(0, limit)
}

/**
 * Builds the balance-history series.
 *
 * Each section contributes one series per commodity. Values are the server's
 * section subtotals: the client never subtracts or aggregates them.
 */
export function historyChartData(
  points: readonly HistoryPoint[],
  categoryLabel: (date: string) => string,
  tooltipCategoryLabel: (date: string) => string = (date) => date,
): ChartData {
  const titles: string[] = []
  for (const point of points) {
    for (const section of point.report.sections) {
      const title = section.title ?? point.report.title
      if (!titles.includes(title)) {
        titles.push(title)
      }
    }
  }

  const series: ChartSeries[] = []
  for (const title of titles) {
    const commodities: string[] = []
    for (const point of points) {
      const section = point.report.sections.find(
        (candidate) => (candidate.title ?? point.report.title) === title,
      )
      for (const amount of section?.subtotal ?? []) {
        if (!commodities.includes(amount.commodity)) {
          commodities.push(amount.commodity)
        }
      }
    }
    for (const commodity of commodities) {
      series.push({
        name: `${title} · ${commodity}`,
        values: points.map((point) => {
          const section = point.report.sections.find(
            (candidate) => (candidate.title ?? point.report.title) === title,
          )
          const amount = amountFor(section?.subtotal ?? [], commodity)
          return amount ? chartNumber(amount.number) : null
        }),
        exact: points.map((point) => {
          const section = point.report.sections.find(
            (candidate) => (candidate.title ?? point.report.title) === title,
          )
          return amountFor(section?.subtotal ?? [], commodity)?.number ?? ''
        }),
      })
    }
  }

  return {
    categories: points.map((point) => categoryLabel(point.date)),
    tooltipCategories: points.map((point) => tooltipCategoryLabel(point.date)),
    series,
  }
}

/**
 * Builds the comparison series: one current and one previous series per
 * commodity. Values come straight from the comparison lines.
 */
export function comparisonChartData(
  comparison: ComparisonReport,
  labels: { current: string; previous: string },
): ChartData {
  const categories: string[] = []
  const commodities: string[] = []
  const values = new Map<string, Map<string, { current: string; previous: string }>>()

  for (const line of comparison.lines) {
    const category = line.section ? `${line.section} · ${line.label}` : line.label
    if (!categories.includes(category)) {
      categories.push(category)
    }
    if (!commodities.includes(line.commodity)) {
      commodities.push(line.commodity)
    }
    if (!values.has(category)) {
      values.set(category, new Map())
    }
    values.get(category)?.set(line.commodity, {
      current: line.current,
      previous: line.previous,
    })
  }

  const series: ChartSeries[] = []
  for (const commodity of commodities) {
    for (const [period, label] of [
      ['current', labels.current],
      ['previous', labels.previous],
    ] as const) {
      series.push({
        name: `${label} · ${commodity}`,
        values: categories.map((category) => {
          const value = values.get(category)?.get(commodity)
          return value ? chartNumber(value[period]) : null
        }),
        exact: categories.map((category) => values.get(category)?.get(commodity)?.[period] ?? ''),
      })
    }
  }

  return { categories, series }
}

/** Returns whether a chart has at least one drawable value. */
export function hasChartData(data: ChartData | null): boolean {
  return Boolean(
    data &&
      data.categories.length > 0 &&
      data.series.some((series) => series.values.some((value) => value !== null)),
  )
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

interface TooltipItem {
  axisValueLabel?: string
  dataIndex?: number
  name?: string
  seriesName?: string
  marker?: string
  data?: { value?: number | null; exact?: string }
}

function tooltipFormatter(options: ChartOptions, tooltipCategories?: readonly string[]) {
  return (params: unknown): string => {
    const items = (Array.isArray(params) ? params : [params]) as TooltipItem[]
    const index = items[0]?.dataIndex
    const title = (index !== undefined ? tooltipCategories?.[index] : undefined) ?? items[0]?.axisValueLabel ?? items[0]?.name ?? ''
    const lines = items.map((item) => {
      const exact = item.data?.exact
      const rendered =
        exact !== undefined && exact !== ''
          ? options.format(exact)
          : item.data?.value === null || item.data?.value === undefined
            ? '—'
            : options.format(String(item.data.value))
      return `${item.marker ?? ''}${escapeHtml(item.seriesName ?? '')}: ${escapeHtml(rendered)}`
    })
    return [escapeHtml(title), ...lines].join('<br/>')
  }
}

function categoryAxis(data: ChartData) {
  return {
    type: 'category' as const,
    data: data.categories,
    axisLabel: {
      interval: 0,
      hideOverlap: true,
      rotate: data.categories.length > 6 ? 30 : 0,
    },
  }
}

function valueAxis(options: ChartOptions) {
  return {
    type: 'value' as const,
    axisLabel: {
      formatter: (value: number) => options.format(String(value)),
    },
  }
}

function legend(data: ChartData) {
  return data.series.length > 1 ? { type: 'scroll' as const, top: 0 } : undefined
}

/** Builds a bar chart option. Tooltips and axes keep exact decimal strings. */
export function buildBarOption(data: ChartData, options: ChartOptions): EChartsOption {
  return {
    aria: { enabled: true },
    animation: false,
    grid: {
      left: 8,
      right: 16,
      top: data.series.length > 1 ? 48 : 16,
      bottom: 8,
      containLabel: true,
    },
    legend: legend(data),
    tooltip: {
      trigger: 'axis',
      axisPointer: { type: 'shadow' },
      formatter: tooltipFormatter(options, data.tooltipCategories),
    },
    xAxis: categoryAxis(data),
    yAxis: valueAxis(options),
    series: data.series.map((series) => ({
      name: series.name,
      type: 'bar' as const,
      barMaxWidth: 32,
      data: series.values.map((value, index) => ({
        value,
        exact: series.exact[index] ?? '',
      })),
    })),
  }
}

/** Builds a line chart option. */
export function buildLineOption(data: ChartData, options: ChartOptions): EChartsOption {
  return {
    aria: { enabled: true },
    animation: false,
    grid: {
      left: 8,
      right: 16,
      top: data.series.length > 1 ? 48 : 16,
      bottom: 8,
      containLabel: true,
    },
    legend: legend(data),
    tooltip: {
      trigger: 'axis',
      formatter: tooltipFormatter(options, data.tooltipCategories),
    },
    xAxis: categoryAxis(data),
    yAxis: valueAxis(options),
    series: data.series.map((series) => ({
      name: series.name,
      type: 'line' as const,
      showSymbol: true,
      symbolSize: 6,
      connectNulls: false,
      data: series.values.map((value, index) => ({
        value,
        exact: series.exact[index] ?? '',
      })),
    })),
  }
}
