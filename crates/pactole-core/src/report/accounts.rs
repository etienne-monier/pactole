use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;

use crate::models::{AccountName, Amount, Entry, Journal};

use super::{Amounts, ReportLine, ReportScope};

/// How account balances are grouped into report lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccountGrouping {
    /// One line per exact account, without any parent grouping.
    Exact,
    /// Fold accounts deeper than this number of segments into their ancestor.
    Depth(NonZeroUsize),
    /// Keep the whole prefix hierarchy.
    ///
    /// Every ancestor gets its own line, carrying the total of its subtree.
    /// These lines are cumulative: a child's amount is also included in its
    /// parent. Do not sum the displayed lines to obtain the report total.
    #[default]
    Full,
}

/// Signed balances or movements, keyed by account.
///
/// This is the input of the generic reporting layer: it accumulates amounts
/// per account and turns them into hierarchical [`ReportLine`]s. The balance
/// sheet (1.2) and the income statement (1.3) both build one of these before
/// rendering a report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountBalances {
    balances: BTreeMap<AccountName, Amounts>,
}

impl AccountBalances {
    /// Builds an empty set of account balances.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accumulates the postings of `journal` dated within `scope`.
    ///
    /// A [`ReportScope::AsOf`] scope sums every posting up to its reference
    /// date (inclusive), while [`ReportScope::Over`] restricts the sum to the
    /// period, both bounds included. The transactions are expected to be
    /// validated: a posting without an amount is skipped, so pass the journal
    /// returned by [`crate::validate_journal_all`] to account for implicit
    /// amounts.
    pub fn from_journal(journal: &Journal, scope: ReportScope) -> Self {
        let mut balances = Self::new();
        for entry in journal.entries() {
            let Entry::Transaction(transaction) = entry else {
                continue;
            };
            if !scope.contains(transaction.date) {
                continue;
            }
            for posting in &transaction.postings {
                if let Some(amount) = &posting.amount {
                    balances.add(&posting.account, amount);
                }
            }
        }
        balances
    }

