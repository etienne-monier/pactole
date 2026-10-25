use std::collections::{BTreeMap, BTreeSet};

use chrono::Datelike;

use crate::models::{AccountName, Entry, Journal};

use super::balance_sheet::{Category, root, section_lines};
use super::grouping::{TransactionGrouping, for_each_grouped_posting, keyed_lines};
use super::{
    AccountBalances, AccountClassification, AccountGrouping, Amounts, Period, Report, ReportLine,
    ReportScope, ReportSection,
};

/// How expenses are grouped on an expenses breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpenseGrouping {
    /// Group by account according to the standard hierarchy rules.
    Account(AccountGrouping),
    /// Group by payee of the transaction.
    Payee,
    /// Group by tag of the transaction.
    Tag,
    /// Group by month of the transaction (`YYYY-MM`).
    Month,
}

/// Options for the expenses breakdown.
#[derive(Debug, Clone, Default)]
pub struct ExpenseOptions {
    /// Accounts considered uncategorized (exact or by prefix).
    pub uncategorized: Vec<AccountName>,
    /// Include an "Uncategorized" section.
    pub include_uncategorized: bool,
    /// Include an "Inactive accounts" section.
    pub include_inactive: bool,
}

fn is_uncategorized_account(account: &AccountName, uncategorized: &[AccountName]) -> bool {
    let account_str = account.as_str();
    for unc in uncategorized {
        let unc_str = unc.as_str();
        if account_str == unc_str {
            return true;
        }
        if account_str.starts_with(&format!("{unc_str}:")) {
            return true;
        }
    }
    false
}

/// Builds the expenses breakdown of `journal` over `period`.
///
/// Only postings whose account root is classified as expense are included.
/// Amounts keep their accounting sign (a refund appears as a negative amount).
/// The report total is the period's spending per commodity.
pub fn expense_breakdown(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    grouping: ExpenseGrouping,
) -> Report {
    expense_breakdown_with_options(
        journal,
        period,
        classification,
        grouping,
        &ExpenseOptions::default(),
    )
}

/// Builds the expenses breakdown with additional options.
pub fn expense_breakdown_with_options(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    grouping: ExpenseGrouping,
    options: &ExpenseOptions,
) -> Report {
    let mut report = match grouping {
        ExpenseGrouping::Account(grouping) => {
            let balances = AccountBalances::from_journal(journal, ReportScope::Over(period));
            let direct_roots: BTreeSet<String> = balances
                .iter()
                .filter(|(account, amounts)| {
                    !amounts.is_zero() && account.as_str() == root(account)
                })
                .map(|(account, _)| account.as_str().to_string())
                .collect();

            let mut expense_balances = AccountBalances::new();
            for (account, amounts) in balances.iter() {
                if matches!(classification.category(root(account)), Category::Expenses) {
                    expense_balances.add_amounts(account, amounts);
                }
            }

            let expense_lines = section_lines(&expense_balances, grouping, &direct_roots);
            let totals = expense_balances.totals();

            Report::new("Expenses", ReportScope::Over(period), expense_lines, totals)
        }
        ExpenseGrouping::Payee => {
            let mut lines_map: BTreeMap<String, Amounts> = BTreeMap::new();
            let mut subtotal = Amounts::new();
            for_each_grouped_posting(
                journal,
                period,
                classification,
                TransactionGrouping::Payee,
                |category, labels, amount| {
                    if matches!(category, Category::Expenses) {
                        subtotal.add(amount);
                        lines_map.entry(labels[0].clone()).or_default().add(amount);
                    }
                },
            );
            let report_lines = keyed_lines(lines_map);
            Report::new(
                "Expenses",
                ReportScope::Over(period),
                report_lines,
                subtotal,
            )
        }
        ExpenseGrouping::Tag => {
            let mut lines_map: BTreeMap<String, Amounts> = BTreeMap::new();
            let mut subtotal = Amounts::new();
            for_each_grouped_posting(
                journal,
                period,
                classification,
                TransactionGrouping::Tag,
                |category, labels, amount| {
                    if matches!(category, Category::Expenses) {
                        subtotal.add(amount);
                        for label in labels {
                            lines_map.entry(label.clone()).or_default().add(amount);
                        }
                    }
                },
            );
            let report_lines = keyed_lines(lines_map);
            Report::new(
                "Expenses",
                ReportScope::Over(period),
                report_lines,
                subtotal,
            )
        }
        ExpenseGrouping::Month => {
            let mut lines_map: BTreeMap<String, Amounts> = BTreeMap::new();
            let mut subtotal = Amounts::new();
            let mut y = period.start().year();
            let mut m = period.start().month();
            let end_y = period.end().year();
            let end_m = period.end().month();
            loop {
                let key = format!("{}-{:02}", y, m);
                lines_map.entry(key).or_default();
                if y == end_y && m == end_m {
                    break;
                }
                if m == 12 {
                    m = 1;
                    y += 1;
                } else {
                    m += 1;
                }
            }
            for entry in journal.entries() {
                let Entry::Transaction(transaction) = entry else {
                    continue;
                };
                if !period.contains(transaction.date) {
                    continue;
                }
                let key = format!(
                    "{}-{:02}",
                    transaction.date.year(),
                    transaction.date.month()
                );
                for posting in &transaction.postings {
                    let Some(amount) = &posting.amount else {
                        continue;
                    };
                    if matches!(
                        classification.category(root(&posting.account)),
                        Category::Expenses
                    ) {
                        subtotal.add(amount);
                        if let Some(entry_map) = lines_map.get_mut(&key) {
                            entry_map.add(amount);
                        }
                    }
                }
            }
            let mut report_lines: Vec<ReportLine> = lines_map
                .into_iter()
                .map(|(label, amounts)| ReportLine::new(label, 0, amounts))
                .collect();
            report_lines.sort_by(|a, b| a.label.cmp(&b.label));
            Report::new(
                "Expenses",
                ReportScope::Over(period),
                report_lines,
                subtotal,
            )
        }
    };

    append_uncategorized_section(journal, period, options, &mut report);
    append_inactive_section(journal, period, classification, options, &mut report);
    report
}

