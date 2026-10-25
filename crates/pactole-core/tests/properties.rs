//! Property-based tests for the core validation rules.
//!
//! The unit tests next to the validator cover hand-written cases; these
//! generate arbitrary posting lists and check the invariants that must hold
//! for every journal: a transaction balances if and only if its postings sum
//! to zero per commodity, a single missing amount is inferred so that it
//! does, and neither the order of the postings nor the entry point used to
//! validate the journal changes the verdict.

use chrono::NaiveDate;
use pactole_core::{
    AccountBalances, AccountClassification, AccountGrouping, AccountName, Amount, Amounts,
    Commodity, CommodityName, Entry, Journal, Metadata, Open, Period, Posting, ReferenceDate,
    Report, ReportFormat, ReportLine, ReportScope, Transaction, TransactionStatus, ValidationError,
    balance_sheet, validate_journal, validate_journal_all,
};
use proptest::prelude::*;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

const ACCOUNTS: [&str; 3] = ["Actifs:Compte", "Actifs:Livret", "Depenses:Logement"];
const COMMODITIES: [&str; 3] = ["EUR", "USD", "GBP"];
const REPORT_ACCOUNTS: [&str; 6] = [
    "Actifs",
    "Actifs:Banque",
    "Actifs:Banque:Courant",
    "Actifs:Banque:Livret",
    "Depenses:Logement",
    "Depenses:Logement:Loyer",
];

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 2).expect("a valid date")
}

/// A signed amount in cents, so every generated decimal is exact.
fn decimal() -> impl Strategy<Value = Decimal> {
    (-1_000_000i64..=1_000_000).prop_map(|cents| Decimal::new(cents, 2))
}

/// An `(account, commodity, amount)` movement.
fn movement() -> impl Strategy<Value = (usize, usize, Decimal)> {
    (0..ACCOUNTS.len(), 0..COMMODITIES.len(), decimal())
}

fn account(index: usize) -> AccountName {
    AccountName::new(ACCOUNTS[index]).expect("a valid account name")
}

fn commodity(index: usize) -> CommodityName {
    CommodityName::new(COMMODITIES[index]).expect("a valid commodity name")
}

fn posting(account_index: usize, commodity_index: usize, number: Decimal) -> Posting {
    Posting {
        account: account(account_index),
        amount: Some(Amount {
            number,
            commodity: commodity(commodity_index),
        }),
        transaction_cost: None,
        meta: Metadata::new(),
    }
}

fn movements_to_postings(movements: &[(usize, usize, Decimal)]) -> Vec<Posting> {
    movements
        .iter()
        .map(|&(account_index, commodity_index, number)| {
            posting(account_index, commodity_index, number)
        })
        .collect()
}

fn transaction(postings: Vec<Posting>) -> Transaction {
    Transaction {
        date: date(),
        effective_date: None,
        status: TransactionStatus::Cleared,
        payee: None,
        narration: None,
        postings,
        tags: Vec::new(),
        links: Vec::new(),
        reference: None,
        meta: Metadata::new(),
    }
}

/// A journal that declares every commodity and opens every account, so the
/// transaction can only fail the balancing checks.
fn journal(postings: Vec<Posting>) -> Journal {
    let mut entries = Vec::new();
    for index in 0..COMMODITIES.len() {
        entries.push(Entry::Commodity(Commodity {
            date: date(),
            name: commodity(index),
            meta: Metadata::new(),
        }));
    }
    for index in 0..ACCOUNTS.len() {
        entries.push(Entry::Open(Open {
            date: date(),
            account: account(index),
            commodities: Vec::new(),
            description: None,
            meta: Metadata::new(),
        }));
    }
    entries.push(Entry::Transaction(transaction(postings)));
    Journal { entries }
}

fn sums(postings: &[Posting]) -> BTreeMap<String, Decimal> {
    let mut sums: BTreeMap<String, Decimal> = BTreeMap::new();
    for posting in postings {
        if let Some(amount) = &posting.amount {
            *sums
                .entry(amount.commodity.as_str().to_string())
                .or_default() += amount.number;
        }
    }
    sums
}

