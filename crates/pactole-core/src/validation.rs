//! Business validation and auto-balancing for Pactole journals.

use crate::errors::ValidationError;
use crate::models::{AccountName, Amount, CommodityName, Entry, Journal, Transaction};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A complete validation result, including the normalized journal and all
/// non-cascading issues found while walking it.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationReport {
    /// Chronologically sorted journal, with inferable amounts filled in.
    pub journal: Journal,
    /// Issues paired with their positions in the input journal.
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Returns the validation errors in the same order as [`Self::issues`].
    pub fn errors(&self) -> impl Iterator<Item = &ValidationError> {
        self.issues.iter().map(|issue| &issue.error)
    }
}

/// A validation error together with the index of the offending entry in the
/// journal passed to [`validate_journal_all`], before chronological sorting.
///
/// The index is deliberately a domain-level location, not a source span:
/// `pactole-core` must remain independent from parsers and file storage. A
/// storage layer can use it to recover the originating file and source span.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    /// Zero-based position in the journal supplied to validation, before the
    /// validator's chronological sort.
    pub input_index: usize,
    /// Business error associated with the input entry.
    pub error: ValidationError,
}

/// Validates a journal and returns the first business error, if any.
///
/// On success, returns the journal sorted chronologically and with inferable
/// transaction amounts filled in. Use [`validate_journal_all`] to retain every
/// non-cascading issue and its input index.
pub fn validate_journal(journal: Journal) -> Result<Journal, ValidationError> {
    let report = validate_journal_all(journal);
    match report.issues.first() {
        Some(issue) => Err(issue.error.clone()),
        None => Ok(report.journal),
    }
}

