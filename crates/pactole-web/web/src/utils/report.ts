import type { Report, ReportScope } from '@/api/types'
import { formatDate } from '@/i18n/format'

/** Renders a report scope as a human-readable, locale-aware label. */
export function scopeText(scope: ReportScope, locale: string, asOf: string): string {
  if (scope.kind === 'as_of') {
    return `${asOf} ${formatDate(scope.date, locale)}`
  }
  return `${formatDate(scope.start, locale)} – ${formatDate(scope.end, locale)}`
}

/** Returns the subtotal of a named section, with an index-based fallback. */
export function sectionAmounts(report: Report, title: string, fallbackIndex: number) {
  const section = report.sections.find((candidate) => candidate.title === title)
  return section?.subtotal ?? report.sections[fallbackIndex]?.subtotal ?? []
}
