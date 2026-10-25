use std::collections::BTreeSet;

use crate::models::{AccountName, Journal};

use super::{
    AccountBalances, AccountGrouping, Amounts, ReferenceDate, Report, ReportLine, ReportScope,
    ReportSection,
};

/// Label of the synthetic line carrying the unclosed result in the equity
/// section of a balance sheet.
const UNCLOSED_RESULT: &str = "Unclosed result";

/// Maps the root segment of an account name to an accounting category.
///
/// Each list holds root names (the first `:`-separated segment of an account),
/// matched exactly and case-sensitively. The default convention is the English
/// set `Assets`, `Liabilities`, `Equity`, `Income` and `Expenses`; a journal
/// that uses another language (or another naming scheme) overrides the lists
/// it needs, without having to configure the others.
///
/// A root may appear in several lists; the first match wins, in the order
/// assets, liabilities, equity, income, expenses. A root matching no list is
/// reported in an `Unclassified` section instead of being dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountClassification {
    /// Roots of the asset accounts.
    pub assets: Vec<String>,
    /// Roots of the liability accounts.
    pub liabilities: Vec<String>,
    /// Roots of the equity accounts.
    pub equity: Vec<String>,
    /// Roots of the income accounts.
    pub income: Vec<String>,
    /// Roots of the expense accounts.
    pub expenses: Vec<String>,
}

impl Default for AccountClassification {
    fn default() -> Self {
        Self {
            assets: vec!["Assets".to_string()],
            liabilities: vec!["Liabilities".to_string()],
            equity: vec!["Equity".to_string()],
            income: vec!["Income".to_string()],
            expenses: vec!["Expenses".to_string()],
        }
    }
}

impl AccountClassification {
    /// Replaces the asset roots.
    pub fn with_assets(mut self, roots: Vec<String>) -> Self {
        self.assets = roots;
        self
    }

    /// Replaces the liability roots.
    pub fn with_liabilities(mut self, roots: Vec<String>) -> Self {
        self.liabilities = roots;
        self
    }

    /// Replaces the equity roots.
    pub fn with_equity(mut self, roots: Vec<String>) -> Self {
        self.equity = roots;
        self
    }

    /// Replaces the income roots.
    pub fn with_income(mut self, roots: Vec<String>) -> Self {
        self.income = roots;
        self
    }

    /// Replaces the expense roots.
    pub fn with_expenses(mut self, roots: Vec<String>) -> Self {
        self.expenses = roots;
        self
    }

