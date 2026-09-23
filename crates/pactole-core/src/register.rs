use crate::models::{AccountName, Amount, CommodityName, Entry, Journal, Transaction, TransactionStatus};
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// Filters used to select which postings are included in a [`register`]
/// listing.
///
/// Every field is optional; unset fields simply do not filter anything.
/// When several filters are set, an entry must satisfy all of them
/// (logical AND).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegisterFilter {
    /// Only keep postings on this account, or on one of its
    /// sub-accounts (e.g. filtering on `Depenses` also matches
    /// `Depenses:Alimentation`).
    pub account: Option<AccountName>,

    /// Only keep transactions on or after this date.
    pub from: Option<NaiveDate>,

    /// Only keep transactions on or before this date.
    pub to: Option<NaiveDate>,

    /// Only keep transactions whose payee contains this text
    /// (case-insensitive).
    pub payee: Option<String>,

    /// Only keep transactions whose narration contains this text
    /// (case-insensitive).
    pub narration: Option<String>,

    /// Only keep transactions carrying this tag.
    pub tag: Option<String>,

    /// Only keep transactions with this status.
    pub status: Option<TransactionStatus>,
}

/// A single line of a [`register`] listing: one matched posting, along
/// with the transaction it belongs to and the running balance of the
/// register up to (and including) this line.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisterEntry {
    pub date: NaiveDate,
    pub status: TransactionStatus,
    pub payee: Option<String>,
    pub narration: Option<String>,
    pub account: AccountName,
    pub amount: Option<Amount>,

    /// Running balance of the register, per commodity, after this
    /// entry. Only postings with an amount contribute to it.
    pub balance: Vec<(CommodityName, Decimal)>,
}

/// Lists the postings of `journal` matching `filter`, in chronological
/// order (the journal is expected to already be sorted, e.g. by
/// [`crate::validate_journal`]), together with a running balance kept
/// per commodity across the whole listing.
///
/// This is akin to the `register` report found in other plain-text
/// accounting tools (Ledger, hledger, Beancount): it is meant to answer
/// "what happened on this account/payee/period?" rather than "what is
/// the current balance?" (for that, see balance assertions).
pub fn register(journal: &Journal, filter: &RegisterFilter) -> Vec<RegisterEntry> {
    // Kept as a `Vec` (rather than e.g. a `BTreeMap`) since `CommodityName`
    // has no ordering; the handful of commodities used in a single
    // register listing makes the linear lookup below a non-issue.
    let mut running: Vec<(CommodityName, Decimal)> = Vec::new();
    let mut entries = Vec::new();

    for entry in journal.entries() {
        let Entry::Transaction(transaction) = entry else {
            continue;
        };

        if !transaction_matches(transaction, filter) {
            continue;
        }

        for posting in &transaction.postings {
            if !account_matches(&posting.account, filter.account.as_ref()) {
                continue;
            }

            if let Some(amount) = &posting.amount {
                match running
                    .iter_mut()
                    .find(|(commodity, _)| commodity == &amount.commodity)
                {
                    Some((_, sum)) => *sum += amount.number,
                    None => running.push((amount.commodity.clone(), amount.number)),
                }
            }

            entries.push(RegisterEntry {
                date: transaction.date,
                status: transaction.status.clone(),
                payee: transaction.payee.clone(),
                narration: transaction.narration.clone(),
                account: posting.account.clone(),
                amount: posting.amount.clone(),
                balance: running.clone(),
            });
        }
    }

    entries
}

fn transaction_matches(transaction: &Transaction, filter: &RegisterFilter) -> bool {
    if let Some(from) = filter.from {
        if transaction.date < from {
            return false;
        }
    }
    if let Some(to) = filter.to {
        if transaction.date > to {
            return false;
        }
    }
    if let Some(status) = &filter.status {
        if &transaction.status != status {
            return false;
        }
    }
    if let Some(payee) = &filter.payee {
        if !contains_case_insensitive(transaction.payee.as_deref(), payee) {
            return false;
        }
    }
    if let Some(narration) = &filter.narration {
        if !contains_case_insensitive(transaction.narration.as_deref(), narration) {
            return false;
        }
    }
    if let Some(tag) = &filter.tag {
        if !transaction.tags.iter().any(|t| t == tag) {
            return false;
        }
    }

    true
}

fn contains_case_insensitive(haystack: Option<&str>, needle: &str) -> bool {
    match haystack {
        Some(haystack) => haystack.to_lowercase().contains(&needle.to_lowercase()),
        None => false,
    }
}

