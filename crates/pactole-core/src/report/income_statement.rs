use std::collections::{BTreeMap, BTreeSet};

use crate::models::{Amount, Journal};

use super::balance_sheet::{Category, root, section_lines};
use super::grouping::{TransactionGrouping, for_each_grouped_posting, keyed_lines};
use super::{
    AccountBalances, AccountClassification, AccountGrouping, Amounts, Period, Report, ReportScope,
    ReportSection,
};

/// How income and expenses are grouped on an income statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomeGrouping {
    /// Group by account according to the standard hierarchy rules.
    Account(AccountGrouping),
    /// Group by payee of the transaction.
    Payee,
    /// Group by tag of the transaction.
    Tag,
}

/// Builds the income statement of `journal` over `period`.
///
/// Only postings whose account root is classified as income or expense are
/// included; other accounts are ignored. Income appears as a positive amount
/// and expenses appear as a positive amount. The report total is the net
/// result per commodity (`income − expenses`).
pub fn income_statement(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    grouping: IncomeGrouping,
) -> Report {
    match grouping {
        IncomeGrouping::Account(grouping) => {
            let balances = AccountBalances::from_journal(journal, ReportScope::Over(period));
            let direct_roots: BTreeSet<String> = balances
                .iter()
                .filter(|(account, amounts)| {
                    !amounts.is_zero() && account.as_str() == root(account)
                })
                .map(|(account, _)| account.as_str().to_string())
                .collect();

            let mut income_balances = AccountBalances::new();
            let mut expense_balances = AccountBalances::new();
            for (account, amounts) in balances.iter() {
                match classification.category(root(account)) {
                    Category::Income => {
                        let mut displayed = amounts.clone();
                        displayed.negate();
                        income_balances.add_amounts(account, &displayed);
                    }
                    Category::Expenses => {
                        expense_balances.add_amounts(account, amounts);
                    }
                    _ => {}
                }
            }

            let sections = vec![
                ReportSection::new(
                    "Income",
                    section_lines(&income_balances, grouping, &direct_roots),
                    income_balances.totals(),
                ),
                ReportSection::new(
                    "Expenses",
                    section_lines(&expense_balances, grouping, &direct_roots),
                    expense_balances.totals(),
                ),
            ];

            let mut net = income_balances.totals();
            let exp = expense_balances.totals();
            for (commodity, amount) in exp.iter() {
                net.add(&Amount {
                    number: -amount,
                    commodity: commodity.clone(),
                });
            }
            let report =
                Report::from_sections("Income statement", ReportScope::Over(period), sections, net);
            report.with_total_label("Net result")
        }
        IncomeGrouping::Payee => {
            grouped_income_statement(journal, period, classification, TransactionGrouping::Payee)
        }
        IncomeGrouping::Tag => {
            grouped_income_statement(journal, period, classification, TransactionGrouping::Tag)
        }
    }
}

