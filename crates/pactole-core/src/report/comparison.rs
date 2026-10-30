use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;
use rust_decimal::Decimal;

use crate::models::CommodityName;

use super::{Amounts, Period, Report, ReportFormat, ReportScope, Valuation};

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
    /// Title of the source report section, if the line belongs to a named section.
    pub section: Option<String>,
    /// Label of the line.
    pub label: String,
    /// Hierarchical indentation.
    pub depth: usize,
    /// Amounts in the current period.
    pub current: Amounts,
    /// Amounts in the previous period.
    pub previous: Amounts,
}

/// Valuation provenance for each side of a comparison.
///
/// The two reports may come from different journals or use different price
/// contexts, so their provenance is retained independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonValuation {
    /// Valuation applied to the current-period report, if any.
    pub current: Option<Valuation>,
    /// Valuation applied to the previous-period report, if any.
    pub previous: Option<Valuation>,
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
    /// Market-price valuation provenance for both periods, when either was valued.
    pub valuation: Option<ComparisonValuation>,
}

/// Key used to keep equal labels in different report sections distinct.
type ComparisonKey = (Option<String>, String);
/// Accumulated depth, current amounts and previous amounts for a comparison key.
type ComparisonValues = (usize, Amounts, Amounts);

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

/// Returns whether a period covers complete calendar months.
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
    // Keep report sections in the key so equal labels in different sections
    // remain separate comparison lines.
    let mut map: BTreeMap<ComparisonKey, ComparisonValues> = BTreeMap::new();
    let mut labels = Vec::new();
    for section in &current.sections {
        for line in &section.lines {
            let key = (section.title.clone(), line.label.clone());
            let values = map.entry(key.clone()).or_insert_with(|| {
                labels.push(key.clone());
                (line.depth, Amounts::new(), Amounts::new())
            });
            values.1.merge(&line.amounts);
        }
    }
    for section in &previous.sections {
        for line in &section.lines {
            let key = (section.title.clone(), line.label.clone());
            let values = map.entry(key.clone()).or_insert_with(|| {
                labels.push(key.clone());
                (line.depth, Amounts::new(), Amounts::new())
            });
            values.2.merge(&line.amounts);
        }
    }
    let lines: Vec<ComparisonLine> = labels
        .into_iter()
        .filter_map(|(section, label)| {
            map.remove(&(section.clone(), label.clone()))
                .map(|(depth, current, previous)| ComparisonLine {
                    section,
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
            section: None,
            label: "Total".to_string(),
            depth: 0,
            current: current.totals.clone(),
            previous: previous.totals.clone(),
        },
        direction,
        mode,
        valuation: comparison_valuation(current.valuation.as_ref(), previous.valuation.as_ref()),
    }
}

/// Combines valuation provenance from the two compared reports.
fn comparison_valuation(
    current: Option<&Valuation>,
    previous: Option<&Valuation>,
) -> Option<ComparisonValuation> {
    (current.is_some() || previous.is_some()).then(|| ComparisonValuation {
        current: current.cloned(),
        previous: previous.cloned(),
    })
}

/// Computes an absolute or percentage difference between two values.
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

/// Renders a comparison report as an aligned text table.
fn render_text(report: &ComparisonReport) -> String {
    let difference_header = match report.direction {
        DiffDirection::CurrentMinusPrevious => "Difference (current − previous)",
        DiffDirection::PreviousMinusCurrent => "Difference (previous − current)",
    };
    let mut rows = Vec::new();
    let mut active_section: Option<&str> = None;
    for line in &report.lines {
        if let Some(section) = line.section.as_deref() {
            if active_section != Some(section) {
                rows.push((
                    section.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                ));
                active_section = Some(section);
            }
        } else {
            active_section = None;
        }
        let commodities = commodities(&line.current, &line.previous);
        let indent = line.depth + if line.section.is_some() { 1 } else { 0 };
        if commodities.is_empty() {
            rows.push((
                format!("{}{}", "  ".repeat(indent), line.label),
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
                format!("{}{}", "  ".repeat(indent), line.label),
                format!("{current} {commodity}"),
                format!("{previous} {commodity}"),
                difference,
            ));
        }
    }
    let line = &report.totals;
    for commodity in commodities(&line.current, &line.previous) {
        let current = line.current.get(&commodity).unwrap_or(Decimal::ZERO);
        let previous = line.previous.get(&commodity).unwrap_or(Decimal::ZERO);
        let difference =
            format_difference(current, previous, &commodity, report.direction, report.mode);
        rows.push((
            line.label.clone(),
            format!("{current} {commodity}"),
            format!("{previous} {commodity}"),
            difference,
        ));
    }
    if commodities(&line.current, &line.previous).is_empty() {
        rows.push((
            line.label.clone(),
            String::new(),
            String::new(),
            String::new(),
        ));
    }
    if rows.iter().all(|row| row.1.is_empty() && row.2.is_empty()) {
        let mut output = format!("{} (comparison)\n", report.title);
        if let Some(valuation) = &report.valuation {
            output.push_str(&comparison_valuation_text(valuation));
        }
        output.push_str("No entries\n");
        return output;
    }

    let mut output = format!(
        "{} ({} to {} vs {} to {})\n",
        report.title,
        report.current_period.start(),
        report.current_period.end(),
        report.previous_period.start(),
        report.previous_period.end()
    );
    if let Some(valuation) = &report.valuation {
        output.push_str(&comparison_valuation_text(valuation));
    }
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

/// Returns the union of commodities present in both compared amounts.
fn commodities(current: &Amounts, previous: &Amounts) -> BTreeSet<CommodityName> {
    current
        .iter()
        .map(|(commodity, _)| commodity.clone())
        .chain(previous.iter().map(|(commodity, _)| commodity.clone()))
        .collect()
}

/// Formats one absolute or percentage difference for display.
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

/// Renders a comparison report as structured JSON.
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
    let has_sections = report.lines.iter().any(|line| line.section.is_some());
    for line in &report.lines {
        let commodities = commodities(&line.current, &line.previous);
        if commodities.is_empty() {
            let mut value = json!({
                "label": line.label,
                "depth": line.depth,
                "commodity": serde_json::Value::Null,
                "current": serde_json::Value::Null,
                "previous": serde_json::Value::Null,
                "difference": serde_json::Value::Null,
            });
            if has_sections {
                value["section"] = line
                    .section
                    .as_ref()
                    .map_or(serde_json::Value::Null, |section| json!(section));
            }
            lines_json.push(value);
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
            let mut value = json!({
                "label": line.label,
                "depth": line.depth,
                "commodity": c.as_str(),
                "current": cur.to_string(),
                "previous": prev.to_string(),
                "difference": diff_json,
            });
            if has_sections {
                value["section"] = line
                    .section
                    .as_ref()
                    .map_or(serde_json::Value::Null, |section| json!(section));
            }
            lines_json.push(value);
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
    let mut value = json!({
        "title": report.title,
        "direction": dir_str,
        "mode": mode_str,
        "current_period": {"start": report.current_period.start().to_string(), "end": report.current_period.end().to_string()},
        "previous_period": {"start": report.previous_period.start().to_string(), "end": report.previous_period.end().to_string()},
        "lines": lines_json,
        "totals": totals_json,
    });
    if let Some(valuation) = &report.valuation {
        value["valuation"] = comparison_valuation_json(valuation);
    }
    let mut output = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"));
    output.push('\n');
    output
}

/// Renders a comparison report as CSV rows.
fn render_csv(report: &ComparisonReport) -> String {
    let dir_str = match report.direction {
        DiffDirection::CurrentMinusPrevious => "current_minus_previous",
        DiffDirection::PreviousMinusCurrent => "previous_minus_current",
    };
    let mode_str = match report.mode {
        DiffMode::Absolute => "absolute",
        DiffMode::Percent => "percent",
    };
    let has_sections = report.lines.iter().any(|line| line.section.is_some());
    let mut rows = vec![if has_sections {
        "report,direction,mode,current_start,current_end,previous_start,previous_end,section,label,depth,commodity,current,previous,difference".to_string()
    } else {
        "report,direction,mode,current_start,current_end,previous_start,previous_end,label,depth,commodity,current,previous,difference".to_string()
    }];
    for line in &report.lines {
        let commodities = commodities(&line.current, &line.previous);
        if commodities.is_empty() {
            if has_sections {
                rows.push(format!(
                    "{},{},{},{},{},{},{},{},{},{},,,,",
                    csv_escape(&report.title),
                    dir_str,
                    mode_str,
                    report.current_period.start(),
                    report.current_period.end(),
                    report.previous_period.start(),
                    report.previous_period.end(),
                    csv_escape(line.section.as_deref().unwrap_or("")),
                    csv_escape(&line.label),
                    line.depth,
                ));
            } else {
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
            if has_sections {
                rows.push(format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&report.title),
                    dir_str,
                    mode_str,
                    report.current_period.start(),
                    report.current_period.end(),
                    report.previous_period.start(),
                    report.previous_period.end(),
                    csv_escape(line.section.as_deref().unwrap_or("")),
                    csv_escape(&line.label),
                    line.depth,
                    c.as_str(),
                    cur,
                    prev,
                    csv_escape(&diff_str),
                ));
            } else {
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
        if has_sections {
            rows.push(format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(&report.title),
                dir_str,
                mode_str,
                report.current_period.start(),
                report.current_period.end(),
                report.previous_period.start(),
                report.previous_period.end(),
                "",
                "Total",
                "",
                c.as_str(),
                cur,
                prev,
                csv_escape(&diff_str),
            ));
        } else {
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
    }
    if let Some(valuations) = &report.valuation {
        for (side, valuation) in [
            ("Current", valuations.current.as_ref()),
            ("Previous", valuations.previous.as_ref()),
        ] {
            let Some(valuation) = valuation else {
                continue;
            };
            let label = format!(
                "{side} valuation: {} at {}",
                valuation.currency, valuation.date
            );
            if has_sections {
                rows.push(format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&report.title),
                    dir_str,
                    mode_str,
                    report.current_period.start(),
                    report.current_period.end(),
                    report.previous_period.start(),
                    report.previous_period.end(),
                    "",
                    csv_escape(&label),
                    "",
                    valuation.currency,
                    "",
                    "",
                    ""
                ));
            } else {
                rows.push(format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    csv_escape(&report.title),
                    dir_str,
                    mode_str,
                    report.current_period.start(),
                    report.current_period.end(),
                    report.previous_period.start(),
                    report.previous_period.end(),
                    csv_escape(&label),
                    "",
                    valuation.currency,
                    "",
                    "",
                    ""
                ));
            }
            for price in &valuation.prices {
                let label = format!(
                    "{side} price {} → {} at {}",
                    price.base, price.quote, price.date
                );
                if has_sections {
                    rows.push(format!(
                        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        csv_escape(&report.title),
                        dir_str,
                        mode_str,
                        report.current_period.start(),
                        report.current_period.end(),
                        report.previous_period.start(),
                        report.previous_period.end(),
                        "",
                        csv_escape(&label),
                        "",
                        price.quote,
                        price.rate,
                        "",
                        ""
                    ));
                } else {
                    rows.push(format!(
                        "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        csv_escape(&report.title),
                        dir_str,
                        mode_str,
                        report.current_period.start(),
                        report.current_period.end(),
                        report.previous_period.start(),
                        report.previous_period.end(),
                        csv_escape(&label),
                        "",
                        price.quote,
                        price.rate,
                        "",
                        ""
                    ));
                }
            }
            for commodity in &valuation.missing {
                let label = format!(
                    "{side} missing price {} → {}",
                    commodity, valuation.currency
                );
                if has_sections {
                    rows.push(format!(
                        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        csv_escape(&report.title),
                        dir_str,
                        mode_str,
                        report.current_period.start(),
                        report.current_period.end(),
                        report.previous_period.start(),
                        report.previous_period.end(),
                        "",
                        csv_escape(&label),
                        "",
                        commodity,
                        "",
                        "",
                        ""
                    ));
                } else {
                    rows.push(format!(
                        "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        csv_escape(&report.title),
                        dir_str,
                        mode_str,
                        report.current_period.start(),
                        report.current_period.end(),
                        report.previous_period.start(),
                        report.previous_period.end(),
                        csv_escape(&label),
                        "",
                        commodity,
                        "",
                        "",
                        ""
                    ));
                }
            }
        }
    }
    let mut output = rows.join("\n");
    output.push('\n');
    output
}

/// Serializes valuation provenance for both comparison periods.
fn comparison_valuation_json(valuation: &ComparisonValuation) -> serde_json::Value {
    serde_json::json!({
        "current": valuation.current.as_ref().map(valuation_json),
        "previous": valuation.previous.as_ref().map(valuation_json),
    })
}

/// Serializes one report valuation into the comparison JSON shape.
fn valuation_json(valuation: &Valuation) -> serde_json::Value {
    serde_json::json!({
        "currency": valuation.currency.as_str(),
        "date": valuation.date.to_string(),
        "prices": valuation.prices.iter().map(|price| serde_json::json!({
            "base": price.base.as_str(),
            "quote": price.quote.as_str(),
            "rate": price.rate.to_string(),
            "date": price.date.to_string(),
        })).collect::<Vec<_>>(),
        "missing": valuation.missing.iter().map(|commodity| commodity.as_str()).collect::<Vec<_>>(),
    })
}

/// Formats valuation provenance for both comparison periods.
fn comparison_valuation_text(valuation: &ComparisonValuation) -> String {
    let mut output = String::new();
    for (label, valuation) in [
        ("Current", valuation.current.as_ref()),
        ("Previous", valuation.previous.as_ref()),
    ] {
        let Some(valuation) = valuation else {
            continue;
        };
        output.push_str(&format!(
            "{label} valuation: {} at {}\n",
            valuation.currency, valuation.date
        ));
        for price in &valuation.prices {
            output.push_str(&format!(
                "  Price: {} → {} = {} ({})\n",
                price.base, price.quote, price.rate, price.date
            ));
        }
        for commodity in &valuation.missing {
            output.push_str(&format!(
                "  Missing price: {} → {}; amount remains in {}\n",
                commodity, valuation.currency, commodity
            ));
        }
    }
    output
}

/// Quotes a comparison CSV field when it contains special characters.
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
    fn comparison_keeps_equal_labels_in_distinct_sections_separate() {
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let current = Report::from_sections(
            "Income statement",
            ReportScope::Over(period),
            vec![
                super::super::ReportSection::new(
                    "Income",
                    vec![super::super::ReportLine::new("Shop", 0, amounts("100"))],
                    amounts("100"),
                ),
                super::super::ReportSection::new(
                    "Expenses",
                    vec![super::super::ReportLine::new("Shop", 0, amounts("20"))],
                    amounts("20"),
                ),
            ],
            amounts("80"),
        );
        let previous = Report::from_sections(
            "Income statement",
            ReportScope::Over(period),
            vec![
                super::super::ReportSection::new(
                    "Income",
                    vec![super::super::ReportLine::new("Shop", 0, amounts("90"))],
                    amounts("90"),
                ),
                super::super::ReportSection::new(
                    "Expenses",
                    vec![super::super::ReportLine::new("Shop", 0, amounts("10"))],
                    amounts("10"),
                ),
            ],
            amounts("80"),
        );

        let comparison = compare_reports(
            &current,
            &previous,
            DiffDirection::CurrentMinusPrevious,
            DiffMode::Absolute,
        );

        assert_eq!(comparison.lines.len(), 2);
        assert_eq!(comparison.lines[0].section.as_deref(), Some("Income"));
        assert_eq!(comparison.lines[0].current, amounts("100"));
        assert_eq!(comparison.lines[0].previous, amounts("90"));
        assert_eq!(comparison.lines[1].section.as_deref(), Some("Expenses"));
        assert_eq!(comparison.lines[1].current, amounts("20"));
        assert_eq!(comparison.lines[1].previous, amounts("10"));

        let json: serde_json::Value =
            serde_json::from_str(&comparison.render(ReportFormat::Json)).unwrap();
        assert_eq!(json["lines"][0]["section"], "Income");
        assert_eq!(json["lines"][1]["section"], "Expenses");
        assert_eq!(json["lines"][0]["current"], "100");
        assert_eq!(json["lines"][1]["current"], "20");
        assert!(
            comparison
                .render(ReportFormat::Text)
                .lines()
                .any(|line| line.trim() == "Income")
        );
        let csv = comparison.render(ReportFormat::Csv);
        let csv_rows: Vec<_> = csv
            .lines()
            .map(|line| line.split(',').collect::<Vec<_>>())
            .collect();
        assert!(csv_rows[0].contains(&"section"));
        assert_eq!(csv_rows[1][7], "Income");
        assert_eq!(csv_rows[1][8], "Shop");
        assert_eq!(csv_rows[1][11], "100");
        assert_eq!(csv_rows[2][7], "Expenses");
        assert_eq!(csv_rows[2][11], "20");
    }

    #[test]
    fn comparison_retains_valuation_provenance_for_each_period() {
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let mut current = Report::new("Expenses", ReportScope::Over(period), vec![], amounts("20"));
        current.valuation = Some(Valuation {
            currency: CommodityName::new("EUR").unwrap(),
            date: date(2026, 1, 31),
            prices: vec![],
            missing: vec![CommodityName::new("GBP").unwrap()],
        });
        let mut previous =
            Report::new("Expenses", ReportScope::Over(period), vec![], amounts("10"));
        previous.valuation = Some(Valuation {
            currency: CommodityName::new("USD").unwrap(),
            date: date(2026, 1, 15),
            prices: vec![],
            missing: vec![CommodityName::new("CAD").unwrap()],
        });

        let comparison = compare_reports(
            &current,
            &previous,
            DiffDirection::CurrentMinusPrevious,
            DiffMode::Absolute,
        );

        let provenance = comparison.valuation.as_ref().unwrap();
        assert_eq!(
            provenance.current.as_ref().unwrap().currency.as_str(),
            "EUR"
        );
        assert_eq!(
            provenance.previous.as_ref().unwrap().currency.as_str(),
            "USD"
        );
        let json: serde_json::Value =
            serde_json::from_str(&comparison.render(ReportFormat::Json)).unwrap();
        assert_eq!(json["valuation"]["current"]["missing"][0], "GBP");
        assert_eq!(json["valuation"]["previous"]["missing"][0], "CAD");
        let text = comparison.render(ReportFormat::Text);
        assert!(text.contains("Current valuation: EUR at 2026-01-31"));
        assert!(text.contains("Previous valuation: USD at 2026-01-15"));
        let csv = comparison.render(ReportFormat::Csv);
        assert!(csv.contains("Current valuation: EUR at 2026-01-31"));
        assert!(csv.contains("Previous valuation: USD at 2026-01-15"));
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
                section: None,
                label: "Expenses:Food".to_string(),
                depth: 1,
                current: amounts("5"),
                previous: Amounts::new(),
            }],
            totals: ComparisonLine {
                section: None,
                label: "Total".to_string(),
                depth: 0,
                current: amounts("5"),
                previous: Amounts::new(),
            },
            direction: DiffDirection::CurrentMinusPrevious,
            mode: DiffMode::Percent,
            valuation: None,
        };

        let value: serde_json::Value =
            serde_json::from_str(&report.render(ReportFormat::Json)).unwrap();
        assert_eq!(value["lines"][0]["difference"], serde_json::Value::Null);
        assert_eq!(value["totals"][0]["difference"], serde_json::Value::Null);
    }
}
