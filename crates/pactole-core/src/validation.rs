use crate::errors::ValidationError;
use crate::models::{AccountName, Amount, CommodityName, Entry, Journal, Transaction};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashSet};

/// Validates a [`Journal`] from a business point of view, on top of the
/// purely grammatical validation already performed while building each
/// entry (see [`crate::ModelError`]).
///
/// This:
/// - sorts every entry chronologically (a stable sort, so entries sharing
///   the same date keep their original relative order),
/// - checks that an account is open (i.e. has been `open`ed and not yet
///   `close`d) before being used in a transaction posting, a balance
///   assertion, or being closed itself,
/// - checks that a commodity has been declared before being used in a
///   transaction posting,
/// - balances every transaction: at most one posting may be left without
///   an amount, in which case it is auto-filled from the others, and the
///   postings for each commodity must sum to zero.
///
/// On success, returns the same journal, sorted and with transactions
/// auto-balanced.
pub fn validate_journal(mut journal: Journal) -> Result<Journal, ValidationError> {
    journal.entries.sort_by_key(Entry::date);

    let mut open_accounts: HashSet<AccountName> = HashSet::new();
    let mut declared_commodities: HashSet<CommodityName> = HashSet::new();

    for entry in &mut journal.entries {
        match entry {
            Entry::Open(open) => {
                open_accounts.insert(open.account.clone());
            }
            Entry::Close(close) => {
                if !open_accounts.remove(&close.account) {
                    return Err(ValidationError::AccountNotOpen {
                        account: close.account.clone(),
                        date: close.date,
                    });
                }
            }
            Entry::Commodity(commodity) => {
                declared_commodities.insert(commodity.name.clone());
            }
            Entry::Balance(balance) => {
                if !open_accounts.contains(&balance.account) {
                    return Err(ValidationError::AccountNotOpen {
                        account: balance.account.clone(),
                        date: balance.date,
                    });
                }
            }
            Entry::Transaction(transaction) => {
                validate_transaction(transaction, &open_accounts, &declared_commodities)?;
            }
        }
    }

    Ok(journal)
}

/// Checks that every posting of `transaction` uses an open account and a
/// declared commodity, then balances it.
fn validate_transaction(
    transaction: &mut Transaction,
    open_accounts: &HashSet<AccountName>,
    declared_commodities: &HashSet<CommodityName>,
) -> Result<(), ValidationError> {
    for posting in &transaction.postings {
        if !open_accounts.contains(&posting.account) {
            return Err(ValidationError::AccountNotOpen {
                account: posting.account.clone(),
                date: transaction.date,
            });
        }
        if let Some(amount) = &posting.amount {
            if !declared_commodities.contains(&amount.commodity) {
                return Err(ValidationError::CommodityNotDeclared {
                    commodity: amount.commodity.clone(),
                    date: transaction.date,
                });
            }
        }
    }

    balance_transaction(transaction)
}