    pub(super) fn category(&self, root: &str) -> Category {
        let matches = |roots: &[String]| roots.iter().any(|candidate| candidate == root);

        if matches(&self.assets) {
            Category::Assets
        } else if matches(&self.liabilities) {
            Category::Liabilities
        } else if matches(&self.equity) {
            Category::Equity
        } else if matches(&self.income) {
            Category::Income
        } else if matches(&self.expenses) {
            Category::Expenses
        } else {
            Category::Unclassified
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
    Unclassified,
}

/// Builds the balance sheet of `journal` as of `as_of`.
///
/// The sheet is made of three sections, *Assets*, *Liabilities* and *Equity*,
/// plus an *Unclassified* section when some accounts match no configured root.
/// Liabilities and equity are presented with a positive sign (their credit
/// balance), so a fully classified, balanced journal satisfies
/// `Assets = Liabilities + Equity` per commodity.
///
/// Income and expense accounts are not sections of their own: their net
/// result is added to the equity section as an `Unclosed result` line,
/// unless it is zero.
///
/// The `grouping` controls how accounts are folded into lines. In
/// [`AccountGrouping::Full`] mode, the cumulative line of a section root is
/// hidden (the section heading plays that role) unless the root account itself
/// carries postings; the remaining depths are shifted accordingly.
///
/// The report total is the algebraic sum of the exact ledger balances, so it
/// is zero for a balanced, fully classified journal.
pub fn balance_sheet(
    journal: &Journal,
    as_of: ReferenceDate,
    classification: &AccountClassification,
    grouping: AccountGrouping,
) -> Report {
    let balances = AccountBalances::from_journal(journal, ReportScope::AsOf(as_of));

    // Roots whose exact account carries postings of its own: their cumulative
    // line is meaningful even in `Full` mode.
    let direct_roots: BTreeSet<String> = balances
        .iter()
        .filter(|(account, amounts)| !amounts.is_zero() && account.as_str() == root(account))
        .map(|(account, _)| account.as_str().to_string())
        .collect();

    let mut assets = AccountBalances::new();
    let mut liabilities = AccountBalances::new();
    let mut equity = AccountBalances::new();
    let mut income = AccountBalances::new();
    let mut expenses = AccountBalances::new();
    let mut unclassified = AccountBalances::new();

    for (account, amounts) in balances.iter() {
        let category = classification.category(root(account));
        let target = match category {
            Category::Assets => &mut assets,
            Category::Liabilities => &mut liabilities,
            Category::Equity => &mut equity,
            Category::Income => &mut income,
            Category::Expenses => &mut expenses,
            Category::Unclassified => &mut unclassified,
        };

        let mut displayed = amounts.clone();
        if matches!(category, Category::Liabilities | Category::Equity) {
            displayed.negate();
        }
        target.add_amounts(account, &displayed);
    }

    let mut result = income.totals();
    result.merge(&expenses.totals());
    result.negate();

    let mut sections = vec![
        section("Assets", &assets, grouping, &direct_roots),
        section("Liabilities", &liabilities, grouping, &direct_roots),
        equity_section(&equity, &result, grouping, &direct_roots),
    ];

    if unclassified.iter().any(|(_, amounts)| !amounts.is_zero()) {
        sections.push(section(
            "Unclassified",
            &unclassified,
            grouping,
            &direct_roots,
        ));
    }

    Report::from_sections(
        "Balance sheet",
        ReportScope::AsOf(as_of),
        sections,
        balances.totals(),
    )
}

fn section(
    title: &str,
    balances: &AccountBalances,
    grouping: AccountGrouping,
    direct_roots: &BTreeSet<String>,
) -> ReportSection {
    ReportSection::new(
        title,
        section_lines(balances, grouping, direct_roots),
        balances.totals(),
    )
}

fn equity_section(
    balances: &AccountBalances,
    result: &Amounts,
    grouping: AccountGrouping,
    direct_roots: &BTreeSet<String>,
) -> ReportSection {
    let mut lines = section_lines(balances, grouping, direct_roots);
    if !result.is_zero() {
        lines.push(ReportLine::new(UNCLOSED_RESULT, 0, result.clone()));
    }

    let mut subtotal = balances.totals();
    subtotal.merge(result);
    ReportSection::new("Equity", lines, subtotal)
}

pub(super) fn section_lines(
    balances: &AccountBalances,
    grouping: AccountGrouping,
    direct_roots: &BTreeSet<String>,
) -> Vec<ReportLine> {
    let mut lines = balances.lines(grouping);

    if grouping == AccountGrouping::Full {
        // The root line duplicates the section heading, unless the root
        // account itself carries postings.
        lines.retain_mut(|line| {
            let root = line.label.split(':').next().unwrap_or(&line.label);
            let root_is_direct = direct_roots.contains(root);
            if line.depth == 0 && !root_is_direct {
                return false;
            }
            if !root_is_direct && line.depth > 0 {
                line.depth -= 1;
            }
            true
        });
    }

    lines
}

/// Returns the first segment of `account`.
pub(super) fn root(account: &AccountName) -> &str {
    account
        .as_str()
        .split(':')
        .next()
        .unwrap_or_else(|| account.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Amount, CommodityName, Entry, Metadata, Posting, Transaction, TransactionStatus,
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
            transaction_cost: None,
            meta: Metadata::new(),
        }
    }

    fn transaction(d: NaiveDate, postings: Vec<Posting>) -> Entry {
        Entry::Transaction(Transaction {
            date: d,
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: None,
            narration: None,
            postings,
            tags: Vec::new(),
            links: Vec::new(),
            reference: None,
            meta: Metadata::new(),
        })
    }

    fn journal(entries: Vec<Entry>) -> Journal {
        Journal { entries }
    }

    fn example_journal() -> Journal {
        journal(vec![
            transaction(
                date(2026, 1, 1),
                vec![
                    posting("Assets:Bank", "1000.00", "EUR"),
                    posting("Equity:Opening", "-1000.00", "EUR"),
                ],
            ),
            transaction(
                date(2026, 1, 10),
                vec![
                    posting("Assets:Bank", "500.00", "EUR"),
                    posting("Income:Salary", "-500.00", "EUR"),
                ],
            ),
            transaction(
                date(2026, 1, 20),
                vec![
                    posting("Expenses:Rent", "200.00", "EUR"),
                    posting("Assets:Bank", "-200.00", "EUR"),
                ],
            ),
        ])
    }

    fn section<'a>(report: &'a Report, title: &str) -> &'a ReportSection {
        report
            .sections
            .iter()
            .find(|section| section.title.as_deref() == Some(title))
            .expect("the report has this section")
    }

    fn eur(amounts: &Amounts) -> Option<Decimal> {
        amounts.get(&CommodityName::new("EUR").unwrap())
    }

    fn labels(section: &ReportSection) -> Vec<(&str, usize)> {
        section
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line.depth))
            .collect()
    }

    #[test]
    fn builds_assets_liabilities_and_equity_sections() {
        let report = balance_sheet(
            &example_journal(),
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        assert_eq!(report.title, "Balance sheet");
        assert_eq!(report.scope, ReportScope::as_of(date(2026, 1, 31)));
        assert_eq!(report.sections.len(), 3);

        let assets = section(&report, "Assets");
        assert_eq!(labels(assets), vec![("Assets:Bank", 0)]);
        assert_eq!(eur(&assets.subtotal), Some("1300.00".parse().unwrap()));

        let liabilities = section(&report, "Liabilities");
        assert!(liabilities.lines.is_empty());
        assert!(liabilities.subtotal.is_empty());

        let equity = section(&report, "Equity");
        assert_eq!(
            labels(equity),
            vec![("Equity:Opening", 0), (UNCLOSED_RESULT, 0)]
        );
        assert_eq!(
            eur(&equity.lines[0].amounts),
            Some("1000.00".parse().unwrap())
        );
        // -(income + expenses) = -(-500 + 200)
        assert_eq!(
            eur(&equity.lines[1].amounts),
            Some("300.00".parse().unwrap())
        );
        assert_eq!(eur(&equity.subtotal), Some("1300.00".parse().unwrap()));

        // The journal is balanced and fully classified.
        assert!(report.totals.is_empty());
    }

    #[test]
    fn presents_liabilities_as_positive_amounts() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Expenses:Food", "50.00", "EUR"),
                posting("Liabilities:Card", "-50.00", "EUR"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Exact,
        );

        let liabilities = section(&report, "Liabilities");
        assert_eq!(labels(liabilities), vec![("Liabilities:Card", 0)]);
        assert_eq!(eur(&liabilities.subtotal), Some("50.00".parse().unwrap()));

        let equity = section(&report, "Equity");
        assert_eq!(eur(&equity.subtotal), Some("-50.00".parse().unwrap()));
    }

    #[test]
    fn ignores_transactions_after_the_reference_date() {
        let mut entries = example_journal().entries;
        entries.push(transaction(
            date(2026, 2, 1),
            vec![
                posting("Assets:Bank", "100.00", "EUR"),
                posting("Equity:Opening", "-100.00", "EUR"),
            ],
        ));
        let journal = journal(entries);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Exact,
        );

        assert_eq!(
            eur(&section(&report, "Assets").subtotal),
            Some("1300.00".parse().unwrap())
        );
    }

    #[test]
    fn classifies_accounts_with_the_configured_roots() {
        let journal = journal(vec![
            transaction(
                date(2026, 1, 1),
                vec![
                    posting("Actifs:Banque", "1000.00", "EUR"),
                    posting("Capitaux:Ouverture", "-1000.00", "EUR"),
                ],
            ),
            transaction(
                date(2026, 1, 15),
                vec![
                    posting("Impots:Revenu", "200.00", "EUR"),
                    posting("Actifs:Banque", "-200.00", "EUR"),
                ],
            ),
        ]);
        let classification = AccountClassification {
            assets: vec!["Actifs".to_string()],
            liabilities: vec!["Passif".to_string()],
            equity: vec!["Capitaux".to_string()],
            income: vec!["Revenus".to_string()],
            expenses: vec!["Depenses".to_string(), "Impots".to_string()],
        };

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &classification,
            AccountGrouping::Full,
        );

        assert_eq!(report.sections.len(), 3);
        assert_eq!(
            eur(&section(&report, "Assets").subtotal),
            Some("800.00".parse().unwrap())
        );
        assert_eq!(
            eur(&section(&report, "Equity").subtotal),
            Some("800.00".parse().unwrap())
        );
        assert!(report.totals.is_empty());
    }

    #[test]
    fn reports_unknown_roots_in_an_unclassified_section() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Divers:Mystere", "42.00", "EUR"),
                posting("Equity:Opening", "-42.00", "EUR"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        let unclassified = section(&report, "Unclassified");
        assert_eq!(labels(unclassified), vec![("Divers:Mystere", 0)]);
        assert_eq!(eur(&unclassified.subtotal), Some("42.00".parse().unwrap()));
        // The report total is the algebraic sum of the exact balances, not the
        // sum of the displayed sections.
        assert!(report.totals.is_empty());
    }

    #[test]
    fn hides_the_cumulative_root_line_of_a_section() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Assets:Bank", "10.00", "EUR"),
                posting("Equity:Opening", "-10.00", "EUR"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        assert_eq!(labels(section(&report, "Assets")), vec![("Assets:Bank", 0)]);
    }

    #[test]
    fn keeps_a_root_line_when_the_root_account_has_postings() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Assets", "10.00", "EUR"),
                posting("Equity:Opening", "-10.00", "EUR"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        assert_eq!(labels(section(&report, "Assets")), vec![("Assets", 0)]);
    }

    #[test]
    fn full_grouping_keeps_children_below_direct_roots_and_shifts_other_roots() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Assets", "10.00", "EUR"),
                posting("Assets:Bank", "5.00", "EUR"),
                posting("CurrentAssets:Cash", "5.00", "EUR"),
                posting("Equity:Opening", "-20.00", "EUR"),
            ],
        )]);
        let classification = AccountClassification::default()
            .with_assets(vec!["Assets".to_string(), "CurrentAssets".to_string()]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &classification,
            AccountGrouping::Full,
        );

        assert_eq!(
            labels(section(&report, "Assets")),
            vec![("Assets", 0), ("Assets:Bank", 1), ("CurrentAssets:Cash", 0)]
        );
    }

    #[test]
    fn omits_the_unclosed_result_when_it_is_zero() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Assets:Bank", "10.00", "EUR"),
                posting("Equity:Opening", "-10.00", "EUR"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        assert_eq!(
            labels(section(&report, "Equity")),
            vec![("Equity:Opening", 0)]
        );
    }

    #[test]
    fn keeps_every_commodity_separate() {
        let journal = journal(vec![transaction(
            date(2026, 1, 5),
            vec![
                posting("Assets:Bank", "10.00", "EUR"),
                posting("Assets:Bank", "20.00", "USD"),
                posting("Equity:Opening", "-10.00", "EUR"),
                posting("Equity:Opening", "-20.00", "USD"),
            ],
        )]);

        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Exact,
        );

        let assets = section(&report, "Assets");
        assert_eq!(eur(&assets.subtotal), Some("10.00".parse().unwrap()));
        assert_eq!(
            assets.subtotal.get(&CommodityName::new("USD").unwrap()),
            Some("20.00".parse().unwrap())
        );
        assert!(report.totals.is_empty());
    }

    #[test]
    fn an_empty_journal_has_three_empty_sections() {
        let report = balance_sheet(
            &journal(Vec::new()),
            ReferenceDate::new(date(2026, 1, 31)),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        assert_eq!(report.sections.len(), 3);
        assert!(report.lines().next().is_none());
        assert!(report.totals.is_empty());
    }
}