    /// Iterates over the exact balances, in account order.
    pub fn iter(&self) -> impl Iterator<Item = (&AccountName, &Amounts)> + '_ {
        self.balances.iter()
    }

    /// Adds one amount to `account`.
    pub fn add(&mut self, account: &AccountName, amount: &Amount) {
        self.balances
            .entry(account.clone())
            .or_default()
            .add(amount);
    }

    /// Merges every amount of `amounts` into `account`.
    pub fn add_amounts(&mut self, account: &AccountName, amounts: &Amounts) {
        self.balances
            .entry(account.clone())
            .or_default()
            .merge(amounts);
    }

    /// Merges every balance of `other` into this set.
    pub fn merge(&mut self, other: &Self) {
        for (account, amounts) in &other.balances {
            self.balances
                .entry(account.clone())
                .or_default()
                .merge(amounts);
        }
    }

    /// Returns the total per commodity across every exact account.
    ///
    /// The result is independent of the grouping used to display the report:
    /// in [`AccountGrouping::Full`] mode, summing the cumulative lines would
    /// count the same amount several times.
    pub fn totals(&self) -> Amounts {
        let mut totals = Amounts::new();
        for amounts in self.balances.values() {
            totals.merge(amounts);
        }
        totals
    }

    /// Turns the balances into report lines according to `grouping`.
    ///
    /// Zero balances are hidden: an exact account whose amounts are all zero
    /// produces no line. In [`AccountGrouping::Full`] mode, an ancestor is
    /// kept as soon as one of its descendants is non-zero, even when the
    /// ancestor's own subtree happens to net to zero.
    pub fn lines(&self, grouping: AccountGrouping) -> Vec<ReportLine> {
        match grouping {
            AccountGrouping::Exact => self.exact_lines(),
            AccountGrouping::Depth(depth) => self.depth_lines(depth.get()),
            AccountGrouping::Full => self.full_lines(),
        }
    }

    fn exact_lines(&self) -> Vec<ReportLine> {
        let mut lines: Vec<ReportLine> = self
            .balances
            .iter()
            .filter(|&(_, amounts)| !amounts.is_zero())
            .map(|(account, amounts)| ReportLine::new(account.as_str(), 0, amounts.clone()))
            .collect();
        sort_lines(&mut lines);
        lines
    }

    fn depth_lines(&self, depth: usize) -> Vec<ReportLine> {
        let mut grouped: BTreeMap<AccountName, Amounts> = BTreeMap::new();
        for (account, amounts) in &self.balances {
            let Some(key) = prefix(account, depth) else {
                continue;
            };
            grouped.entry(key).or_default().merge(amounts);
        }

        let mut lines: Vec<ReportLine> = grouped
            .into_iter()
            .filter(|(_, amounts)| !amounts.is_zero())
            .map(|(account, amounts)| {
                let depth = segment_count(&account).saturating_sub(1);
                ReportLine::new(account.as_str(), depth, amounts)
            })
            .collect();
        sort_lines(&mut lines);
        lines
    }

    fn full_lines(&self) -> Vec<ReportLine> {
        let mut grouped: BTreeMap<AccountName, Amounts> = BTreeMap::new();
        let mut keep: BTreeSet<AccountName> = BTreeSet::new();

        for (account, amounts) in &self.balances {
            let non_zero = !amounts.is_zero();
            for depth in 1..=segment_count(account) {
                let Some(key) = prefix(account, depth) else {
                    continue;
                };
                grouped.entry(key.clone()).or_default().merge(amounts);
                if non_zero {
                    keep.insert(key);
                }
            }
        }

        let mut lines: Vec<ReportLine> = grouped
            .into_iter()
            .filter(|(account, _)| keep.contains(account))
            .map(|(account, amounts)| {
                let depth = segment_count(&account).saturating_sub(1);
                ReportLine::new(account.as_str(), depth, amounts)
            })
            .collect();
        sort_lines(&mut lines);
        lines
    }
}

/// Returns the prefix of `account` made of its first `segments` segments.
///
/// Returns `None` when `segments` is zero. The prefix of an already valid
/// account cannot fail to validate, but the error is ignored rather than
/// panicking if it ever does.
fn prefix(account: &AccountName, segments: usize) -> Option<AccountName> {
    let mut prefix = String::new();
    for (index, segment) in account.as_str().split(':').take(segments).enumerate() {
        if index > 0 {
            prefix.push(':');
        }
        prefix.push_str(segment);
    }

    if prefix.is_empty() {
        None
    } else {
        AccountName::new(prefix).ok()
    }
}

fn segment_count(account: &AccountName) -> usize {
    account.as_str().split(':').count()
}

/// Sorts account lines segment by segment, so a parent always comes before
/// its descendants and a whole subtree stays contiguous.
fn sort_lines(lines: &mut [ReportLine]) {
    lines.sort_by(|left, right| compare_labels(&left.label, &right.label));
}

