use serde_json::{Value, json};

use super::{Amounts, Report, ReportFormat, ReportScope};

/// Renders `report` in `format`.
pub(super) fn render(report: &Report, format: ReportFormat) -> String {
    match format {
        ReportFormat::Text => text(report),
        ReportFormat::Json => json(report),
        ReportFormat::Csv => csv(report),
    }
}

/// Renders a report as aligned text.
fn text(report: &Report) -> String {
    let mut output = format!("{} ({})\n", report.title, scope_text(report.scope));

    if report.lines.is_empty() && !has_amounts(&report.totals) {
        output.push_str("No entries\n");
        return output;
    }

    let mut rows: Vec<(String, String)> = report
        .lines
        .iter()
        .map(|line| {
            (
                format!("{}{}", "  ".repeat(line.depth), line.label),
                amounts_text(&line.amounts),
            )
        })
        .collect();
    rows.push(("Total".to_string(), amounts_text(&report.totals)));

    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let amount_width = rows
        .iter()
        .map(|(_, amounts)| amounts.chars().count())
        .max()
        .unwrap_or(0);

    for (label, amounts) in rows {
        let row = if amounts.is_empty() {
            label
        } else {
            format!("{label:<label_width$}  {amounts:>amount_width$}")
        };
        output.push_str(row.trim_end());
        output.push('\n');
    }

    output
}

fn has_amounts(amounts: &Amounts) -> bool {
    amounts.iter().any(|(_, number)| !number.is_zero())
}