fn append_uncategorized_section(
    journal: &Journal,
    period: Period,
    options: &ExpenseOptions,
    report: &mut Report,
) {
    if !options.include_uncategorized || options.uncategorized.is_empty() {
        return;
    }

    let mut lines = Vec::new();
    let mut subtotal = Amounts::new();
    for entry in journal.entries() {
        let Entry::Transaction(transaction) = entry else {
            continue;
        };
        if !period.contains(transaction.date) {
            continue;
        }
        let mut amounts = Amounts::new();
        for posting in &transaction.postings {
            if is_uncategorized_account(&posting.account, &options.uncategorized)
                && let Some(amount) = &posting.amount
            {
                amounts.add(amount);
            }
        }
        if amounts.is_zero() {
            continue;
        }
        let label = format!(
            "{} {}{}",
            transaction.date,
            transaction.payee.as_deref().unwrap_or("(no payee)"),
            transaction
                .narration
                .as_deref()
                .map(|narration| format!(" — {narration}"))
                .unwrap_or_default()
        );
        subtotal.merge(&amounts);
        lines.push(ReportLine::new(label, 0, amounts));
    }
    report
        .sections
        .push(ReportSection::new("Uncategorized", lines, subtotal));
}

fn append_inactive_section(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    options: &ExpenseOptions,
    report: &mut Report,
) {
    if !options.include_inactive {
        return;
    }

    let mut opened = BTreeMap::new();
    let mut closed = BTreeMap::new();
    for entry in journal.entries() {
        match entry {
            Entry::Open(open)
                if matches!(
                    classification.category(root(&open.account)),
                    Category::Expenses
                ) =>
            {
                opened.insert(open.account.clone(), open.date);
            }
            Entry::Close(close) => {
                closed.insert(close.account.clone(), close.date);
            }
            _ => {}
        }
    }

    let mut posted = BTreeSet::new();
    for entry in journal.entries() {
        let Entry::Transaction(transaction) = entry else {
            continue;
        };
        if period.contains(transaction.date) {
            posted.extend(
                transaction
                    .postings
                    .iter()
                    .map(|posting| posting.account.clone()),
            );
        }
    }

    let lines = opened
        .into_iter()
        .filter(|(account, opened_on)| {
            *opened_on <= period.end()
                && closed
                    .get(account)
                    .is_none_or(|closed_on| *closed_on >= period.start())
                && !posted.contains(account)
        })
        .map(|(account, _)| ReportLine::new(account.as_str(), 0, Amounts::new()))
        .collect::<Vec<_>>();
    if !lines.is_empty() {
        report.sections.push(ReportSection::new(
            "Inactive accounts",
            lines,
            Amounts::new(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AccountName, Amount, CommodityName, Entry, Metadata, Open, Posting, Transaction,
        TransactionStatus,
    };
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn account(name: &str) -> AccountName {
        AccountName::new(name).unwrap()
    }

    fn amount(number: &str, commodity_name: &str) -> Amount {
        Amount {
            number: number.parse::<Decimal>().unwrap(),
            commodity: CommodityName::new(commodity_name).unwrap(),
        }
    }

    fn posting(account_name: &str, number: &str, commodity_name: &str) -> Posting {
        Posting {
            account: account(account_name),
            amount: Some(amount(number, commodity_name)),
            meta: Metadata::new(),
        }
    }

    fn transaction(
        d: NaiveDate,
        payee: Option<&str>,
        postings: Vec<Posting>,
        tags: Vec<&str>,
    ) -> Entry {
        Entry::Transaction(Transaction {
            date: d,
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: payee.map(|s| s.to_string()),
            narration: None,
            postings,
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            links: Vec::new(),
            reference: None,
            meta: Metadata::new(),
        })
    }

    fn journal(entries: Vec<Entry>) -> Journal {
        Journal { entries }
    }

    #[test]
    fn expenses_by_account() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Shop"),
                vec![
                    posting("Expenses:Food", "50.00", "EUR"),
                    posting("Assets:Bank", "-50.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 20),
                Some("Landlord"),
                vec![
                    posting("Expenses:Rent", "200.00", "EUR"),
                    posting("Assets:Bank", "-200.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = expense_breakdown(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Account(AccountGrouping::Exact),
        );
        assert_eq!(report.title, "Expenses");
        let eur = report
            .totals
            .get(&CommodityName::new("EUR").unwrap())
            .unwrap();
        assert_eq!(eur, "250.00".parse::<Decimal>().unwrap());
    }

    #[test]
    fn expenses_by_payee() {
        let journal = journal(vec![transaction(
            date(2026, 1, 10),
            Some("Shop"),
            vec![
                posting("Expenses:Food", "50.00", "EUR"),
                posting("Assets:Bank", "-50.00", "EUR"),
            ],
            vec![],
        )]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = expense_breakdown(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Payee,
        );
        assert_eq!(report.lines().next().unwrap().label, "Shop");
    }

    #[test]
    fn expenses_by_tag() {
        let journal = journal(vec![transaction(
            date(2026, 1, 10),
            Some("Shop"),
            vec![
                posting("Expenses:Food", "50.00", "EUR"),
                posting("Assets:Bank", "-50.00", "EUR"),
            ],
            vec!["food", "quick"],
        )]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = expense_breakdown(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Tag,
        );
        assert_eq!(report.lines().count(), 2);
    }

    #[test]
    fn expenses_by_month_includes_empty_months() {
        let journal = journal(vec![transaction(
            date(2026, 1, 10),
            Some("Shop"),
            vec![
                posting("Expenses:Food", "50.00", "EUR"),
                posting("Assets:Bank", "-50.00", "EUR"),
            ],
            vec![],
        )]);
        let period = Period::new(date(2026, 1, 1), date(2026, 3, 31)).unwrap();
        let report = expense_breakdown(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Month,
        );
        let labels: Vec<String> = report.lines().map(|l| l.label.clone()).collect();
        assert_eq!(labels, vec!["2026-01", "2026-02", "2026-03"]);
        assert!(report.lines().nth(1).unwrap().amounts.is_zero());
        assert!(
            report
                .render(super::super::ReportFormat::Csv)
                .contains("Expenses,period,2026-01-01,2026-03-31,,line,2026-02,0,,\n")
        );
    }

    #[test]
    fn optional_sections_are_included_with_payee_grouping() {
        let journal = journal(vec![
            Entry::Open(Open {
                date: date(2026, 1, 1),
                account: account("Expenses:ToReview"),
                commodities: Vec::new(),
                description: None,
                meta: Metadata::new(),
            }),
            Entry::Open(Open {
                date: date(2026, 1, 1),
                account: account("Expenses:Inactive"),
                commodities: Vec::new(),
                description: None,
                meta: Metadata::new(),
            }),
            transaction(
                date(2026, 1, 10),
                Some("Unknown shop"),
                vec![
                    posting("Expenses:ToReview", "12.00", "EUR"),
                    posting("Assets:Bank", "-12.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = expense_breakdown_with_options(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Payee,
            &ExpenseOptions {
                uncategorized: vec![account("Expenses:ToReview")],
                include_uncategorized: true,
                include_inactive: true,
            },
        );

        assert_eq!(report.sections[1].title.as_deref(), Some("Uncategorized"));
        assert_eq!(report.sections[1].lines[0].label, "2026-01-10 Unknown shop");
        assert_eq!(
            report.sections[2].title.as_deref(),
            Some("Inactive accounts")
        );
        assert_eq!(report.sections[2].lines[0].label, "Expenses:Inactive");
        assert!(report.sections[2].lines[0].amounts.is_empty());
    }

    #[test]
    fn uncategorized_prefix_matches_subaccounts_and_keeps_duplicate_transactions() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Unknown shop"),
                vec![
                    posting("Expenses:ToReview:Food", "12.00", "EUR"),
                    posting("Assets:Bank", "-12.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 10),
                Some("Unknown shop"),
                vec![
                    posting("Expenses:ToReview:Food", "3.00", "EUR"),
                    posting("Assets:Bank", "-3.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = expense_breakdown_with_options(
            &journal,
            period,
            &AccountClassification::default(),
            ExpenseGrouping::Tag,
            &ExpenseOptions {
                uncategorized: vec![account("Expenses:ToReview")],
                include_uncategorized: true,
                include_inactive: false,
            },
        );

        assert_eq!(report.sections[1].lines.len(), 2);
        assert_eq!(
            report.sections[1]
                .subtotal
                .get(&CommodityName::new("EUR").unwrap()),
            Some("15.00".parse().unwrap())
        );
    }
}