/// Whether every commodity sums to zero, ignoring commodities that are
/// present with a zero total.
fn is_balanced(postings: &[Posting]) -> bool {
    sums(postings).values().all(Decimal::is_zero)
}

/// Appends, for every commodity used, the posting that brings its sum to
/// zero.
fn balanced(mut postings: Vec<Posting>) -> Vec<Posting> {
    for (commodity, sum) in sums(&postings) {
        postings.push(Posting {
            account: account(0),
            amount: Some(Amount {
                number: -sum,
                commodity: CommodityName::new(commodity).expect("a commodity read from a posting"),
            }),
            transaction_cost: None,
            meta: Metadata::new(),
        });
    }
    postings
}

fn validated_transaction(report: &pactole_core::ValidationReport) -> &Transaction {
    report
        .journal
        .entries
        .iter()
        .find_map(|entry| match entry {
            Entry::Transaction(transaction) => Some(transaction),
            _ => None,
        })
        .expect("the journal holds the generated transaction")
}

proptest! {
    #[test]
    fn a_balanced_transaction_is_accepted(movements in prop::collection::vec(movement(), 0..6)) {
        let report = validate_journal_all(journal(balanced(movements_to_postings(&movements))));

        prop_assert!(
            report.issues.is_empty(),
            "a balanced transaction raised {:?}",
            report.issues
        );
        prop_assert!(
            is_balanced(&validated_transaction(&report).postings),
            "the accepted transaction is not balanced: {:?}",
            sums(&validated_transaction(&report).postings)
        );
    }

    #[test]
    fn an_unbalanced_transaction_is_reported(
        movements in prop::collection::vec(movement(), 0..6),
        commodity_index in 0..COMMODITIES.len(),
    ) {
        // The base postings balance, so adding a single cent is a known,
        // non-zero imbalance in exactly one commodity.
        let mut postings = balanced(movements_to_postings(&movements));
        postings.push(posting(1, commodity_index, Decimal::new(1, 2)));

        let report = validate_journal_all(journal(postings));

        prop_assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
        match &report.issues[0].error {
            ValidationError::TransactionNotBalanced { commodity, sum, .. } => {
                prop_assert_eq!(commodity, COMMODITIES[commodity_index]);
                prop_assert_eq!(*sum, Decimal::new(1, 2));
            }
            other => prop_assert!(false, "unexpected error: {other:?}"),
        }
    }

    #[test]
    fn the_inferred_amount_balancing_a_transaction_is_the_negated_sum(
        movements in prop::collection::vec(movement(), 1..6),
        commodity_index in 0..COMMODITIES.len(),
        missing_at in any::<prop::sample::Index>(),
    ) {
        // Every known amount uses the same commodity, so the commodity of the
        // missing amount is unambiguous whatever its position.
        let mut postings: Vec<Posting> = movements
            .iter()
            .map(|&(account_index, _, number)| posting(account_index, commodity_index, number))
            .collect();
        let expected = -sums(&postings)[COMMODITIES[commodity_index]];
        let missing_at = missing_at.index(postings.len());
        postings.insert(
            missing_at,
            Posting {
                account: account(2),
                amount: None,
                transaction_cost: None,
                meta: Metadata::new(),
            },
        );

        let report = validate_journal_all(journal(postings));

        prop_assert!(report.issues.is_empty(), "{:?}", report.issues);
        let inferred = validated_transaction(&report).postings[missing_at]
            .amount
            .as_ref()
            .expect("the missing amount is inferred");
        prop_assert_eq!(inferred.number, expected);
        prop_assert_eq!(inferred.commodity.as_str(), COMMODITIES[commodity_index]);
        prop_assert!(is_balanced(&validated_transaction(&report).postings));
    }

    #[test]
    fn two_postings_without_an_amount_are_rejected(
        first in movement(),
        second in movement(),
    ) {
        let postings = vec![
            posting(first.0, first.1, first.2),
            Posting { account: account(1), amount: None, transaction_cost: None, meta: Metadata::new() },
            Posting { account: account(2), amount: None, transaction_cost: None, meta: Metadata::new() },
            posting(second.0, second.1, second.2),
        ];

        let report = validate_journal_all(journal(postings));

        prop_assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
        let too_many = matches!(
            report.issues[0].error,
            ValidationError::TooManyPostingsWithoutAmount { count: 2, .. }
        );
        prop_assert!(too_many);
    }

    #[test]
    fn a_missing_amount_across_several_commodities_is_ambiguous(
        first in movement(),
        second in movement(),
    ) {
        let first_index = first.1;
        let second_index = (first.1 + 1 + second.1 % (COMMODITIES.len() - 1)) % COMMODITIES.len();

        let postings = vec![
            posting(first.0, first_index, first.2),
            posting(second.0, second_index, second.2),
            Posting { account: account(2), amount: None, transaction_cost: None, meta: Metadata::new() },
        ];

        let report = validate_journal_all(journal(postings));

        prop_assert_eq!(report.issues.len(), 1, "{:?}", report.issues);
        let ambiguous = matches!(
            report.issues[0].error,
            ValidationError::AmbiguousAutoBalanceCommodity { .. }
        );
        prop_assert!(ambiguous);
    }

    #[test]
    fn the_order_of_the_postings_does_not_change_the_verdict(
        movements in prop::collection::vec(movement(), 0..6),
        shuffled in any::<prop::sample::Index>(),
    ) {
        let postings = balanced(movements_to_postings(&movements));

        let mut reversed = postings.clone();
        let offset = if reversed.is_empty() {
            0
        } else {
            shuffled.index(reversed.len())
        };
        reversed.rotate_left(offset);

        let straight = validate_journal_all(journal(postings));
        let rotated = validate_journal_all(journal(reversed));

        prop_assert!(straight.issues.is_empty(), "{:?}", straight.issues);
        prop_assert!(rotated.issues.is_empty(), "{:?}", rotated.issues);
        prop_assert_eq!(
            sums(&validated_transaction(&straight).postings),
            sums(&validated_transaction(&rotated).postings)
        );
    }

    #[test]
    fn both_validation_entry_points_agree(movements in prop::collection::vec(movement(), 0..6)) {
        let postings = movements_to_postings(&movements);

        let all = validate_journal_all(journal(postings.clone()));
        let first = validate_journal(journal(postings));

        prop_assert_eq!(first.is_ok(), all.issues.is_empty());
        if let (Ok(first_journal), true) = (first, all.issues.is_empty()) {
            prop_assert_eq!(first_journal.entries.len(), all.journal.entries.len());
        }
    }

    #[test]
    fn validation_never_drops_or_adds_entries(movements in prop::collection::vec(movement(), 0..6)) {
        let postings = balanced(movements_to_postings(&movements));
        let input = journal(postings);

        let report = validate_journal_all(input.clone());

        prop_assert_eq!(report.journal.entries.len(), input.entries.len());
        prop_assert_eq!(
            report.journal.entries.iter().filter(|entry| matches!(entry, Entry::Transaction(_))).count(),
            1
        );
    }
}