/// Validates every entry and returns all errors that can be diagnosed without
/// aborting the journal walk. Invalid entries do not mutate validation state,
/// preventing one error from causing a cascade of misleading later errors.
///
/// The returned journal is chronologically sorted with stable ordering for
/// equal dates. Payee declarations, which have no date, are available before
/// dated entries. Validation checks account lifecycles, declarations,
/// transaction balancing and balance assertions; at most one missing posting
/// amount is inferred when its commodity is unambiguous.
pub fn validate_journal_all(journal: Journal) -> ValidationReport {
    let mut indexed_entries: Vec<(usize, Entry)> =
        journal.entries.into_iter().enumerate().collect();
    indexed_entries.sort_by_key(|(_, entry)| entry.date());

    // Maps each open account to the (possibly empty) list of commodities
    // it is restricted to, as declared on its `open` directive. An empty
    // list means the account is unrestricted.
    let mut open_accounts: HashMap<AccountName, Vec<CommodityName>> = HashMap::new();
    let mut declared_commodities: HashSet<CommodityName> = HashSet::new();
    let mut declared_payees: HashSet<String> = HashSet::new();
    // Running balance, per (account, commodity) pair, of every posting
    // seen so far, used to check `balance` assertions.
    let mut running_balances: HashMap<(AccountName, CommodityName), Decimal> = HashMap::new();
    let mut issues = Vec::new();

    for (input_index, entry) in &mut indexed_entries {
        match entry {
            Entry::Open(open) => {
                if open_accounts.contains_key(&open.account) {
                    let error = ValidationError::AccountAlreadyOpen {
                        account: open.account.clone(),
                        date: open.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }
                open_accounts.insert(open.account.clone(), open.commodities.clone());
            }
            Entry::Close(close) => {
                if open_accounts.remove(&close.account).is_none() {
                    let error = ValidationError::AccountNotOpen {
                        account: close.account.clone(),
                        date: close.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }
            }
            Entry::Commodity(commodity) => {
                if !declared_commodities.insert(commodity.name.clone()) {
                    let error = ValidationError::CommodityAlreadyDeclared {
                        commodity: commodity.name.clone(),
                        date: commodity.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }
            }
            Entry::Payee(payee) => {
                if !declared_payees.insert(payee.name.clone()) {
                    let error = ValidationError::PayeeAlreadyDeclared {
                        payee: payee.name.clone(),
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }
            }
            Entry::Balance(balance) => {
                let Some(allowed_commodities) = open_accounts.get(&balance.account) else {
                    let error = ValidationError::AccountNotOpen {
                        account: balance.account.clone(),
                        date: balance.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                };

                if !declared_commodities.contains(&balance.amount.commodity) {
                    let error = ValidationError::CommodityNotDeclared {
                        commodity: balance.amount.commodity.clone(),
                        date: balance.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }

                if !allowed_commodities.is_empty()
                    && !allowed_commodities.contains(&balance.amount.commodity)
                {
                    let error = ValidationError::CommodityNotAllowed {
                        account: balance.account.clone(),
                        commodity: balance.amount.commodity.clone(),
                        date: balance.date,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }

                let key = (balance.account.clone(), balance.amount.commodity.clone());
                let actual = running_balances.get(&key).copied().unwrap_or(Decimal::ZERO);
                let expected = balance.amount.number;
                let tolerance = balance.tolerance.unwrap_or(Decimal::ZERO);
                if (actual - expected).abs() > tolerance {
                    let error = ValidationError::BalanceAssertionFailed {
                        account: balance.account.clone(),
                        commodity: balance.amount.commodity.clone(),
                        date: balance.date,
                        expected,
                        actual,
                        tolerance,
                    };
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                    continue;
                }
            }
            Entry::Transaction(transaction) => {
                let transaction_errors = validate_transaction(
                    transaction,
                    &open_accounts,
                    &declared_commodities,
                    &declared_payees,
                );
                let has_transaction_errors = !transaction_errors.is_empty();
                for error in transaction_errors {
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error,
                    });
                }
                if has_transaction_errors {
                    continue;
                }

                for posting in &transaction.postings {
                    if let Some(amount) = &posting.amount {
                        let key = (posting.account.clone(), amount.commodity.clone());
                        *running_balances.entry(key).or_insert(Decimal::ZERO) += amount.number;
                    }
                }
            }
        }
    }

    ValidationReport {
        journal: Journal {
            entries: indexed_entries
                .into_iter()
                .map(|(_, entry)| entry)
                .collect(),
        },
        issues,
    }
}

/// Checks that every posting of `transaction` uses an open account and a
/// declared commodity, and — when the account is restricted to a list of
/// commodities — that it uses one of them, then balances it, and finally
/// checks the payee: a transaction must carry a payee (`PayeeMissing`
/// otherwise) and, when set, that payee must have been declared
/// (`PayeeNotDeclared`).
fn validate_transaction(
    transaction: &mut Transaction,
    open_accounts: &HashMap<AccountName, Vec<CommodityName>>,
    declared_commodities: &HashSet<CommodityName>,
    declared_payees: &HashSet<String>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for posting in &transaction.postings {
        if !open_accounts.contains_key(&posting.account) {
            errors.push(ValidationError::AccountNotOpen {
                account: posting.account.clone(),
                date: transaction.date,
            });
        }
        if let Some(amount) = &posting.amount
            && !declared_commodities.contains(&amount.commodity)
        {
            errors.push(ValidationError::CommodityNotDeclared {
                commodity: amount.commodity.clone(),
                date: transaction.date,
            });
        }
    }

    if errors.is_empty()
        && let Err(error) = balance_transaction(transaction)
    {
        errors.push(error);
    }

    if errors.is_empty() {
        // Enforce `open`'s commodity restrictions after balancing, so that
        // an auto-balanced posting's inferred commodity is checked too.
        for posting in &transaction.postings {
            let allowed = match open_accounts.get(&posting.account) {
                Some(allowed) => allowed,
                None => {
                    unreachable!("account presence was checked before commodity restrictions")
                }
            };
            if allowed.is_empty() {
                continue;
            }
            if let Some(amount) = &posting.amount
                && !allowed.contains(&amount.commodity)
            {
                errors.push(ValidationError::CommodityNotAllowed {
                    account: posting.account.clone(),
                    commodity: amount.commodity.clone(),
                    date: transaction.date,
                });
            }
        }
    }

    // The payee is optional in the grammar (so a transaction may have none);
    // a missing payee is reported as `PayeeMissing`, and a payee that does
    // not match any declaration as `PayeeNotDeclared`. Checked last so that
    // posting- and balance-level problems are reported ahead of it, but
    // always appended so both kinds surface together in a full report.
    match &transaction.payee {
        Some(payee) if !declared_payees.contains(payee) => {
            errors.push(ValidationError::PayeeNotDeclared {
                payee: payee.clone(),
                date: transaction.date,
            });
        }
        None => {
            errors.push(ValidationError::PayeeMissing {
                date: transaction.date,
            });
        }
        _ => {}
    }

    errors
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
        return Err(ValidationError::TooManyPostingsWithoutAmount {
            count: missing.len(),
            date: transaction.date,
        });
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
            return Err(ValidationError::AmbiguousAutoBalanceCommodity {
                commodities,
                date: transaction.date,
            });
        }

        let Some((_, (commodity, sum))) = sums.into_iter().next() else {
            unreachable!("a missing amount requires exactly one commodity sum");
        };
        transaction.postings[index].amount = Some(Amount {
            number: -sum,
            commodity,
        });
        return Ok(());
    }

    for (_, (commodity, sum)) in sums {
        if !sum.is_zero() {
            return Err(ValidationError::TransactionNotBalanced {
                commodity: commodity.as_str().to_string(),
                sum,
                date: transaction.date,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Balance, Close, Commodity, Metadata, Open, Payee, Posting, TransactionStatus,
    };

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

    fn transaction_with_payee(
        d: chrono::NaiveDate,
        payee: &str,
        postings: Vec<Posting>,
    ) -> Transaction {
        Transaction {
            payee: Some(payee.to_string()),
            ..transaction(d, postings)
        }
    }

    #[test]
    fn validation_issue_index_refers_to_the_unsorted_input() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 2, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Actifs:Compte"),
            ],
        };

        let report = validate_journal_all(journal);

        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].input_index, 0);
        assert!(matches!(
            report.issues[0].error,
            ValidationError::AccountAlreadyOpen { .. }
        ));
        assert_eq!(report.journal.entries[0].date(), Some(date(2024, 1, 1)));
        assert_eq!(report.errors().count(), 1);
    }

    fn payee(name: &str) -> Entry {
        Entry::Payee(Payee {
            name: name.to_string(),
            meta: Metadata::new(),
        })
    }

    fn open(d: chrono::NaiveDate, account: &str) -> Entry {
        open_restricted(d, account, &[])
    }

    fn open_restricted(d: chrono::NaiveDate, account: &str, commodities: &[&str]) -> Entry {
        Entry::Open(Open {
            date: d,
            account: AccountName::new(account).unwrap(),
            commodities: commodities
                .iter()
                .map(|c| CommodityName::new(*c).unwrap())
                .collect(),
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
        balance_with_tolerance(d, account, amount, None)
    }

    fn balance_with_tolerance(
        d: chrono::NaiveDate,
        account: &str,
        amount: (&str, &str),
        tolerance: Option<&str>,
    ) -> Entry {
        Entry::Balance(Balance {
            date: d,
            account: AccountName::new(account).unwrap(),
            amount: Amount {
                number: amount.0.parse().unwrap(),
                commodity: CommodityName::new(amount.1).unwrap(),
            },
            tolerance: tolerance.map(|t| t.parse().unwrap()),
            meta: Metadata::new(),
        })
    }

    #[test]
    fn validates_a_well_formed_journal_and_sorts_entries() {
        let journal = Journal {
            entries: vec![
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("100.00", "EUR"))),
                        posting("Depenses:Divers", None),
                    ],
                )),
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                payee("Payee"),
            ],
        };

        let validated = validate_journal(journal).unwrap();

        // The undated payee declaration sorts first (it must be available
        // before any dated entry), then dated entries chronologically.
        assert!(matches!(validated.entries[0], Entry::Payee(_)));
        assert_eq!(validated.entries[1].date(), Some(date(2024, 1, 1)));
        assert_eq!(
            validated.entries.last().unwrap().date(),
            Some(date(2024, 1, 5))
        );

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
    fn rejects_opening_an_already_open_account() {
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["EUR"]),
                open_restricted(date(2024, 2, 1), "Actifs:Compte", &["USD"]),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::AccountAlreadyOpen {
                account: AccountName::new("Actifs:Compte").unwrap(),
                date: date(2024, 2, 1),
            })
        );
    }

    #[test]
    fn accepts_reopening_an_account_after_closing_it() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                close(date(2024, 2, 1), "Actifs:Compte"),
                open(date(2024, 3, 1), "Actifs:Compte"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_declaring_a_commodity_twice() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 2, 1), "EUR"),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::CommodityAlreadyDeclared {
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 2, 1),
            })
        );
    }

    #[test]
    fn rejects_declaring_a_payee_twice() {
        let journal = Journal {
            entries: vec![payee("Carrefour"), payee("Carrefour")],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::PayeeAlreadyDeclared {
                payee: "Carrefour".to_string(),
            })
        );
    }

    #[test]
    fn accepts_distinct_commodities_and_payees() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
                payee("Carrefour"),
                payee("Cinema"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_balance_assertion_using_an_undeclared_commodity() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                balance(date(2024, 1, 5), "Actifs:Compte", ("0.00", "EUR")),
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
    fn rejects_balance_assertion_using_a_commodity_not_allowed_by_open() {
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
                commodity(date(2024, 1, 1), "EUR"),
                balance(date(2024, 1, 5), "Actifs:Compte", ("0.00", "EUR")),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::CommodityNotAllowed {
                account: AccountName::new("Actifs:Compte").unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn accepts_balance_on_account_closed_afterwards() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                commodity(date(2024, 1, 1), "EUR"),
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
            Err(ValidationError::TransactionNotBalanced {
                commodity: "EUR".to_string(),
                sum: "50.00".parse().unwrap(),
                date: date(2024, 1, 5),
            })
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
                    vec![
                        posting("Actifs:Compte", None),
                        posting("Depenses:Divers", None),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::TooManyPostingsWithoutAmount {
                count: 2,
                date: date(2024, 1, 5),
            })
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
            Err(ValidationError::AmbiguousAutoBalanceCommodity {
                commodities: vec!["EUR".to_string(), "USD".to_string()],
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn accepts_transaction_with_declared_payee_regardless_of_declaration_order() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Carrefour",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                // Declared *after* the transaction using it in the file:
                // this must still be accepted, since a payee has no
                // date and is not checked chronologically.
                payee("Carrefour"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_transaction_with_undeclared_payee() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Carrefour",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::PayeeNotDeclared {
                payee: "Carrefour".to_string(),
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn rejects_transaction_with_no_payee() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::PayeeMissing {
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn accepts_balance_assertion_matching_running_balance() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("-100.00", "EUR")),
                payee("Payee"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_balance_assertion_not_matching_running_balance() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("-50.00", "EUR")),
                payee("Payee"),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::BalanceAssertionFailed {
                account: AccountName::new("Actifs:Compte").unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 10),
                expected: "-50.00".parse().unwrap(),
                actual: "-100.00".parse().unwrap(),
                tolerance: Decimal::ZERO,
            })
        );
    }

    #[test]
    fn accepts_balance_assertion_within_tolerance() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.01", "EUR"))),
                        posting("Depenses:Divers", Some(("100.01", "EUR"))),
                    ],
                )),
                balance_with_tolerance(
                    date(2024, 1, 10),
                    "Actifs:Compte",
                    ("-100.00", "EUR"),
                    Some("0.01"),
                ),
                payee("Payee"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_balance_assertion_outside_tolerance() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.02", "EUR"))),
                        posting("Depenses:Divers", Some(("100.02", "EUR"))),
                    ],
                )),
                balance_with_tolerance(
                    date(2024, 1, 10),
                    "Actifs:Compte",
                    ("-100.00", "EUR"),
                    Some("0.01"),
                ),
                payee("Payee"),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::BalanceAssertionFailed {
                account: AccountName::new("Actifs:Compte").unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 10),
                expected: "-100.00".parse().unwrap(),
                actual: "-100.02".parse().unwrap(),
                tolerance: "0.01".parse().unwrap(),
            })
        );
    }

    #[test]
    fn balance_assertion_only_sees_entries_up_to_its_stable_sort_position() {
        // Two entries share the same date: the transaction and the
        // balance assertion. Since both are on 2024-01-05, the stable
        // sort keeps their original relative order, so the assertion
        // (declared after the transaction in the input) must see its
        // effect.
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 5), "Actifs:Compte", ("-100.00", "EUR")),
                payee("Payee"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn accepts_posting_using_a_commodity_allowed_by_open() {
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["EUR", "USD"]),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Payee",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                payee("Payee"),
            ],
        };

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn rejects_posting_using_a_commodity_not_allowed_by_open() {
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::CommodityNotAllowed {
                account: AccountName::new("Actifs:Compte").unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 5),
            })
        );
    }

    #[test]
    fn rejects_auto_balanced_posting_using_a_commodity_not_allowed_by_open() {
        // `Actifs:Compte`'s missing amount is auto-filled with EUR
        // (inferred from `Depenses:Divers`), but `Actifs:Compte` is
        // restricted to USD only: the auto-balanced posting must still be
        // rejected.
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", None),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert_eq!(
            validate_journal(journal),
            Err(ValidationError::CommodityNotAllowed {
                account: AccountName::new("Actifs:Compte").unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
                date: date(2024, 1, 5),
            })
        );
    }
}