/// Validates and auto-balances a transaction's postings.
///
/// At most one posting may be left without an amount: if that is the
/// case, and the other postings all share the same commodity, the
/// missing amount is computed and filled in so that the transaction
/// balances. If more than one posting has no amount, or the known
/// amounts use several commodities while one is missing, an error is
/// returned. If every posting already has an amount, the postings must
/// sum to zero for each commodity.
fn balance_transaction(transaction: &mut Transaction) -> Result<(), ValidationError> {
    let missing: Vec<usize> = transaction
        .postings
        .iter()
        .enumerate()
        .filter(|(_, posting)| posting.amount.is_none())
        .map(|(index, _)| index)
        .collect();

    if missing.len() > 1 {
        return Err(ValidationError::TooManyPostingsWithoutAmount(
            missing.len(),
        ));
    }

    let mut sums: BTreeMap<&str, (CommodityName, Decimal)> = BTreeMap::new();
    for posting in &transaction.postings {
        if let Some(amount) = &posting.amount {
            sums.entry(amount.commodity.as_str())
                .or_insert_with(|| (amount.commodity.clone(), Decimal::ZERO))
                .1 += amount.number;
        }
    }

    if let Some(index) = missing.first().copied() {
        if sums.len() != 1 {
            let commodities: Vec<String> = sums.keys().map(|k| k.to_string()).collect();
            return Err(ValidationError::AmbiguousAutoBalanceCommodity(commodities));
        }

        let (_, (commodity, sum)) = sums.into_iter().next().unwrap();
        transaction.postings[index].amount = Some(Amount {
            number: -sum,
            commodity,
        });
        return Ok(());
    }

    for (_, (commodity, sum)) in sums {
        if !sum.is_zero() {
            return Err(ValidationError::TransactionNotBalanced(
                commodity.as_str().to_string(),
                sum,
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Balance, Close, Commodity, Metadata, Open, Posting, TransactionStatus};

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
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

    fn transaction(d: chrono::NaiveDate, postings: Vec<Posting>) -> Transaction {
        Transaction {
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
        }
    }

    fn open(d: chrono::NaiveDate, account: &str) -> Entry {
        Entry::Open(Open {
            date: d,
            account: AccountName::new(account).unwrap(),
            description: None,
            meta: Metadata::new(),
        })
    }

    fn close(d: chrono::NaiveDate, account: &str) -> Entry {
        Entry::Close(Close {
            date: d,
            account: AccountName::new(account).unwrap(),
            meta: Metadata::new(),
        })
    }

    fn commodity(d: chrono::NaiveDate, name: &str) -> Entry {
        Entry::Commodity(Commodity {
            date: d,
            name: CommodityName::new(name).unwrap(),
            meta: Metadata::new(),
        })
    }

    fn balance(d: chrono::NaiveDate, account: &str, amount: (&str, &str)) -> Entry {
        Entry::Balance(Balance {
            date: d,
            account: AccountName::new(account).unwrap(),
            amount: Amount {
                number: amount.0.parse().unwrap(),
                commodity: CommodityName::new(amount.1).unwrap(),
            },
            tolerance: None,
            meta: Metadata::new(),
        })
    }

    #[test]
    fn validates_a_well_formed_journal_and_sorts_entries() {
        let journal = Journal {
            entries: vec![
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Depenses:Divers", None),
                    ],
                )),
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
            ],
        };

        let validated = validate_journal(journal).unwrap();

        // Entries were sorted chronologically.
        assert_eq!(validated.entries[0].date(), date(2024, 1, 1));
        assert_eq!(validated.entries.last().unwrap().date(), date(2024, 1, 5));

        // The transaction's missing amount was auto-filled.
        let Entry::Transaction(txn) = validated.entries.last().unwrap() else {
            panic!("expected a transaction");
        };
        assert_eq!(
            txn.postings[1].amount,
            Some(Amount {
                number: "-100.00".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            })
        );
    }

    #[test]
    fn rejects_transaction_posting_on_unopened_account() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("-100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AccountNotOpen {
                account: AccountName::new("Actifs:Compte").unwrap(),
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn rejects_balance_assertion_on_unopened_account() {
        let journal = Journal {
            entries: vec![balance(date(2024, 1, 1), "Actifs:Compte", ("0.00", "EUR"))],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AccountNotOpen {
                account: AccountName::new("Actifs:Compte").unwrap(),
                date: date(2024, 1, 1),
            })
        );
    }

    #[test]
    fn rejects_closing_an_unopened_account() {
        let journal = Journal {
            entries: vec![close(date(2024, 1, 1), "Actifs:Compte")],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AccountNotOpen {
                account: AccountName::new("Actifs:Compte").unwrap(),
                date: date(2024, 1, 1),
            })
        );
    }

    #[test]
    fn accepts_balance_on_account_closed_afterwards() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                balance(date(2024, 1, 2), "Actifs:Compte", ("0.00", "EUR")),
                close(date(2024, 1, 3), "Actifs:Compte"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_balance_on_account_closed_before() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                close(date(2024, 1, 2), "Actifs:Compte"),
                balance(date(2024, 1, 3), "Actifs:Compte", ("0.00", "EUR")),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AccountNotOpen {
                account: AccountName::new("Actifs:Compte").unwrap(),
                date: date(2024, 1, 3),
            })
        );
    }

    #[test]
    fn rejects_transaction_posting_with_undeclared_commodity() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("-100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::CommodityNotDeclared {
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn rejects_unbalanced_transaction() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("-50.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::TransactionNotBalanced(
                "EUR".to_string(),
                "50.00".parse().unwrap()
            ))
        );
    }

    #[test]
    fn rejects_more_than_one_missing_amount() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![posting("Actifs:Compte", None), posting("Depenses:Divers", None)],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::TooManyPostingsWithoutAmount(2))
        );
    }

    #[test]
    fn rejects_ambiguous_commodity_for_missing_amount() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Actifs:Autre"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Actifs:Autre", Some(("50.00", "USD"))),
                        posting("Depenses:Divers", None),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AmbiguousAutoBalanceCommodity(vec![
                "EUR".to_string(),
                "USD".to_string()
            ]))
        );
    }
}
