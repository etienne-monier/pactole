//! Property-based tests for the canonical formatter.
//!
//! The unit tests next to the printer check hand-written layouts; these
//! generate syntactically valid documents with arbitrary spacing, indentation
//! and blank lines, and check the invariants the formatter must always
//! satisfy: it accepts every valid document, is idempotent, preserves the
//! lowered entries, and emits text that parses back without a diagnostic.

use pactole_core::{
    AccountName, Amount, CommodityName, Entry, Metadata, MetadataKey, Posting, Transaction,
    TransactionStatus,
};
use pactole_storage_fs::{analyze_file, format, format_transactions};
use proptest::prelude::*;
use rust_decimal::Decimal;

const ACCOUNTS: [&str; 3] = ["Actifs:A", "Actifs:B", "Depenses:C"];
const COMMODITIES: [&str; 2] = ["EUR", "USD"];
const PAYEES: [&str; 3] = ["Café", "Hôtel du Port", "Boutique"];
const NARRATIONS: [&str; 3] = ["Dîner", "Courses", ""];

fn date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 1, 2).expect("a valid date")
}

/// Cycles through a random set of whitespace widths, so that no two
/// consecutive separators are necessarily the same.
struct Spacing {
    widths: Vec<usize>,
    next: usize,
}

impl Spacing {
    fn new(widths: Vec<usize>) -> Self {
        Self { widths, next: 0 }
    }

    fn take(&mut self) -> usize {
        let width = self.widths[self.next % self.widths.len()];
        self.next += 1;
        width
    }

    /// An inline separator: spaces, or a tab.
    fn ws(&mut self) -> String {
        let width = self.take();
        if width.is_multiple_of(11) {
            "\t".to_string()
        } else {
            " ".repeat(width)
        }
    }

    /// The indentation of a property line or a posting: spaces, or a tab.
    fn indent(&mut self) -> String {
        let width = self.take();
        if width.is_multiple_of(7) {
            "\t".to_string()
        } else {
            " ".repeat(width % 4 + 1)
        }
    }
}

#[derive(Debug, Clone)]
enum Directive {
    Commodity(usize),
    Open {
        account: usize,
        commodity: Option<usize>,
    },
    Close {
        account: usize,
    },
    Payee(usize),
    Balance {
        account: usize,
        cents: i64,
        tolerance: bool,
        commodity: usize,
    },
    Include,
    Comment,
    Transaction {
        status: char,
        payee: usize,
        narration: Option<usize>,
        key: Option<(String, String)>,
        postings: Vec<(usize, String, usize)>,
        posting_meta: Option<(String, String)>,
    },
}

impl Directive {
    fn render(&self, spacing: &mut Spacing) -> String {
        match self {
            Directive::Commodity(index) => {
                format!(
                    "2026-01-01{}commodity{}{}",
                    spacing.ws(),
                    spacing.ws(),
                    COMMODITIES[*index]
                )
            }
            Directive::Open { account, commodity } => {
                let mut out = format!(
                    "2026-01-01{}open{}{}",
                    spacing.ws(),
                    spacing.ws(),
                    ACCOUNTS[*account]
                );
                if let Some(index) = commodity {
                    out.push_str(&spacing.ws());
                    out.push_str(COMMODITIES[*index]);
                }
                out
            }
            Directive::Close { account } => {
                format!(
                    "2026-01-01{}close{}{}",
                    spacing.ws(),
                    spacing.ws(),
                    ACCOUNTS[*account]
                )
            }
            Directive::Payee(index) => {
                format!("payee{}\"{}\"", spacing.ws(), PAYEES[*index])
            }
            Directive::Balance {
                account,
                cents,
                tolerance,
                commodity,
            } => {
                let mut out = format!(
                    "2026-01-01{}balance{}{}{}{}",
                    spacing.ws(),
                    spacing.ws(),
                    ACCOUNTS[*account],
                    spacing.ws(),
                    Decimal::new(*cents, 2)
                );
                if *tolerance {
                    out.push_str(&format!("{}~{}0.01", spacing.ws(), spacing.ws()));
                }
                out.push_str(&spacing.ws());
                out.push_str(COMMODITIES[*commodity]);
                out
            }
            Directive::Include => format!("include{}\"sub/ledger.pactole\"", spacing.ws()),
            Directive::Comment => "; a standalone comment".to_string(),
            Directive::Transaction {
                status,
                payee,
                narration,
                key,
                postings,
                posting_meta,
            } => {
                let mut out = format!(
                    "2026-01-01{}{}{}\"{}\"",
                    spacing.ws(),
                    status,
                    spacing.ws(),
                    PAYEES[*payee]
                );
                if let Some(index) = narration {
                    out.push_str(&format!("{}\"{}\"", spacing.ws(), NARRATIONS[*index]));
                }
                if let Some((key, value)) = key {
                    out.push_str(&format!(
                        "\n{}{}:{}\"{value}\"",
                        spacing.indent(),
                        key,
                        spacing.ws()
                    ));
                }
                for (position, (account, number, commodity)) in postings.iter().enumerate() {
                    out.push_str(&format!(
                        "\n{}{}{}{}{}",
                        spacing.indent(),
                        ACCOUNTS[*account],
                        spacing.ws(),
                        number,
                        spacing.ws()
                    ));
                    out.push_str(COMMODITIES[*commodity]);
                    // Attach at most one property to one posting.
                    if position == 0
                        && let Some((key, value)) = posting_meta
                    {
                        out.push_str(&format!(
                            "\n{}{}:{}\"{value}\"",
                            spacing.indent(),
                            key,
                            spacing.ws()
                        ));
                    }
                }
                out
            }
        }
    }
}

