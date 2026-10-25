use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use rust_decimal::Decimal;

use crate::models::CommodityName;

use super::{Amounts, Period, Report, ReportFormat, ReportScope};

/// Direction of difference calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffDirection {
    /// `current - previous` (positive means spending went up).
    #[default]
    CurrentMinusPrevious,
    /// `previous - current` (positive means you saved money).
    PreviousMinusCurrent,
}

/// How to display differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode {
    /// Absolute difference.
    #[default]
    Absolute,
    /// Percentage difference (previous is the base).
    Percent,
}

/// One line in a comparison report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonLine {
    /// Label of the line.
    pub label: String,
    /// Hierarchical indentation.
    pub depth: usize,
    /// Amounts in the current period.
    pub current: Amounts,
    /// Amounts in the previous period.
    pub previous: Amounts,
}

/// A comparison between two periods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonReport {
    /// Report title.
    pub title: String,
    /// Current period.
    pub current_period: Period,
    /// Previous period.
    pub previous_period: Period,
    /// Lines in display order.
    pub lines: Vec<ComparisonLine>,
    /// Totals per commodity.
    pub totals: ComparisonLine,
    /// Direction of difference.
    pub direction: DiffDirection,
    /// Display mode.
    pub mode: DiffMode,
}

impl ComparisonReport {
    /// Render the comparison report.
    pub fn render(&self, format: ReportFormat) -> String {
        match format {
            ReportFormat::Text => render_text(self),
            ReportFormat::Json => render_json(self),
            ReportFormat::Csv => render_csv(self),
        }
    }
}