// ---------------------------------------------------------------------------
// Reporting properties
// ---------------------------------------------------------------------------

/// A date between 2000 and roughly 2055, away from the chrono bounds.
fn arbitrary_date() -> impl Strategy<Value = NaiveDate> {
    (0i32..=20_000).prop_map(|days| {
        NaiveDate::from_num_days_from_ce_opt(days + 730_000).expect("a valid date")
    })
}

/// An `(account, commodity, amount)` movement over the reporting accounts.
fn report_movement() -> impl Strategy<Value = (usize, usize, Decimal)> {
    (0..REPORT_ACCOUNTS.len(), 0..COMMODITIES.len(), decimal())
}

fn report_account(index: usize) -> AccountName {
    AccountName::new(REPORT_ACCOUNTS[index]).expect("a valid account name")
}

fn add_movement(balances: &mut AccountBalances, movement: &(usize, usize, Decimal)) {
    let (account_index, commodity_index, number) = movement;
    balances.add(
        &report_account(*account_index),
        &Amount {
            number: *number,
            commodity: commodity(*commodity_index),
        },
    );
}

fn totals_by_name(balances: &AccountBalances) -> BTreeMap<String, Decimal> {
    balances
        .totals()
        .iter()
        .map(|(commodity, number)| (commodity.as_str().to_string(), number))
        .collect()
}