fn number() -> impl Strategy<Value = String> {
    (-100_000i64..=100_000).prop_map(|cents| Decimal::new(cents, 2).to_string())
}

fn metadata() -> impl Strategy<Value = (String, String)> {
    ("[a-z][a-z_]{0,6}", "[A-Za-zé ]{1,8}")
}

fn postings() -> impl Strategy<Value = Vec<(usize, String, usize)>> {
    prop::collection::vec((0..ACCOUNTS.len(), number(), 0..COMMODITIES.len()), 1..4)
}

fn status() -> impl Strategy<Value = char> {
    prop::sample::select(&['*', '!', '?'][..])
}

fn transaction_directive() -> impl Strategy<Value = Directive> {
    (
        status(),
        0..PAYEES.len(),
        prop::option::of(0..NARRATIONS.len()),
        prop::option::of(metadata()),
        postings(),
        prop::option::of(metadata()),
    )
        .prop_map(|(status, payee, narration, key, postings, posting_meta)| {
            Directive::Transaction {
                status,
                payee,
                narration,
                key,
                postings,
                posting_meta,
            }
        })
}

fn directive() -> impl Strategy<Value = Directive> {
    prop_oneof![
        1 => (0..COMMODITIES.len()).prop_map(Directive::Commodity),
        1 => (0..ACCOUNTS.len(), prop::option::of(0..COMMODITIES.len()))
            .prop_map(|(account, commodity)| Directive::Open { account, commodity }),
        1 => (0..ACCOUNTS.len()).prop_map(|account| Directive::Close { account }),
        1 => (0..PAYEES.len()).prop_map(Directive::Payee),
        1 => (0..ACCOUNTS.len(), -100_000i64..=100_000, any::<bool>(), 0..COMMODITIES.len())
            .prop_map(|(account, cents, tolerance, commodity)| Directive::Balance {
                account,
                cents,
                tolerance,
                commodity,
            }),
        1 => Just(Directive::Include),
        1 => Just(Directive::Comment),
        3 => transaction_directive(),
    ]
}

fn journal_source() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(1usize..=12, 1..8),
        prop::collection::vec(directive(), 1..6),
        prop::collection::vec(1usize..=2, 1..4),
    )
        .prop_map(|(widths, directives, gaps)| {
            let mut spacing = Spacing::new(widths);
            let mut source = String::new();
            for (index, directive) in directives.iter().enumerate() {
                if index > 0 {
                    for _ in 0..gaps[index % gaps.len()] {
                        source.push('\n');
                    }
                }
                source.push_str(&directive.render(&mut spacing));
            }
            source.push('\n');
            source
        })
}

fn lowered_entries(source: &str) -> Vec<Entry> {
    analyze_file(source)
        .entries()
        .iter()
        .map(|entry| entry.entry.clone())
        .collect()
}