/// Compute the previous period of the same length.
/// If the period covers whole calendar months, shift back by that many months.
/// Otherwise shift back by the same number of days.
pub fn previous_period(period: Period) -> Option<Period> {
    let start = period.start();
    let end = period.end();
    if is_full_month_period(period) {
        let months = ((end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32
            + 1) as u32;
        let previous_start = start.checked_sub_months(chrono::Months::new(months))?;
        let previous_end = start.pred_opt()?;
        return Period::new(previous_start, previous_end).ok();
    }
    let days = (end - start).num_days() + 1;
    let previous_end = start.pred_opt()?;
    let previous_start = previous_end.checked_sub_signed(chrono::Duration::days(days - 1))?;
    Period::new(previous_start, previous_end).ok()
}

fn is_full_month_period(period: Period) -> bool {
    let start = period.start();
    let end = period.end();
    if start.day() != 1 {
        return false;
    }
    let (ny, nm) = if end.month() == 12 {
        (end.year() + 1, 1)
    } else {
        (end.year(), end.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(ny, nm, 1).and_then(|next_month| next_month.pred_opt())
        == Some(end)
}

/// Returns whether periods have the same comparison length.
///
/// Complete calendar-month ranges compare by month count; other ranges compare
/// by their inclusive number of days.
pub fn periods_have_same_length(left: Period, right: Period) -> bool {
    if is_full_month_period(left) && is_full_month_period(right) {
        let month_count = |period: Period| {
            let start = period.start();
            let end = period.end();
            (end.year() - start.year()) * 12 + end.month() as i32 - start.month() as i32 + 1
        };
        month_count(left) == month_count(right)
    } else {
        (left.end() - left.start()).num_days() == (right.end() - right.start()).num_days()
    }
}

/// Compare two reports line by line.
pub fn compare_reports(
    current: &Report,
    previous: &Report,
    direction: DiffDirection,
    mode: DiffMode,
) -> ComparisonReport {
    // Build a map of label -> (current amounts, previous amounts)
    let mut map: BTreeMap<String, (usize, Amounts, Amounts)> = BTreeMap::new();
    let mut labels = Vec::new();
    for line in current.lines() {
        let values = map.entry(line.label.clone()).or_insert_with(|| {
            labels.push(line.label.clone());
            (line.depth, Amounts::new(), Amounts::new())
        });
        values.1.merge(&line.amounts);
    }
    for line in previous.lines() {
        let values = map.entry(line.label.clone()).or_insert_with(|| {
            labels.push(line.label.clone());
            (line.depth, Amounts::new(), Amounts::new())
        });
        values.2.merge(&line.amounts);
    }
    let lines: Vec<ComparisonLine> = labels
        .into_iter()
        .filter_map(|label| {
            map.remove(&label)
                .map(|(depth, current, previous)| ComparisonLine {
                    label,
                    depth,
                    current,
                    previous,
                })
        })
        .collect();
    // Use current scope periods if available
    let current_period = match current.scope {
        ReportScope::Over(p) => p,
        ReportScope::AsOf(r) => Period::single(r.date()),
    };
    let previous_period = match previous.scope {
        ReportScope::Over(p) => p,
        ReportScope::AsOf(r) => Period::single(r.date()),
    };
    ComparisonReport {
        title: current.title.clone(),
        current_period,
        previous_period,
        lines,
        totals: ComparisonLine {
            label: "Total".to_string(),
            depth: 0,
            current: current.totals.clone(),
            previous: previous.totals.clone(),
        },
        direction,
        mode,
    }
}

fn compute_diff(
    cur: Decimal,
    prev: Decimal,
    direction: DiffDirection,
    mode: DiffMode,
) -> Option<Decimal> {
    match mode {
        DiffMode::Absolute => {
            let diff = match direction {
                DiffDirection::CurrentMinusPrevious => cur - prev,
                DiffDirection::PreviousMinusCurrent => prev - cur,
            };
            Some(diff)
        }
        DiffMode::Percent => {
            if prev.is_zero() {
                None
            } else {
                let diff = match direction {
                    DiffDirection::CurrentMinusPrevious => (cur - prev) / prev * Decimal::from(100),
                    DiffDirection::PreviousMinusCurrent => (prev - cur) / prev * Decimal::from(100),
                };
                Some(diff)
            }
        }
    }
}

fn render_text(report: &ComparisonReport) -> String {
    let difference_header = match report.direction {
        DiffDirection::CurrentMinusPrevious => "Difference (current − previous)",
        DiffDirection::PreviousMinusCurrent => "Difference (previous − current)",
    };
    let mut rows = Vec::new();
    for line in report.lines.iter().chain(std::iter::once(&report.totals)) {
        let commodities = commodities(&line.current, &line.previous);
        if commodities.is_empty() {
            rows.push((
                format!("{}{}", "  ".repeat(line.depth), line.label),
                String::new(),
                String::new(),
                String::new(),
            ));
        }
        for commodity in commodities {
            let current = line.current.get(&commodity).unwrap_or(Decimal::ZERO);
            let previous = line.previous.get(&commodity).unwrap_or(Decimal::ZERO);
            let difference =
                format_difference(current, previous, &commodity, report.direction, report.mode);
            rows.push((
                format!("{}{}", "  ".repeat(line.depth), line.label),
                format!("{current} {commodity}"),
                format!("{previous} {commodity}"),
                difference,
            ));
        }
    }
    if rows.iter().all(|row| row.1.is_empty() && row.2.is_empty()) {
        return format!("{} (comparison)\nNo entries\n", report.title);
    }

    let mut output = format!(
        "{} ({} to {} vs {} to {})\n",
        report.title,
        report.current_period.start(),
        report.current_period.end(),
        report.previous_period.start(),
        report.previous_period.end()
    );
    let label_width = rows
        .iter()
        .map(|row| row.0.chars().count())
        .max()
        .unwrap_or(0)
        .max("Label".len());
    let current_width = rows.iter().map(|row| row.1.len()).max().unwrap_or(0).max(7);
    let previous_width = rows.iter().map(|row| row.2.len()).max().unwrap_or(0).max(8);
    let difference_width = rows
        .iter()
        .map(|row| row.3.len())
        .max()
        .unwrap_or(0)
        .max(difference_header.len());
    output.push_str(&format!(
        "{:<label_width$}  {:>current_width$}  {:>previous_width$}  {:>difference_width$}\n",
        "Label", "Current", "Previous", difference_header
    ));
    for (label, current, previous, difference) in rows {
        output.push_str(&format!(
            "{label:<label_width$}  {current:>current_width$}  {previous:>previous_width$}  {difference:>difference_width$}\n"
        ));
    }
    output
}

fn commodities(current: &Amounts, previous: &Amounts) -> BTreeSet<CommodityName> {
    current
        .iter()
        .map(|(commodity, _)| commodity.clone())
        .chain(previous.iter().map(|(commodity, _)| commodity.clone()))
        .collect()
}

fn format_difference(
    current: Decimal,
    previous: Decimal,
    commodity: &CommodityName,
    direction: DiffDirection,
    mode: DiffMode,
) -> String {
    match compute_diff(current, previous, direction, mode) {
        Some(value) => match mode {
            DiffMode::Absolute => format!("{value} {commodity}"),
            DiffMode::Percent => format!("{}%", value.round_dp(2)),
        },
        None => "n/a".to_string(),
    }
}

fn render_json(report: &ComparisonReport) -> String {
    use serde_json::json;
    let dir_str = match report.direction {
        DiffDirection::CurrentMinusPrevious => "current_minus_previous",
        DiffDirection::PreviousMinusCurrent => "previous_minus_current",
    };
    let mode_str = match report.mode {
        DiffMode::Absolute => "absolute",
        DiffMode::Percent => "percent",
    };
    let mut lines_json = Vec::new();
    for line in &report.lines {
        let commodities = commodities(&line.current, &line.previous);
        if commodities.is_empty() {
            lines_json.push(json!({
                "label": line.label,
                "depth": line.depth,
                "commodity": serde_json::Value::Null,
                "current": serde_json::Value::Null,
                "previous": serde_json::Value::Null,
                "difference": serde_json::Value::Null,
            }));
        }
        for c in &commodities {
            let cur = line.current.get(c).unwrap_or(Decimal::ZERO);
            let prev = line.previous.get(c).unwrap_or(Decimal::ZERO);
            let diff = compute_diff(cur, prev, report.direction, report.mode);
            let diff_json = match (diff, report.mode) {
                (Some(d), DiffMode::Absolute) => json!(d.to_string()),
                (Some(d), DiffMode::Percent) => json!(format!("{}%", d.round_dp(2))),
                (None, _) => serde_json::Value::Null,
            };
            lines_json.push(json!({
                "label": line.label,
                "depth": line.depth,
                "commodity": c.as_str(),
                "current": cur.to_string(),
                "previous": prev.to_string(),
                "difference": diff_json,
            }));
        }
    }
    let mut totals_json = Vec::new();
    let mut commodities: std::collections::BTreeSet<CommodityName> =
        std::collections::BTreeSet::new();
    for (c, _) in report.totals.current.iter() {
        commodities.insert(c.clone());
    }
    for (c, _) in report.totals.previous.iter() {
        commodities.insert(c.clone());
    }
    for c in &commodities {
        let cur = report.totals.current.get(c).unwrap_or(Decimal::ZERO);
        let prev = report.totals.previous.get(c).unwrap_or(Decimal::ZERO);
        let diff = compute_diff(cur, prev, report.direction, report.mode);
        let diff_json = match (diff, report.mode) {
            (Some(d), DiffMode::Absolute) => json!(d.to_string()),
            (Some(d), DiffMode::Percent) => json!(format!("{}%", d.round_dp(2))),
            (None, _) => serde_json::Value::Null,
        };
        totals_json.push(json!({
            "commodity": c.as_str(),
            "current": cur.to_string(),
            "previous": prev.to_string(),
            "difference": diff_json,
        }));
    }
    let value = json!({
        "title": report.title,
        "direction": dir_str,
        "mode": mode_str,
        "current_period": {"start": report.current_period.start().to_string(), "end": report.current_period.end().to_string()},
        "previous_period": {"start": report.previous_period.start().to_string(), "end": report.previous_period.end().to_string()},
        "lines": lines_json,
        "totals": totals_json,
    });
    let mut output = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"));
    output.push('\n');
    output
}

fn render_csv(report: &ComparisonReport) -> String {
    let dir_str = match report.direction {
        DiffDirection::CurrentMinusPrevious => "current_minus_previous",
        DiffDirection::PreviousMinusCurrent => "previous_minus_current",
    };
    let mode_str = match report.mode {
        DiffMode::Absolute => "absolute",
        DiffMode::Percent => "percent",
    };
    let mut rows = vec![
        "report,direction,mode,current_start,current_end,previous_start,previous_end,label,depth,commodity,current,previous,difference".to_string(),
    ];
    for line in &report.lines {
        let commodities = commodities(&line.current, &line.previous);
        if commodities.is_empty() {
            rows.push(format!(
                "{},{},{},{},{},{},{},{},{},,,,",
                csv_escape(&report.title),
                dir_str,
                mode_str,
                report.current_period.start(),
                report.current_period.end(),
                report.previous_period.start(),
                report.previous_period.end(),
                csv_escape(&line.label),
                line.depth,
            ));
        }
        for c in &commodities {
            let cur = line.current.get(c).unwrap_or(Decimal::ZERO);
            let prev = line.previous.get(c).unwrap_or(Decimal::ZERO);
            let diff = compute_diff(cur, prev, report.direction, report.mode);
            let diff_str = match (diff, report.mode) {
                (Some(d), DiffMode::Absolute) => d.to_string(),
                (Some(d), DiffMode::Percent) => format!("{}%", d.round_dp(2)),
                (None, _) => "n/a".to_string(),
            };
            rows.push(format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&report.title),
                dir_str,
                mode_str,
                report.current_period.start(),
                report.current_period.end(),
                report.previous_period.start(),
                report.previous_period.end(),
                csv_escape(&line.label),
                line.depth,
                c.as_str(),
                cur,
                prev,
                csv_escape(&diff_str),
            ));
        }
    }
    // totals
    let mut commodities: std::collections::BTreeSet<CommodityName> =
        std::collections::BTreeSet::new();
    for (c, _) in report.totals.current.iter() {
        commodities.insert(c.clone());
    }
    for (c, _) in report.totals.previous.iter() {
        commodities.insert(c.clone());
    }
    for c in &commodities {
        let cur = report.totals.current.get(c).unwrap_or(Decimal::ZERO);
        let prev = report.totals.previous.get(c).unwrap_or(Decimal::ZERO);
        let diff = compute_diff(cur, prev, report.direction, report.mode);
        let diff_str = match (diff, report.mode) {
            (Some(d), DiffMode::Absolute) => d.to_string(),
            (Some(d), DiffMode::Percent) => format!("{}%", d.round_dp(2)),
            (None, _) => "n/a".to_string(),
        };
        rows.push(format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(&report.title),
            dir_str,
            mode_str,
            report.current_period.start(),
            report.current_period.end(),
            report.previous_period.start(),
            report.previous_period.end(),
            "Total",
            "",
            c.as_str(),
            cur,
            prev,
            csv_escape(&diff_str),
        ));
    }
    let mut output = rows.join("\n");
    output.push('\n');
    output
}