fn groupings() -> [AccountGrouping; 4] {
    [
        AccountGrouping::Exact,
        AccountGrouping::Depth(NonZeroUsize::MIN),
        AccountGrouping::Depth(NonZeroUsize::new(2).expect("a non-zero depth")),
        AccountGrouping::Full,
    ]
}

proptest! {
    #[test]
    fn period_contains_exactly_its_inclusive_bounds(
        start in arbitrary_date(),
        span in 0u32..=3_650,
    ) {
        let end = start + chrono::Duration::days(i64::from(span));
        let period = Period::new(start, end).expect("start is not after end");

        for offset in -2i64..=i64::from(span) + 2 {
            let date = start + chrono::Duration::days(offset);
            prop_assert_eq!(
                period.contains(date),
                start <= date && date <= end,
                "date {} between {} and {}",
                date,
                start,
                end
            );
        }
    }

    #[test]
    fn as_of_scope_contains_exactly_the_dates_up_to_its_reference(
        reference in arbitrary_date(),
        date in arbitrary_date(),
    ) {
        prop_assert_eq!(
            ReportScope::as_of(reference).contains(date),
            date <= reference
        );
    }

    #[test]
    fn account_grouping_preserves_totals_per_commodity(
        movements in prop::collection::vec(report_movement(), 0..30),
    ) {
        let mut balances = AccountBalances::new();
        for movement in &movements {
            add_movement(&mut balances, movement);
        }
        let expected = totals_by_name(&balances);

        for grouping in groupings() {
            let mut summed = Amounts::new();
            for line in balances.lines(grouping) {
                // Full lines are cumulative: only the top-level lines form a
                // partition of the exact balances.
                if grouping == AccountGrouping::Full && line.depth != 0 {
                    continue;
                }
                summed.merge(&line.amounts);
            }

            for (name, number) in &expected {
                let commodity = CommodityName::new(name.clone()).expect("a valid commodity name");
                prop_assert_eq!(
                    summed.get(&commodity).unwrap_or_default(),
                    *number,
                    "commodity {} with grouping {:?}",
                    name,
                    grouping
                );
            }
        }
    }

    #[test]
    fn account_aggregation_is_order_independent(
        movements in prop::collection::vec(report_movement(), 0..30),
    ) {
        let mut forward = AccountBalances::new();
        for movement in &movements {
            add_movement(&mut forward, movement);
        }
        let mut backward = AccountBalances::new();
        for movement in movements.iter().rev() {
            add_movement(&mut backward, movement);
        }

        prop_assert_eq!(forward.totals(), backward.totals());
        for grouping in groupings() {
            prop_assert_eq!(forward.lines(grouping), backward.lines(grouping));
        }
    }

    #[test]
    fn json_rendering_is_always_parseable(
        title in ".*",
        label in ".*",
        number in decimal(),
    ) {
        let mut amounts = Amounts::new();
        amounts.add(&Amount {
            number,
            commodity: commodity(0),
        });
        let report = Report::new(
            title,
            ReportScope::as_of(date()),
            vec![ReportLine::new(label, 0, amounts.clone())],
            amounts,
        );

        let rendered = report.render(ReportFormat::Json);

        prop_assert!(serde_json::from_str::<serde_json::Value>(&rendered).is_ok());
    }
}

// ---------------------------------------------------------------------------
// Balance-sheet properties
// ---------------------------------------------------------------------------