proptest! {
    #[test]
    fn every_valid_document_is_accepted(source in journal_source()) {
        prop_assert!(
            format(&source).is_ok(),
            "the formatter refused a valid document: {source:?}"
        );
    }

    #[test]
    fn formatting_is_idempotent(source in journal_source()) {
        let once = format(&source).expect("a valid document formats");
        let twice = format(&once).expect("formatted output is still valid");

        prop_assert_eq!(twice, once);
    }

    #[test]
    fn formatting_preserves_the_lowered_entries(source in journal_source()) {
        let formatted = format(&source).expect("a valid document formats");
        let before = lowered_entries(&source);
        let after = lowered_entries(&formatted);

        prop_assert_eq!(
            &after,
            &before,
            "formatting changed the entries of {:?}",
            source
        );
    }

    #[test]
    fn formatted_output_is_diagnostic_free(source in journal_source()) {
        let formatted = format(&source).expect("a valid document formats");
        let file = analyze_file(&formatted);

        prop_assert!(
            file.syntax_diagnostics().is_empty(),
            "formatted output has syntax diagnostics: {:?}\n{formatted}",
            file.syntax_diagnostics()
        );
        prop_assert!(
            file.lowering_diagnostics().is_empty(),
            "formatted output has lowering diagnostics: {:?}\n{formatted}",
            file.lowering_diagnostics()
        );
    }

    #[test]
    fn transactions_round_trip_through_the_serializer(
        generated in prop::collection::vec(
            (
                prop::option::of(0..PAYEES.len()),
                prop::option::of(0..NARRATIONS.len()),
                prop::collection::vec(
                    (
                        0..ACCOUNTS.len(),
                        prop::option::of(-100_000i64..=100_000),
                        0..COMMODITIES.len(),
                        prop::option::of(metadata()),
                        prop::option::of((0..2usize, 1i64..=100_000, 0..COMMODITIES.len())),
                    ),
                    1..4,
                ),
                prop::option::of(metadata()),
            ),
            1..4,
        ),
    ) {
        let transactions: Vec<Transaction> = generated
            .into_iter()
            .map(|(payee, narration, postings, meta)| Transaction {
                date: date(),
                effective_date: None,
                status: TransactionStatus::Cleared,
                payee: Some(PAYEES[payee.unwrap_or(0)].to_string()),
                narration: narration.map(|index| NARRATIONS[index].to_string()),
                postings: postings
                    .into_iter()
                    .map(|(account, cents, commodity, posting_meta, cost)| {
                        let amount = cents.map(|cents| Amount {
                            number: Decimal::new(cents, 2),
                            commodity: CommodityName::new(COMMODITIES[commodity])
                                .expect("a valid commodity"),
                        });
                        let transaction_cost = cost.and_then(|(annotation, cents, commodity)| {
                            amount
                                .as_ref()
                                .is_some_and(|amount| !amount.number.is_zero())
                                .then(|| pactole_core::TransactionCost {
                                    amount: Amount {
                                        number: Decimal::new(cents, 2),
                                        commodity: CommodityName::new(COMMODITIES[commodity])
                                            .expect("a valid commodity"),
                                    },
                                    annotation: if annotation == 0 {
                                        pactole_core::TransactionCostAnnotation::Unit
                                    } else {
                                        pactole_core::TransactionCostAnnotation::Total
                                    },
                                })
                        });
                        Posting {
                            account: AccountName::new(ACCOUNTS[account])
                                .expect("a valid account"),
                            amount,
                            transaction_cost,
                            meta: posting_meta
                                .map(|(key, value)| {
                                    [(MetadataKey::new(key).expect("a valid key"), value)]
                                        .into_iter()
                                        .collect()
                                })
                                .unwrap_or_default(),
                        }
                    })
                    .collect(),
                tags: Vec::new(),
                links: Vec::new(),
                reference: None,
                meta: meta
                    .map(|(key, value)| {
                        [(MetadataKey::new(key).expect("a valid key"), value)]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_else(Metadata::new),
            })
            .collect();

        let source = format_transactions(&transactions);
        let file = analyze_file(&source);

        prop_assert!(
            file.syntax_diagnostics().is_empty(),
            "serialized transactions do not parse: {:?}\n{source}",
            file.syntax_diagnostics()
        );
        prop_assert!(
            file.lowering_diagnostics().is_empty(),
            "serialized transactions do not lower: {:?}\n{source}",
            file.lowering_diagnostics()
        );

        let reparsed: Vec<Transaction> = file
            .entries()
            .iter()
            .filter_map(|entry| match &entry.entry {
                Entry::Transaction(transaction) => Some(transaction.clone()),
                _ => None,
            })
            .collect();
        prop_assert_eq!(&reparsed, &transactions);

        // The serializer's output is already canonical.
        prop_assert_eq!(format(&source).expect("serialized output is valid"), source);
    }
}