fn csv_escape(field: &str) -> String {
    if field
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn amounts(number: &str) -> Amounts {
        let mut amounts = Amounts::new();
        amounts.add(&crate::models::Amount {
            number: number.parse::<Decimal>().unwrap(),
            commodity: CommodityName::new("EUR").unwrap(),
        });
        amounts
    }

    #[test]
    fn previous_period_shifts_full_calendar_months_as_months() {
        let current = Period::new(date(2026, 2, 1), date(2026, 3, 31)).unwrap();
        let previous = previous_period(current).unwrap();

        assert_eq!(
            previous,
            Period::new(date(2025, 12, 1), date(2026, 1, 31)).unwrap()
        );
        assert!(periods_have_same_length(current, previous));
    }

    #[test]
    fn previous_period_shifts_other_ranges_by_the_same_days() {
        let current = Period::new(date(2026, 3, 10), date(2026, 3, 16)).unwrap();
        let previous = previous_period(current).unwrap();

        assert_eq!(
            previous,
            Period::new(date(2026, 3, 3), date(2026, 3, 9)).unwrap()
        );
    }

    #[test]
    fn comparison_uses_report_totals_and_keeps_line_order_and_depth() {
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let current = Report::new(
            "Expenses",
            ReportScope::Over(period),
            vec![
                super::super::ReportLine::new("Expenses", 0, amounts("20")),
                super::super::ReportLine::new("Expenses:Food", 1, amounts("20")),
            ],
            amounts("20"),
        );
        let previous = Report::new(
            "Expenses",
            ReportScope::Over(period),
            vec![
                super::super::ReportLine::new("Expenses", 0, amounts("10")),
                super::super::ReportLine::new("Expenses:Food", 1, amounts("10")),
            ],
            amounts("10"),
        );

        let comparison = compare_reports(
            &current,
            &previous,
            DiffDirection::CurrentMinusPrevious,
            DiffMode::Absolute,
        );

        assert_eq!(comparison.lines[0].label, "Expenses");
        assert_eq!(comparison.lines[1].depth, 1);
        assert_eq!(comparison.totals.current, amounts("20"));
        assert_eq!(comparison.totals.previous, amounts("10"));
        assert!(
            comparison
                .render(ReportFormat::Text)
                .contains("Difference (current − previous)")
        );
    }

    #[test]
    fn reversed_percent_difference_uses_previous_as_base_and_zero_is_null() {
        assert_eq!(
            compute_diff(
                Decimal::new(75, 0),
                Decimal::new(100, 0),
                DiffDirection::PreviousMinusCurrent,
                DiffMode::Percent,
            ),
            Some(Decimal::new(25, 0))
        );
        assert_eq!(
            compute_diff(
                Decimal::new(75, 0),
                Decimal::ZERO,
                DiffDirection::CurrentMinusPrevious,
                DiffMode::Percent,
            ),
            None
        );
    }

    #[test]
    fn json_represents_a_zero_previous_percent_difference_as_null() {
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = ComparisonReport {
            title: "Expenses".to_string(),
            current_period: period,
            previous_period: period,
            lines: vec![ComparisonLine {
                label: "Expenses:Food".to_string(),
                depth: 1,
                current: amounts("5"),
                previous: Amounts::new(),
            }],
            totals: ComparisonLine {
                label: "Total".to_string(),
                depth: 0,
                current: amounts("5"),
                previous: Amounts::new(),
            },
            direction: DiffDirection::CurrentMinusPrevious,
            mode: DiffMode::Percent,
        };

        let value: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Json)).unwrap();
        assert_eq!(value["lines"][0]["difference"], serde_json::Value::Null);
        assert_eq!(value["totals"][0]["difference"], serde_json::Value::Null);
    }
}