fn compare_labels(left: &str, right: &str) -> std::cmp::Ordering {
    left.split(':').cmp(right.split(':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommodityName;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn account(name: &str) -> AccountName {
        AccountName::new(name).unwrap()
    }

    fn amount(number: &str, commodity: &str) -> Amount {
        Amount {
            number: number.parse::<Decimal>().unwrap(),
            commodity: CommodityName::new(commodity).unwrap(),
        }
    }

    fn balances(entries: &[(&str, &str, &str)]) -> AccountBalances {
        let mut balances = AccountBalances::new();
        for (account_name, number, commodity) in entries {
            balances.add(&account(account_name), &amount(number, commodity));
        }
        balances
    }

    fn labels(lines: &[ReportLine]) -> Vec<(&str, usize)> {
        lines
            .iter()
            .map(|line| (line.label.as_str(), line.depth))
            .collect()
    }

    #[test]
    fn exact_keeps_one_line_per_account() {
        let balances = balances(&[
            ("Actifs:Banque", "10.00", "EUR"),
            ("Depenses:Cafe", "-10.00", "EUR"),
        ]);

        let lines = balances.lines(AccountGrouping::Exact);

        assert_eq!(
            labels(&lines),
            vec![("Actifs:Banque", 0), ("Depenses:Cafe", 0)]
        );
    }

    #[test]
    fn full_adds_every_prefix_with_cumulative_totals() {
        let balances = balances(&[
            ("Actifs:Banque:Courant", "100.00", "EUR"),
            ("Actifs:Banque:Livret", "50.00", "EUR"),
            ("Depenses:Logement", "-150.00", "EUR"),
        ]);

        let lines = balances.lines(AccountGrouping::Full);

        assert_eq!(
            labels(&lines),
            vec![
                ("Actifs", 0),
                ("Actifs:Banque", 1),
                ("Actifs:Banque:Courant", 2),
                ("Actifs:Banque:Livret", 2),
                ("Depenses", 0),
                ("Depenses:Logement", 1),
            ]
        );
        let eur = CommodityName::new("EUR").unwrap();
        assert_eq!(lines[0].amounts.get(&eur), Some("150.00".parse().unwrap()));
        assert_eq!(lines[1].amounts.get(&eur), Some("150.00".parse().unwrap()));
        assert_eq!(lines[4].amounts.get(&eur), Some("-150.00".parse().unwrap()));
    }

    #[test]
    fn depth_folds_deeper_accounts() {
        let balances = balances(&[
            ("Actifs:Banque:Courant", "100.00", "EUR"),
            ("Actifs:Banque:Livret", "50.00", "EUR"),
            ("Depenses:Logement:Loyer", "-150.00", "EUR"),
        ]);

        let lines = balances.lines(AccountGrouping::Depth(NonZeroUsize::new(2).unwrap()));

        assert_eq!(
            labels(&lines),
            vec![("Actifs:Banque", 1), ("Depenses:Logement", 1)]
        );
        let eur = CommodityName::new("EUR").unwrap();
        assert_eq!(lines[0].amounts.get(&eur), Some("150.00".parse().unwrap()));
        assert_eq!(lines[1].amounts.get(&eur), Some("-150.00".parse().unwrap()));
    }

    #[test]
    fn depth_one_keeps_only_top_level_accounts() {
        let balances = balances(&[("Actifs:Banque:Courant", "100.00", "EUR")]);

        let lines = balances.lines(AccountGrouping::Depth(NonZeroUsize::MIN));

        assert_eq!(labels(&lines), vec![("Actifs", 0)]);
        assert_eq!(
            lines[0].amounts.get(&CommodityName::new("EUR").unwrap()),
            Some("100.00".parse().unwrap())
        );
    }

    #[test]
    fn full_keeps_an_ancestor_of_a_non_zero_descendant() {
        let balances = balances(&[
            ("Actifs:Banque:Courant", "100.00", "EUR"),
            ("Actifs:Banque:Livret", "-100.00", "EUR"),
        ]);

        let lines = balances.lines(AccountGrouping::Full);

        assert_eq!(
            labels(&lines),
            vec![
                ("Actifs", 0),
                ("Actifs:Banque", 1),
                ("Actifs:Banque:Courant", 2),
                ("Actifs:Banque:Livret", 2),
            ]
        );
        assert!(lines[0].amounts.is_empty());
        assert!(lines[1].amounts.is_empty());
    }

    #[test]
    fn hides_accounts_whose_balance_is_zero() {
        let balances = balances(&[
            ("Actifs:Vide", "10.00", "EUR"),
            ("Actifs:Vide", "-10.00", "EUR"),
            ("Actifs:Plein", "1.00", "EUR"),
        ]);

        let lines = balances.lines(AccountGrouping::Exact);

        assert_eq!(labels(&lines), vec![("Actifs:Plein", 0)]);
    }

    #[test]
    fn totals_are_independent_of_the_grouping() {
        let balances = balances(&[
            ("Actifs:Banque:Courant", "100.00", "EUR"),
            ("Actifs:Banque:Livret", "50.00", "USD"),
            ("Depenses:Logement", "-100.00", "EUR"),
        ]);

        let totals = balances.totals();
        let eur = CommodityName::new("EUR").unwrap();
        let usd = CommodityName::new("USD").unwrap();

        assert_eq!(totals.get(&eur), Some("0.00".parse().unwrap()));
        assert_eq!(totals.get(&usd), Some("50.00".parse().unwrap()));
    }

    #[test]
    fn merges_balances_of_the_same_account() {
        let mut left = balances(&[("Actifs:Banque", "10.00", "EUR")]);
        let right = balances(&[("Actifs:Banque", "5.00", "EUR")]);

        left.merge(&right);

        assert_eq!(
            left.totals()
                .get(&CommodityName::new("EUR").unwrap())
                .unwrap(),
            "15.00".parse().unwrap()
        );
    }

    #[test]
    fn iterates_over_the_exact_balances() {
        let balances = balances(&[("Actifs:Banque", "10.00", "EUR")]);

        let entries: Vec<(&str, Decimal)> = balances
            .iter()
            .map(|(account, amounts)| {
                (
                    account.as_str(),
                    amounts.get(&CommodityName::new("EUR").unwrap()).unwrap(),
                )
            })
            .collect();

        assert_eq!(entries, vec![("Actifs:Banque", "10.00".parse().unwrap())]);
    }

    #[test]
    fn sums_the_postings_dated_within_an_as_of_scope() {
        let journal = journal(&[
            (date(2024, 1, 10), "Actifs:Banque", "10.00"),
            (date(2024, 1, 31), "Actifs:Banque", "5.00"),
            (date(2024, 2, 1), "Actifs:Banque", "100.00"),
        ]);

        let balances =
            AccountBalances::from_journal(&journal, ReportScope::as_of(date(2024, 1, 31)));

        assert_eq!(
            balances
                .totals()
                .get(&CommodityName::new("EUR").unwrap())
                .unwrap(),
            "15.00".parse().unwrap()
        );
    }

    #[test]
    fn sums_the_postings_dated_within_an_inclusive_period() {
        let journal = journal(&[
            (date(2024, 1, 9), "Actifs:Banque", "1.00"),
            (date(2024, 1, 10), "Actifs:Banque", "2.00"),
            (date(2024, 1, 20), "Actifs:Banque", "3.00"),
            (date(2024, 1, 21), "Actifs:Banque", "4.00"),
        ]);

        let balances = AccountBalances::from_journal(
            &journal,
            ReportScope::over(date(2024, 1, 10), date(2024, 1, 20)).unwrap(),
        );

        assert_eq!(
            balances
                .totals()
                .get(&CommodityName::new("EUR").unwrap())
                .unwrap(),
            "5.00".parse().unwrap()
        );
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    /// A journal with one single-posting transaction per `(date, account,
    /// number)` triple, all in EUR.
    fn journal(entries: &[(NaiveDate, &str, &str)]) -> Journal {
        use crate::models::{Metadata, Posting, Transaction, TransactionStatus};

        Journal {
            entries: entries
                .iter()
                .map(|(date, account_name, number)| {
                    Entry::Transaction(Transaction {
                        date: *date,
                        effective_date: None,
                        status: TransactionStatus::Cleared,
                        payee: None,
                        narration: None,
                        postings: vec![Posting {
                            account: account(account_name),
                            amount: Some(amount(number, "EUR")),
                            transaction_cost: None,
                            meta: Metadata::new(),
                        }],
                        tags: Vec::new(),
                        links: Vec::new(),
                        reference: None,
                        meta: Metadata::new(),
                    })
                })
                .collect(),
        }
    }
}