fn grouped_income_statement(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    grouping: TransactionGrouping,
) -> Report {
    let mut income_lines = BTreeMap::new();
    let mut expense_lines = BTreeMap::new();
    let mut income_subtotal = Amounts::new();
    let mut expense_subtotal = Amounts::new();

    for_each_grouped_posting(
        journal,
        period,
        classification,
        grouping,
        |category, labels, amount| match category {
            Category::Income => {
                let displayed = Amount {
                    number: -amount.number,
                    commodity: amount.commodity.clone(),
                };
                income_subtotal.add(&displayed);
                for label in labels {
                    income_lines
                        .entry(label.clone())
                        .or_insert_with(Amounts::new)
                        .add(&displayed);
                }
            }
            Category::Expenses => {
                expense_subtotal.add(amount);
                for label in labels {
                    expense_lines
                        .entry(label.clone())
                        .or_insert_with(Amounts::new)
                        .add(amount);
                }
            }
            _ => {}
        },
    );

    let sections = vec![
        ReportSection::new("Income", keyed_lines(income_lines), income_subtotal.clone()),
        ReportSection::new(
            "Expenses",
            keyed_lines(expense_lines),
            expense_subtotal.clone(),
        ),
    ];
    let mut net = income_subtotal;
    for (commodity, amount) in expense_subtotal.iter() {
        net.add(&Amount {
            number: -amount,
            commodity: commodity.clone(),
        });
    }
    Report::from_sections("Income statement", ReportScope::Over(period), sections, net)
        .with_total_label("Net result")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AccountName, Amount, CommodityName, Entry, Metadata, Posting, Transaction,
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

    fn eur_amounts(report: &Report, section: &str) -> Option<Decimal> {
        let section = report
            .sections
            .iter()
            .find(|s| s.title.as_deref() == Some(section))?;
        section.subtotal.get(&CommodityName::new("EUR").unwrap())
    }

    fn eur_total(report: &Report) -> Option<Decimal> {
        report.totals.get(&CommodityName::new("EUR").unwrap())
    }

    #[test]
    fn income_and_expenses_split_over_period() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Acme"),
                vec![
                    posting("Assets:Bank", "500.00", "EUR"),
                    posting("Income:Salary", "-500.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 20),
                Some("Shop"),
                vec![
                    posting("Expenses:Rent", "200.00", "EUR"),
                    posting("Assets:Bank", "-200.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Account(AccountGrouping::Exact),
        );
        assert_eq!(report.title, "Income statement");
        assert_eq!(
            eur_amounts(&report, "Income"),
            Some("500.00".parse().unwrap())
        );
        assert_eq!(
            eur_amounts(&report, "Expenses"),
            Some("200.00".parse().unwrap())
        );
        assert_eq!(eur_total(&report), Some("300.00".parse().unwrap()));
    }

    #[test]
    fn ignores_transactions_outside_period() {
        let journal = journal(vec![
            transaction(
                date(2025, 12, 31),
                Some("Old"),
                vec![
                    posting("Assets:Bank", "100.00", "EUR"),
                    posting("Income:Salary", "-100.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 2, 1),
                Some("New"),
                vec![
                    posting("Assets:Bank", "50.00", "EUR"),
                    posting("Income:Salary", "-50.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Account(AccountGrouping::Exact),
        );
        assert!(
            eur_amounts(&report, "Income").is_none()
                || eur_amounts(&report, "Income").unwrap() == "0".parse::<Decimal>().unwrap()
        );
    }

    #[test]
    fn classifies_expenses_with_configured_roots() {
        let journal = journal(vec![transaction(
            date(2026, 1, 10),
            Some("Impots"),
            vec![
                posting("Impots:Revenu", "200.00", "EUR"),
                posting("Actifs:Banque", "-200.00", "EUR"),
            ],
            vec![],
        )]);
        let classification = AccountClassification {
            assets: vec!["Actifs".to_string()],
            liabilities: vec!["Passif".to_string()],
            equity: vec!["Capitaux".to_string()],
            income: vec!["Revenus".to_string()],
            expenses: vec!["Depenses".to_string(), "Impots".to_string()],
        };
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &classification,
            IncomeGrouping::Account(AccountGrouping::Exact),
        );
        assert_eq!(
            eur_amounts(&report, "Expenses"),
            Some("200.00".parse().unwrap())
        );
    }

    #[test]
    fn groups_by_payee() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Acme"),
                vec![
                    posting("Assets:Bank", "500.00", "EUR"),
                    posting("Income:Salary", "-500.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 20),
                Some("Shop"),
                vec![
                    posting("Expenses:Rent", "200.00", "EUR"),
                    posting("Assets:Bank", "-200.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Payee,
        );
        let income = report
            .sections
            .iter()
            .find(|s| s.title.as_deref() == Some("Income"))
            .unwrap();
        assert_eq!(income.lines[0].label, "Acme");
    }

    #[test]
    fn groups_by_payee_with_spaces_and_no_payee() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Café du coin"),
                vec![
                    posting("Expenses:Food", "10.00", "EUR"),
                    posting("Assets:Bank", "-10.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 11),
                None,
                vec![
                    posting("Expenses:Snacks", "5.00", "EUR"),
                    posting("Assets:Bank", "-5.00", "EUR"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Payee,
        );
        let expenses = report
            .sections
            .iter()
            .find(|s| s.title.as_deref() == Some("Expenses"))
            .unwrap();
        assert_eq!(expenses.lines.len(), 2);
        assert_eq!(expenses.lines[1].label, "(no payee)");
        assert_eq!(
            eur_amounts(&report, "Expenses"),
            Some("15.00".parse().unwrap())
        );
    }

    #[test]
    fn groups_by_tag_counts_each_tag() {
        let journal = journal(vec![transaction(
            date(2026, 1, 10),
            Some("Shop"),
            vec![
                posting("Expenses:Food", "50.00", "EUR"),
                posting("Assets:Bank", "-50.00", "EUR"),
            ],
            vec!["courses", "quick"],
        )]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Tag,
        );
        let expenses = report
            .sections
            .iter()
            .find(|s| s.title.as_deref() == Some("Expenses"))
            .unwrap();
        assert_eq!(expenses.lines.len(), 2);
        assert_eq!(
            eur_amounts(&report, "Expenses"),
            Some("50.00".parse().unwrap())
        );
    }

    #[test]
    fn keeps_commodities_separate() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 10),
                Some("Acme"),
                vec![
                    posting("Assets:Bank", "10.00", "EUR"),
                    posting("Income:Salary", "-10.00", "EUR"),
                ],
                vec![],
            ),
            transaction(
                date(2026, 1, 11),
                Some("Acme"),
                vec![
                    posting("Assets:Bank", "5.00", "USD"),
                    posting("Income:Salary", "-5.00", "USD"),
                ],
                vec![],
            ),
        ]);
        let period = Period::new(date(2026, 1, 1), date(2026, 1, 31)).unwrap();
        let report = income_statement(
            &journal,
            period,
            &AccountClassification::default(),
            IncomeGrouping::Account(AccountGrouping::Exact),
        );
        assert_eq!(eur_total(&report), Some("10.00".parse().unwrap()));
        assert_eq!(
            report.totals.get(&CommodityName::new("USD").unwrap()),
            Some("5.00".parse().unwrap())
        );
    }
}