/// An account matches the filter if no account filter is set, if it is
/// exactly the filtered account, or if it is one of its sub-accounts
/// (i.e. its name starts with `<filter>:`).
fn account_matches(account: &AccountName, filter: Option<&AccountName>) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    let account = account.as_str();
    let filter = filter.as_str();

    account == filter || account.starts_with(&format!("{filter}:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Metadata, Posting};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn posting(account: &str, amount: Option<(&str, &str)>) -> Posting {
        Posting {
            account: AccountName::new(account).unwrap(),
            amount: amount.map(|(number, commodity)| Amount {
                number: number.parse().unwrap(),
                commodity: CommodityName::new(commodity).unwrap(),
            }),
            meta: Metadata::new(),
        }
    }

    fn transaction(
        d: NaiveDate,
        payee: Option<&str>,
        narration: Option<&str>,
        tags: Vec<&str>,
        status: TransactionStatus,
        postings: Vec<Posting>,
    ) -> Entry {
        Entry::Transaction(Transaction {
            date: d,
            effective_date: None,
            status,
            payee: payee.map(str::to_string),
            narration: narration.map(str::to_string),
            postings,
            tags: tags.into_iter().map(str::to_string).collect(),
            links: Vec::new(),
            reference: None,
            meta: Metadata::new(),
        })
    }

    #[test]
    fn lists_every_posting_with_running_balance_when_unfiltered() {
        let journal = Journal {
            entries: vec![
                transaction(
                    date(2024, 1, 5),
                    Some("Carrefour"),
                    Some("Courses"),
                    vec![],
                    TransactionStatus::Cleared,
                    vec![
                        posting("Depenses:Alimentation", Some(("45.30", "EUR"))),
                        posting("Actifs:Compte", Some(("-45.30", "EUR"))),
                    ],
                ),
                transaction(
                    date(2024, 1, 10),
                    Some("Carrefour"),
                    None,
                    vec![],
                    TransactionStatus::Cleared,
                    vec![
                        posting("Depenses:Alimentation", Some(("10.00", "EUR"))),
                        posting("Actifs:Compte", Some(("-10.00", "EUR"))),
                    ],
                ),
            ],
        };

        let entries = register(&journal, &RegisterFilter::default());

        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries[3].balance,
            vec![(CommodityName::new("EUR").unwrap(), "0.00".parse().unwrap())]
        );
    }

    #[test]
    fn filters_by_account_including_sub_accounts() {
        let journal = Journal {
            entries: vec![transaction(
                date(2024, 1, 5),
                None,
                None,
                vec![],
                TransactionStatus::Cleared,
                vec![
                    posting("Depenses:Alimentation", Some(("45.30", "EUR"))),
                    posting("Actifs:Compte", Some(("-45.30", "EUR"))),
                ],
            )],
        };

        let entries = register(
            &journal,
            &RegisterFilter {
                account: Some(AccountName::new("Depenses").unwrap()),
                ..Default::default()
            },
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].account.as_str(), "Depenses:Alimentation");
    }

    #[test]
    fn filters_by_date_range() {
        let journal = Journal {
            entries: vec![
                transaction(
                    date(2024, 1, 5),
                    None,
                    None,
                    vec![],
                    TransactionStatus::Cleared,
                    vec![posting("Actifs:Compte", Some(("1.00", "EUR")))],
                ),
                transaction(
                    date(2024, 2, 5),
                    None,
                    None,
                    vec![],
                    TransactionStatus::Cleared,
                    vec![posting("Actifs:Compte", Some(("2.00", "EUR")))],
                ),
            ],
        };

        let entries = register(
            &journal,
            &RegisterFilter {
                from: Some(date(2024, 2, 1)),
                ..Default::default()
            },
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, date(2024, 2, 5));
    }

    #[test]
    fn filters_by_payee_case_insensitively() {
        let journal = Journal {
            entries: vec![transaction(
                date(2024, 1, 5),
                Some("Carrefour"),
                None,
                vec![],
                TransactionStatus::Cleared,
                vec![posting("Actifs:Compte", Some(("1.00", "EUR")))],
            )],
        };

        let entries = register(
            &journal,
            &RegisterFilter {
                payee: Some("carre".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn filters_by_tag_and_status() {
        let journal = Journal {
            entries: vec![
                transaction(
                    date(2024, 1, 5),
                    None,
                    None,
                    vec!["vacances"],
                    TransactionStatus::Cleared,
                    vec![posting("Actifs:Compte", Some(("1.00", "EUR")))],
                ),
                transaction(
                    date(2024, 1, 6),
                    None,
                    None,
                    vec![],
                    TransactionStatus::Pending,
                    vec![posting("Actifs:Compte", Some(("2.00", "EUR")))],
                ),
            ],
        };

        let entries = register(
            &journal,
            &RegisterFilter {
                tag: Some("vacances".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, date(2024, 1, 5));

        let entries = register(
            &journal,
            &RegisterFilter {
                status: Some(TransactionStatus::Pending),
                ..Default::default()
            },
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, date(2024, 1, 6));
    }
}