fn amounts_text(amounts: &Amounts) -> String {
    amounts
        .iter()
        .filter(|(_, number)| !number.is_zero())
        .map(|(commodity, number)| format!("{number} {commodity}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn scope_text(scope: ReportScope) -> String {
    match scope {
        ReportScope::AsOf(reference) => format!("as of {}", reference.date()),
        ReportScope::Over(period) => format!("from {} to {}", period.start(), period.end()),
    }
}

/// Renders a report as JSON.
///
/// Amounts are strings, never JSON numbers: a JSON number may be parsed as a
/// floating-point value and lose the exact decimal representation.
fn json(report: &Report) -> String {
    let lines = report
        .lines
        .iter()
        .map(|line| {
            json!({
                "label": &line.label,
                "depth": line.depth,
                "amounts": amounts_json(&line.amounts),
            })
        })
        .collect::<Vec<_>>();

    let value = json!({
        "title": &report.title,
        "scope": scope_json(report.scope),
        "lines": lines,
        "totals": amounts_json(&report.totals),
    });

    let mut output = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"));
    output.push('\n');
    output
}

fn amounts_json(amounts: &Amounts) -> Vec<Value> {
    amounts
        .iter()
        .filter(|(_, number)| !number.is_zero())
        .map(|(commodity, number)| {
            json!({
                "commodity": commodity.as_str(),
                "number": number.to_string(),
            })
        })
        .collect()
}

fn scope_json(scope: ReportScope) -> Value {
    match scope {
        ReportScope::AsOf(reference) => json!({
            "kind": "as_of",
            "date": reference.date().to_string(),
        }),
        ReportScope::Over(period) => json!({
            "kind": "period",
            "start": period.start().to_string(),
            "end": period.end().to_string(),
        }),
    }
}

/// Renders a report as tidy CSV.
///
/// Each line/commodity pair is one row; totals use the `total` kind and leave
/// the label and depth empty. The report title and scope are repeated on
/// every row so a single row remains understandable on its own.
fn csv(report: &Report) -> String {
    let mut rows = vec![
        "report,scope_kind,scope_start,scope_end,kind,label,depth,commodity,number".to_string(),
    ];
    let (scope_kind, scope_start, scope_end) = scope_fields(report.scope);

    for line in &report.lines {
        for (commodity, number) in line.amounts.iter().filter(|(_, number)| !number.is_zero()) {
            rows.push(csv_row([
                report.title.clone(),
                scope_kind.to_string(),
                scope_start.clone(),
                scope_end.clone(),
                "line".to_string(),
                line.label.clone(),
                line.depth.to_string(),
                commodity.to_string(),
                number.to_string(),
            ]));
        }
    }

    for (commodity, number) in report.totals.iter().filter(|(_, number)| !number.is_zero()) {
        rows.push(csv_row([
            report.title.clone(),
            scope_kind.to_string(),
            scope_start.clone(),
            scope_end.clone(),
            "total".to_string(),
            String::new(),
            String::new(),
            commodity.to_string(),
            number.to_string(),
        ]));
    }

    let mut output = rows.join("\n");
    output.push('\n');
    output
}

fn scope_fields(scope: ReportScope) -> (&'static str, String, String) {
    match scope {
        ReportScope::AsOf(reference) => ("as_of", String::new(), reference.date().to_string()),
        ReportScope::Over(period) => (
            "period",
            period.start().to_string(),
            period.end().to_string(),
        ),
    }
}

fn csv_row(fields: impl IntoIterator<Item = String>) -> String {
    fields
        .into_iter()
        .map(|field| csv_escape(&field))
        .collect::<Vec<_>>()
        .join(",")
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
    use crate::models::{AccountName, Amount, CommodityName};
    use crate::report::{AccountBalances, AccountGrouping, ReportLine};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn account(name: &str) -> AccountName {
        AccountName::new(name).unwrap()
    }

    fn amount(number: &str, commodity: &str) -> Amount {
        Amount {
            number: number.parse::<Decimal>().unwrap(),
            commodity: CommodityName::new(commodity).unwrap(),
        }
    }

    fn amounts(entries: &[(&str, &str)]) -> Amounts {
        let mut amounts = Amounts::new();
        for (number, commodity) in entries {
            amounts.add(&amount(number, commodity));
        }
        amounts
    }

    fn example_report() -> Report {
        let mut balances = AccountBalances::new();
        balances.add(&account("Actifs:Banque"), &amount("1234.56", "EUR"));
        balances.add(&account("Depenses:Cafe"), &amount("-234.56", "EUR"));

        Report::from_balances(
            "Balance sheet",
            ReportScope::as_of(date(2026, 1, 31)),
            &balances,
            AccountGrouping::Exact,
        )
    }

    #[test]
    fn text_renders_header_lines_and_total() {
        let rendered = example_report().render(ReportFormat::Text);

        assert_eq!(
            rendered,
            concat!(
                "Balance sheet (as of 2026-01-31)\n",
                "Actifs:Banque  1234.56 EUR\n",
                "Depenses:Cafe  -234.56 EUR\n",
                "Total          1000.00 EUR\n",
            )
        );
    }

    #[test]
    fn text_keeps_every_commodity() {
        let report = Report::new(
            "Balance",
            ReportScope::as_of(date(2026, 1, 31)),
            vec![ReportLine::new(
                "Actifs:Multi",
                0,
                amounts(&[("10.00", "EUR"), ("20.00", "USD")]),
            )],
            amounts(&[("10.00", "EUR"), ("20.00", "USD")]),
        );

        let rendered = report.render(ReportFormat::Text);

        assert!(rendered.contains("10.00 EUR, 20.00 USD"));
    }

    #[test]
    fn text_renders_an_empty_report() {
        let report = Report::new(
            "Empty",
            ReportScope::over(date(2026, 1, 1), date(2026, 1, 31)).unwrap(),
            Vec::new(),
            Amounts::new(),
        );

        assert_eq!(
            report.render(ReportFormat::Text),
            "Empty (from 2026-01-01 to 2026-01-31)\nNo entries\n"
        );
    }

    #[test]
    fn json_uses_decimal_strings_and_describes_the_scope() {
        let rendered = example_report().render(ReportFormat::Json);
        let value: Value = serde_json::from_str(&rendered).unwrap();

        assert_eq!(value["title"], "Balance sheet");
        assert_eq!(value["scope"]["kind"], "as_of");
        assert_eq!(value["scope"]["date"], "2026-01-31");
        assert_eq!(value["lines"][0]["label"], "Actifs:Banque");
        assert_eq!(value["lines"][0]["depth"], 0);
        assert_eq!(value["lines"][0]["amounts"][0]["commodity"], "EUR");
        assert_eq!(value["lines"][0]["amounts"][0]["number"], "1234.56");
        assert_eq!(value["totals"][0]["number"], "1000.00");
    }

    #[test]
    fn json_describes_a_period_scope() {
        let report = Report::new(
            "Income",
            ReportScope::over(date(2026, 1, 1), date(2026, 1, 31)).unwrap(),
            Vec::new(),
            Amounts::new(),
        );

        let value: Value = serde_json::from_str(&report.render(ReportFormat::Json)).unwrap();

        assert_eq!(value["scope"]["kind"], "period");
        assert_eq!(value["scope"]["start"], "2026-01-01");
        assert_eq!(value["scope"]["end"], "2026-01-31");
    }

    #[test]
    fn csv_emits_one_row_per_commodity_and_totals() {
        let rendered = example_report().render(ReportFormat::Csv);

        assert_eq!(
            rendered,
            concat!(
                "report,scope_kind,scope_start,scope_end,kind,label,depth,commodity,number\n",
                "Balance sheet,as_of,,2026-01-31,line,Actifs:Banque,0,EUR,1234.56\n",
                "Balance sheet,as_of,,2026-01-31,line,Depenses:Cafe,0,EUR,-234.56\n",
                "Balance sheet,as_of,,2026-01-31,total,,,EUR,1000.00\n",
            )
        );
    }

    #[test]
    fn csv_escapes_commas_quotes_and_newlines() {
        let report = Report::new(
            "A, \"quoted\" title",
            ReportScope::as_of(date(2026, 1, 31)),
            vec![ReportLine::new(
                "Line\nbreak",
                0,
                amounts(&[("1.00", "EUR")]),
            )],
            amounts(&[("1.00", "EUR")]),
        );

        let rendered = report.render(ReportFormat::Csv);

        assert!(rendered.contains("\"A, \"\"quoted\"\" title\""));
        assert!(rendered.contains("\"Line\nbreak\""));
        // The embedded newline stays inside the quoted field: four physical
        // lines for the header, the line row and the total row.
        assert_eq!(rendered.lines().count(), 4);
    }

    #[test]
    fn every_format_renders_the_same_report() {
        let report = example_report();

        for format in ReportFormat::ALL {
            let rendered = report.render(format);
            assert!(rendered.ends_with('\n'));
        }
    }
}
