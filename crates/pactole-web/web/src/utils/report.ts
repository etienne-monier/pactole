import type { Report, ReportScope } from '@/api/types'
import { formatDate } from '@/i18n/format'

/** Renders a report scope as a human-readable, locale-aware label. */
export function scopeText(scope: ReportScope, locale: string, asOf: string): string {
  if (scope.kind === 'as_of') {
    return `${asOf} ${formatDate(scope.date, locale)}`
  }
  return `${formatDate(scope.start, locale)} – ${formatDate(scope.end, locale)}`
}

/** Returns a section subtotal, preferring a stable position over presentation titles. */
export function sectionAmounts(report: Report, index: number): Report['totals']
export function sectionAmounts(report: Report, title: string, fallbackIndex: number): Report['totals']
export function sectionAmounts(report: Report, selector: number | string, fallbackIndex = 0) {
  if (typeof selector === 'number') {
    return report.sections[selector]?.subtotal ?? []
  }
  return report.sections.find((section) => section.title === selector)?.subtotal ??
    report.sections[fallbackIndex]?.subtotal ?? []
}