/// Accounts of the five default categories.
const BALANCE_SHEET_ACCOUNTS: [&str; 7] = [
    "Assets:Bank",
    "Assets:Cash",
    "Liabilities:Card",
    "Equity:Opening",
    "Income:Salary",
    "Expenses:Food",
    "Expenses:Food:Restaurant",
];

/// An `(account, commodity, amount)` movement over the balance-sheet accounts.
fn balance_sheet_movement() -> impl Strategy<Value = (usize, usize, Decimal)> {
    (
        0..BALANCE_SHEET_ACCOUNTS.len(),
        0..COMMODITIES.len(),
        decimal(),
    )
}

/// A journal with one balanced transaction per movement.
///
/// Each movement is paired with its opposite on `Equity:Suspense`, so every
/// transaction balances per commodity and every account is classified.
fn balance_sheet_journal(movements: &[(usize, usize, Decimal)]) -> Journal {
    let suspense = AccountName::new("Equity:Suspense").expect("a valid account name");
    let entries = movements
        .iter()
        .map(|&(account_index, commodity_index, number)| {
            Entry::Transaction(transaction(vec![
                Posting {
                    account: AccountName::new(BALANCE_SHEET_ACCOUNTS[account_index])
                        .expect("a valid account name"),
                    amount: Some(Amount {
                        number,
                        commodity: commodity(commodity_index),
                    }),
                    transaction_cost: None,
                    meta: Metadata::new(),
                },
                Posting {
                    account: suspense.clone(),
                    amount: Some(Amount {
                        number: -number,
                        commodity: commodity(commodity_index),
                    }),
                    transaction_cost: None,
                    meta: Metadata::new(),
                },
            ]))
        })
        .collect();
    Journal { entries }
}

fn section_subtotal<'a>(report: &'a Report, title: &str) -> &'a Amounts {
    &report
        .sections
        .iter()
        .find(|section| section.title.as_deref() == Some(title))
        .expect("the balance sheet has this section")
        .subtotal
}

fn amounts_by_name(amounts: &Amounts) -> BTreeMap<String, Decimal> {
    amounts
        .iter()
        .map(|(commodity, number)| (commodity.as_str().to_string(), number))
        .collect()
}

proptest! {
    #[test]
    fn a_balance_sheet_satisfies_the_accounting_equation(
        movements in prop::collection::vec(balance_sheet_movement(), 0..30),
    ) {
        let journal = balance_sheet_journal(&movements);

        for grouping in groupings() {
            let report = balance_sheet(
                &journal,
                ReferenceDate::new(date()),
                &AccountClassification::default(),
                grouping,
            );

            let assets = amounts_by_name(section_subtotal(&report, "Assets"));
            let liabilities = amounts_by_name(section_subtotal(&report, "Liabilities"));
            let equity = amounts_by_name(section_subtotal(&report, "Equity"));

            for name in COMMODITIES {
                prop_assert_eq!(
                    assets.get(name).copied().unwrap_or_default(),
                    liabilities.get(name).copied().unwrap_or_default()
                        + equity.get(name).copied().unwrap_or_default(),
                    "commodity {} with grouping {:?}",
                    name,
                    grouping
                );
            }

            prop_assert!(
                report.totals.is_empty(),
                "a balanced, fully classified journal has no report total: {:?}",
                report.totals
            );
        }
    }

    #[test]
    fn a_balance_sheet_keeps_every_commodity_separate(
        movements in prop::collection::vec(balance_sheet_movement(), 0..30),
    ) {
        let journal = balance_sheet_journal(&movements);
        let report = balance_sheet(
            &journal,
            ReferenceDate::new(date()),
            &AccountClassification::default(),
            AccountGrouping::Full,
        );

        for section in &report.sections {
            for line in &section.lines {
                for (commodity, number) in line.amounts.iter() {
                    prop_assert!(!number.is_zero());
                    prop_assert!(COMMODITIES.contains(&commodity.as_str()));
                }
            }
        }
    }
}
