//! Business validation and auto-balancing for Pactole journals.

use crate::errors::ValidationError;
use crate::models::{
    AccountName, Amount, CommodityName, Entry, Journal, Transaction, TransactionCostAnnotation,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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

/// What a journal may be missing, when it was assembled from a project that
/// could not be analyzed completely.
///
/// Business validation uses it to tell a real problem apart from an artefact
/// of the missing data: a check whose outcome depends on the missing content
/// is not reported, while a check that does not depend on it — the balancing
/// of a transaction, for instance — still is.
///
/// The two axes are distinct:
///
/// - the *declaration* axis (`accounts`, `commodities`, `payees`) lists the
///   names whose declaration may be missing. The checks that depend on it
///   (`account-not-open`, `commodity-not-declared`, `payee-not-declared`,
///   `commodity-not-allowed`) are suppressed for those names only, and the
///   names are treated as if they had been declared;
/// - the *usage* axis (`used_accounts`, `used_commodities`, `used_payees`)
///   lists the names a missing transaction, balance assertion, price directive
///   or transaction-cost annotation may have used. The checks that depend on
///   it (`balance-assertion-failed`,
///   `account-unused`, `commodity-unused`, `payee-unused`) are suppressed for
///   those names only.
///
/// The default value describes a complete journal: every check is reported.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingData {
    /// Whether an unknown amount of content is missing — an include whose
    /// target could not be loaded, for instance.
    ///
    /// Every check that depends on the completeness of the journal is then
    /// uncertain, whatever the names below.
    pub everything: bool,
    /// Accounts whose `open` directive may be missing.
    pub accounts: BTreeSet<AccountName>,
    /// Commodities whose `commodity` directive may be missing.
    pub commodities: BTreeSet<CommodityName>,
    /// Payees whose `payee` directive may be missing.
    pub payees: BTreeSet<String>,
    /// Accounts whose postings or balance assertions may be missing.
    pub used_accounts: BTreeSet<AccountName>,
    /// Commodities whose amounts, price references or transaction-cost
    /// references may be missing.
    pub used_commodities: BTreeSet<CommodityName>,
    /// Payees whose transactions may be missing.
    pub used_payees: BTreeSet<String>,
}

impl MissingData {
    /// Whether the account's `open` directive may be missing.
    fn account_may_be_open(&self, account: &AccountName) -> bool {
        self.everything || self.accounts.contains(account)
    }

    /// Whether the commodity's `commodity` directive may be missing.
    fn commodity_may_be_declared(&self, commodity: &CommodityName) -> bool {
        self.everything || self.commodities.contains(commodity)
    }

    /// Whether the payee's `payee` directive may be missing.
    fn payee_may_be_declared(&self, payee: &str) -> bool {
        self.everything || self.payees.contains(payee)
    }

    /// Whether the account may have postings or balance assertions that the
    /// journal does not contain.
    fn account_may_be_used(&self, account: &AccountName) -> bool {
        self.everything || self.used_accounts.contains(account)
    }

    /// Whether the commodity may have amounts or price references that the
    /// journal does not contain.
    fn commodity_may_be_used(&self, commodity: &CommodityName) -> bool {
        self.everything || self.used_commodities.contains(commodity)
    }

    /// Whether the payee may have transactions that the journal does not
    /// contain.
    fn payee_may_be_used(&self, payee: &str) -> bool {
        self.everything || self.used_payees.contains(payee)
    }
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
///
/// This is the complete-journal entry point: use
/// [`validate_journal_all_with`] to validate a journal that may be missing
/// some of its content.
pub fn validate_journal_all(journal: Journal) -> ValidationReport {
    validate_journal_all_with(journal, &MissingData::default())
}

/// Validates every entry of a journal that may be missing some of its
/// content, and returns all errors that can be diagnosed without aborting the
/// journal walk.
///
/// `missing` describes what the journal may be missing. A check whose outcome
/// depends on the missing content is suppressed, so that a partial analysis
/// reports the problems it can prove instead of staying silent:
///
/// - the checks that only depend on the entry they are about — the balancing
///   of a transaction, a duplicated declaration — are always reported;
/// - `account-not-open`, `commodity-not-declared`, `payee-not-declared` and
///   `commodity-not-allowed` are reported unless the name they are about may
///   have been declared in the missing content; a name that may have been
///   declared is treated as if it had been, so that the entries using it are
///   still validated (and still contribute to the running balances);
/// - `balance-assertion-failed` is reported unless the account may have
///   postings in the missing content.
///
/// The returned journal is chronologically sorted with stable ordering for
/// equal dates.
pub fn validate_journal_all_with(journal: Journal, missing: &MissingData) -> ValidationReport {
    let mut indexed_entries: Vec<(usize, Entry)> =
        journal.entries.into_iter().enumerate().collect();
    indexed_entries.sort_by_key(|(_, entry)| entry.date());

    // Maps each open account to the (possibly empty) list of commodities
    // it is restricted to, as declared on its `open` directive. An empty
    // list means the account is unrestricted.
    let mut open_accounts: HashMap<AccountName, Vec<CommodityName>> = HashMap::new();
    let mut declared_commodities: HashSet<CommodityName> = HashSet::new();
    let mut declared_payees: HashSet<String> = HashSet::new();
    // First quote for each dated, directed commodity pair. Identical repeats
    // are harmless; a different value on the same date is contradictory.
    let mut prices: BTreeMap<(NaiveDate, CommodityName, CommodityName), Decimal> = BTreeMap::new();
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
                if open_accounts.remove(&close.account).is_none()
                    && !missing.account_may_be_open(&close.account)
                {
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
            Entry::Price(price) => {
                let mut price_errors = Vec::new();
                for commodity in [&price.base, &price.quote.commodity] {
                    if !declared_commodities.contains(commodity)
                        && !missing.commodity_may_be_declared(commodity)
                    {
                        price_errors.push(ValidationError::CommodityNotDeclared {
                            commodity: commodity.clone(),
                            date: price.date,
                        });
                    }
                }
                if !price_errors.is_empty() {
                    issues.extend(price_errors.into_iter().map(|error| ValidationIssue {
                        input_index: *input_index,
                        error,
                    }));
                    continue;
                }
                if price.quote.number <= Decimal::ZERO {
                    issues.push(ValidationIssue {
                        input_index: *input_index,
                        error: ValidationError::PriceNotPositive {
                            base: price.base.clone(),
                            date: price.date,
                        },
                    });
                    continue;
                }

                let key = (
                    price.date,
                    price.base.clone(),
                    price.quote.commodity.clone(),
                );
                if let Some(first) = prices.get(&key) {
                    if *first != price.quote.number {
                        issues.push(ValidationIssue {
                            input_index: *input_index,
                            error: ValidationError::PriceConflict {
                                base: price.base.clone(),
                                quote: price.quote.commodity.clone(),
                                date: price.date,
                                first: *first,
                                second: price.quote.number,
                            },
                        });
                    }
                } else {
                    prices.insert(key, price.quote.number);
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
                let allowed_commodities: &[CommodityName] =
                    match open_accounts.get(&balance.account) {
                        Some(allowed) => allowed.as_slice(),
                        None => {
                            if !missing.account_may_be_open(&balance.account) {
                                let error = ValidationError::AccountNotOpen {
                                    account: balance.account.clone(),
                                    date: balance.date,
                                };
                                issues.push(ValidationIssue {
                                    input_index: *input_index,
                                    error,
                                });
                                continue;
                            }
                            // The account may be open, but the commodities it is
                            // restricted to are unknown: it is treated as
                            // unrestricted.
                            &[]
                        }
                    };

                if !declared_commodities.contains(&balance.amount.commodity)
                    && !missing.commodity_may_be_declared(&balance.amount.commodity)
                {
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

                // Postings missing from the journal may have changed the
                // running balance of the account: the assertion cannot be
                // checked.
                if missing.account_may_be_used(&balance.account) {
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
                    missing,
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
/// commodities — that it uses one of them, that its payee (when set) has
/// been declared, then balances it.
/// Reports declarations that no entry of the journal ever exercises.
///
/// This is the "unused declaration" check behind the `unused` key of the
/// `[validation]` section of `pactole.toml`. Unlike
/// [`validate_journal_all`], it does not validate or auto-balance anything:
/// it only reports declarations nothing depends on, paired with their
/// position in `journal` so a positioned front-end can point at them.
///
/// What counts as a use:
///
/// - an `open`ed account is used by a transaction posting or by a `balance`
///   assertion; merely `close`ing it does not count (an account exists to
///   hold postings, so opened-then-closed-without-ever-posting is reported);
/// - a declared commodity is used by a transaction posting amount or cost
///   annotation, by a `balance` assertion amount, by a `price` directive, or
///   by the restricted list of an `open` directive (an explicit reference to
///   the commodity);
/// - a declared payee is used by a transaction's payee.
///
/// A use counts even when the entry carrying it fails another validation
/// rule: the declaration is referenced either way, and reporting that is
/// the caller's concern.
///
/// The caller decides whether the returned issues matter at all — front-ends
/// map them to a severity of their own (the `unused` setting, a warning by
/// default), so this is deliberately not part of [`validate_journal_all`].
///
/// This is the complete-journal entry point: use [`unused_declarations_with`]
/// for a journal that may be missing some of its content.
#[must_use = "the unused-declaration issues are only useful when inspected"]
pub fn unused_declarations(journal: &Journal) -> Vec<ValidationIssue> {
    unused_declarations_with(journal, &MissingData::default())
}

/// Reports the declarations of `journal` that nothing uses, ignoring the
/// declarations a missing transaction or balance assertion may have used.
///
/// `missing` describes what the journal may be missing. A declaration whose
/// name may have been used by the missing content is not reported: the
/// missing content is exactly what could have used it.
#[must_use = "the unused-declaration issues are only useful when inspected"]
pub fn unused_declarations_with(journal: &Journal, missing: &MissingData) -> Vec<ValidationIssue> {
    // Declaration name -> (position in `journal`, declaration date).
    let mut opened_accounts: HashMap<AccountName, (usize, NaiveDate)> = HashMap::new();
    let mut declared_commodities: HashMap<CommodityName, (usize, NaiveDate)> = HashMap::new();
    let mut declared_payees: HashMap<String, usize> = HashMap::new();
    let mut used_accounts = HashSet::new();
    let mut used_commodities = HashSet::new();
    let mut used_payees = HashSet::new();

    for (input_index, entry) in journal.entries.iter().enumerate() {
        match entry {
            Entry::Open(open) => {
                // The first opening is the one validation considers real
                // (a duplicate one is reported separately as an error).
                opened_accounts
                    .entry(open.account.clone())
                    .or_insert((input_index, open.date));
                // Restricting an account to a list of commodities already
                // references those commodities.
                used_commodities.extend(open.commodities.iter().cloned());
            }
            // A `close` only requires the account to be open, never used.
            Entry::Close(_) => {}
            Entry::Commodity(commodity) => {
                declared_commodities
                    .entry(commodity.name.clone())
                    .or_insert((input_index, commodity.date));
            }
            Entry::Price(price) => {
                used_commodities.insert(price.base.clone());
                used_commodities.insert(price.quote.commodity.clone());
            }
            Entry::Payee(payee) => {
                declared_payees
                    .entry(payee.name.clone())
                    .or_insert(input_index);
            }
            Entry::Balance(balance) => {
                used_accounts.insert(balance.account.clone());
                used_commodities.insert(balance.amount.commodity.clone());
            }
            Entry::Transaction(transaction) => {
                if let Some(payee) = &transaction.payee {
                    used_payees.insert(payee.clone());
                }
                for posting in &transaction.postings {
                    used_accounts.insert(posting.account.clone());
                    if let Some(amount) = &posting.amount {
                        used_commodities.insert(amount.commodity.clone());
                    }
                    if let Some(cost) = &posting.transaction_cost {
                        used_commodities.insert(cost.amount.commodity.clone());
                    }
                }
            }
        }
    }

    let mut unused: Vec<(usize, ValidationError)> = Vec::new();

    for (payee, input_index) in &declared_payees {
        if !used_payees.contains(payee) && !missing.payee_may_be_used(payee) {
            unused.push((
                *input_index,
                ValidationError::PayeeUnused {
                    payee: payee.clone(),
                },
            ));
        }
    }
    for (account, (input_index, date)) in &opened_accounts {
        if !used_accounts.contains(account) && !missing.account_may_be_used(account) {
            unused.push((
                *input_index,
                ValidationError::AccountUnused {
                    account: account.clone(),
                    date: *date,
                },
            ));
        }
    }
    for (commodity, (input_index, date)) in &declared_commodities {
        if !used_commodities.contains(commodity) && !missing.commodity_may_be_used(commodity) {
            unused.push((
                *input_index,
                ValidationError::CommodityUnused {
                    commodity: commodity.clone(),
                    date: *date,
                },
            ));
        }
    }

    // Hash map iteration order is unspecified: report in journal order so
    // diagnostics are stable from one run to the next.
    unused.sort_by_key(|(input_index, _)| *input_index);
    unused
        .into_iter()
        .map(|(input_index, error)| ValidationIssue { input_index, error })
        .collect()
}

fn validate_transaction(
    transaction: &mut Transaction,
    open_accounts: &HashMap<AccountName, Vec<CommodityName>>,
    declared_commodities: &HashSet<CommodityName>,
    declared_payees: &HashSet<String>,
    missing: &MissingData,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(payee) = &transaction.payee
        && !declared_payees.contains(payee)
        && !missing.payee_may_be_declared(payee)
    {
        errors.push(ValidationError::PayeeNotDeclared {
            payee: payee.clone(),
            date: transaction.date,
        });
    }

    for posting in &transaction.postings {
        if !open_accounts.contains_key(&posting.account)
            && !missing.account_may_be_open(&posting.account)
        {
            errors.push(ValidationError::AccountNotOpen {
                account: posting.account.clone(),
                date: transaction.date,
            });
        }
        let mut checked_commodities = HashSet::new();
        if let Some(amount) = &posting.amount {
            checked_commodities.insert(amount.commodity.clone());
        }
        if let Some(amount) = &posting.amount
            && !declared_commodities.contains(&amount.commodity)
            && !missing.commodity_may_be_declared(&amount.commodity)
        {
            errors.push(ValidationError::CommodityNotDeclared {
                commodity: amount.commodity.clone(),
                date: transaction.date,
            });
        }
        if let Some(cost) = &posting.transaction_cost {
            if checked_commodities.insert(cost.amount.commodity.clone())
                && !declared_commodities.contains(&cost.amount.commodity)
                && !missing.commodity_may_be_declared(&cost.amount.commodity)
            {
                errors.push(ValidationError::CommodityNotDeclared {
                    commodity: cost.amount.commodity.clone(),
                    date: transaction.date,
                });
            }
            if cost.amount.number <= Decimal::ZERO {
                errors.push(ValidationError::TransactionCostNotPositive {
                    account: posting.account.clone(),
                    date: transaction.date,
                });
            } else if !posting.amount.as_ref().is_some_and(|amount| {
                !amount.number.is_zero()
                    && (cost.annotation == TransactionCostAnnotation::Total
                        || checked_exact_decimal_product(amount.number.abs(), cost.amount.number)
                            .is_some())
            }) {
                errors.push(ValidationError::TransactionCostInconsistent {
                    account: posting.account.clone(),
                    date: transaction.date,
                });
            }
        }
    }

    if !errors.is_empty() {
        return errors;
    }

    if let Err(error) = balance_transaction(transaction) {
        errors.push(error);
        return errors;
    }

    // Enforce `open`'s commodity restrictions after balancing, so that an
    // auto-balanced posting's inferred commodity is checked too.
    for posting in &transaction.postings {
        let allowed = match open_accounts.get(&posting.account) {
            Some(allowed) => allowed,
            // The account may be open, but the commodities it is restricted
            // to are unknown: it is treated as unrestricted.
            None if missing.account_may_be_open(&posting.account) => continue,
            None => unreachable!("account presence was checked before commodity restrictions"),
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

    errors
}
/// Validates and auto-balances a transaction's postings.
///
/// At most one posting may be left without an amount: if that is the
/// case, and the other postings all share the same commodity, the
/// missing amount is computed and filled in so that the transaction
/// balances. If more than one posting has no amount, or the known
/// amounts' balancing weights use several commodities while one is missing,
/// an error is returned. If every posting already has an amount, the posting
/// weights must sum to zero for each commodity. A transaction cost (`@` or
/// `@@`) changes only the balancing weight, never the amount stored on a
/// posting; market prices are not consulted.
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

    let mut sums: BTreeMap<CommodityName, ExactDecimalSum> = BTreeMap::new();
    for posting in &transaction.postings {
        if posting.amount.is_some() {
            let (weight, commodity) = posting_weight(posting).ok_or_else(|| {
                ValidationError::TransactionCostInconsistent {
                    account: posting.account.clone(),
                    date: transaction.date,
                }
            })?;
            sums.entry(commodity).or_default().add(weight);
        }
    }

    let sums: BTreeMap<CommodityName, Decimal> = sums
        .into_iter()
        .map(|(commodity, sum)| {
            sum.into_decimal()
                .map(|sum| (commodity.clone(), sum))
                .ok_or(ValidationError::TransactionWeightUnrepresentable {
                    commodity,
                    date: transaction.date,
                })
        })
        .collect::<Result<_, _>>()?;

    if let Some(index) = missing.first().copied() {
        if sums.len() != 1 {
            let commodities: Vec<String> = sums.keys().map(ToString::to_string).collect();
            return Err(ValidationError::AmbiguousAutoBalanceCommodity {
                commodities,
                date: transaction.date,
            });
        }

        let Some((commodity, sum)) = sums.into_iter().next() else {
            unreachable!("a missing amount requires exactly one commodity sum");
        };
        let number = Decimal::ZERO.checked_sub(sum).ok_or_else(|| {
            ValidationError::TransactionWeightUnrepresentable {
                commodity: commodity.clone(),
                date: transaction.date,
            }
        })?;
        transaction.postings[index].amount = Some(Amount { number, commodity });
        return Ok(());
    }

    let imbalances: Vec<(CommodityName, Decimal)> =
        sums.into_iter().filter(|(_, sum)| !sum.is_zero()).collect();
    match imbalances.as_slice() {
        [] => Ok(()),
        [(commodity, sum)] => Err(ValidationError::TransactionNotBalanced {
            commodity: commodity.to_string(),
            sum: *sum,
            date: transaction.date,
        }),
        _ => Err(ValidationError::MultiCommodityTransactionNotBalanced {
            imbalances,
            date: transaction.date,
        }),
    }
}

/// Returns the signed balancing weight and its commodity for one valued posting.
/// `None` means the amount is absent or the converted value is not exactly
/// representable. In valid transactions the latter is already rejected by
/// `validate_transaction` as `TransactionCostInconsistent`.
fn posting_weight(posting: &crate::Posting) -> Option<(Decimal, CommodityName)> {
    let amount = posting.amount.as_ref()?;
    let Some(cost) = &posting.transaction_cost else {
        return Some((amount.number, amount.commodity.clone()));
    };

    let weight = match cost.annotation {
        TransactionCostAnnotation::Unit => {
            checked_exact_decimal_product(amount.number, cost.amount.number)?
        }
        TransactionCostAnnotation::Total if amount.number.is_sign_negative() => {
            Decimal::ZERO.checked_sub(cost.amount.number)?
        }
        TransactionCostAnnotation::Total => cost.amount.number,
    };
    Some((weight, cost.amount.commodity.clone()))
}

/// Multiplies two decimals only when the exact result is representable by `Decimal`.
pub(crate) fn checked_exact_decimal_product(left: Decimal, right: Decimal) -> Option<Decimal> {
    let product = left.checked_mul(right)?;
    if left.is_zero() || right.is_zero() {
        return Some(product);
    }

    // Decimal's checked multiplication may round when it has to reduce scale.
    // The product of the coefficients must contain enough factors of both 2
    // and 5 for every discarded decimal place to be exact.
    let discarded_scale = left
        .scale()
        .saturating_add(right.scale())
        .saturating_sub(product.scale());
    if discarded_scale == 0 {
        return Some(product);
    }

    let left_mantissa = left.mantissa().unsigned_abs();
    let right_mantissa = right.mantissa().unsigned_abs();
    let factor_count = |mut value: u128, prime: u128| {
        let mut count = 0;
        while value != 0 && value.is_multiple_of(prime) {
            value /= prime;
            count += 1;
        }
        count
    };
    [2, 5]
        .into_iter()
        .all(|prime| {
            factor_count(left_mantissa, prime) + factor_count(right_mantissa, prime)
                >= discarded_scale
        })
        .then_some(product)
}

/// Exact signed decimal sum represented as an integer coefficient and scale.
/// The coefficient grows as needed, so intermediate posting order cannot
/// overflow or round a sum whose final result is representable.
#[derive(Default)]
struct ExactDecimalSum {
    magnitude: Vec<u32>,
    negative: bool,
    scale: u32,
}

pub(crate) fn checked_exact_decimal_sum(left: Decimal, right: Decimal) -> Option<Decimal> {
    let mut sum = ExactDecimalSum::default();
    sum.add(left);
    sum.add(right);
    sum.into_decimal()
}

impl ExactDecimalSum {
    fn add(&mut self, value: Decimal) {
        if value.is_zero() {
            return;
        }

        let target_scale = self.scale.max(value.scale());
        multiply_magnitude_by_power_of_ten(&mut self.magnitude, target_scale - self.scale);
        let mut value_magnitude = magnitude_from_u128(value.mantissa().unsigned_abs());
        multiply_magnitude_by_power_of_ten(&mut value_magnitude, target_scale - value.scale());
        self.scale = target_scale;

        if self.magnitude.is_empty() {
            self.magnitude = value_magnitude;
            self.negative = value.is_sign_negative();
        } else if self.negative == value.is_sign_negative() {
            add_magnitudes(&mut self.magnitude, &value_magnitude);
        } else {
            match compare_magnitudes(&self.magnitude, &value_magnitude) {
                std::cmp::Ordering::Greater => {
                    subtract_magnitude(&mut self.magnitude, &value_magnitude)
                }
                std::cmp::Ordering::Less => {
                    subtract_magnitude(&mut value_magnitude, &self.magnitude);
                    self.magnitude = value_magnitude;
                    self.negative = value.is_sign_negative();
                }
                std::cmp::Ordering::Equal => {
                    self.magnitude.clear();
                    self.negative = false;
                }
            }
        }
    }

    fn into_decimal(mut self) -> Option<Decimal> {
        while self.magnitude.len() > 3 && self.scale > 0 {
            if divide_magnitude(&mut self.magnitude, 10) != 0 {
                return None;
            }
            self.scale -= 1;
        }
        if self.magnitude.len() > 3 {
            return None;
        }

        let mut parts = [0; 3];
        parts[..self.magnitude.len()].copy_from_slice(&self.magnitude);
        Some(Decimal::from_parts(
            parts[0],
            parts[1],
            parts[2],
            self.negative,
            self.scale,
        ))
    }
}

fn magnitude_from_u128(mut value: u128) -> Vec<u32> {
    let mut magnitude = Vec::new();
    while value != 0 {
        magnitude.push(value as u32);
        value >>= 32;
    }
    magnitude
}

fn multiply_magnitude_by_power_of_ten(magnitude: &mut Vec<u32>, power: u32) {
    for _ in 0..power {
        let mut carry = 0u64;
        for limb in magnitude.iter_mut() {
            let product = u64::from(*limb) * 10 + carry;
            *limb = product as u32;
            carry = product >> 32;
        }
        if carry != 0 {
            magnitude.push(carry as u32);
        }
    }
}

fn add_magnitudes(left: &mut Vec<u32>, right: &[u32]) {
    left.resize(left.len().max(right.len()), 0);
    let mut carry = 0u64;
    for (index, left_limb) in left.iter_mut().enumerate() {
        let sum = u64::from(*left_limb) + u64::from(*right.get(index).unwrap_or(&0)) + carry;
        *left_limb = sum as u32;
        carry = sum >> 32;
    }
    if carry != 0 {
        left.push(carry as u32);
    }
}

fn compare_magnitudes(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    left.len()
        .cmp(&right.len())
        .then_with(|| left.iter().rev().cmp(right.iter().rev()))
}

/// Subtracts `right` from `left`; the caller guarantees `left >= right`.
fn subtract_magnitude(left: &mut Vec<u32>, right: &[u32]) {
    let mut borrow = 0i64;
    for (index, left_limb) in left.iter_mut().enumerate() {
        let difference =
            i64::from(*left_limb) - i64::from(*right.get(index).unwrap_or(&0)) - borrow;
        if difference < 0 {
            *left_limb = (difference + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            *left_limb = difference as u32;
            borrow = 0;
        }
    }
    while left.last() == Some(&0) {
        left.pop();
    }
}

/// Divides the magnitude by a small integer, returning the remainder.
fn divide_magnitude(magnitude: &mut Vec<u32>, divisor: u32) -> u32 {
    let mut remainder = 0u64;
    for limb in magnitude.iter_mut().rev() {
        let value = (remainder << 32) | u64::from(*limb);
        *limb = (value / u64::from(divisor)) as u32;
        remainder = value % u64::from(divisor);
    }
    while magnitude.last() == Some(&0) {
        magnitude.pop();
    }
    remainder as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Balance, Close, Commodity, Metadata, Open, Payee, Posting, Price, TransactionCost,
        TransactionCostAnnotation, TransactionStatus,
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
            transaction_cost: None,
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

    fn price(d: chrono::NaiveDate, base: &str, number: &str, quote: &str) -> Entry {
        Entry::Price(Price {
            date: d,
            base: CommodityName::new(base).unwrap(),
            quote: Amount {
                number: number.parse().unwrap(),
                commodity: CommodityName::new(quote).unwrap(),
            },
            meta: Metadata::new(),
        })
    }

    #[test]
    fn price_may_precede_the_first_posting_but_not_commodity_declarations() {
        let report = validate_journal_all(Journal {
            entries: vec![
                price(date(2024, 1, 2), "USD", "0.92", "EUR"),
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
            ],
        });

        assert!(report.issues.is_empty(), "{:?}", report.issues);
    }

    #[test]
    fn price_before_a_commodity_declaration_is_rejected() {
        let report = validate_journal_all(Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                price(date(2024, 1, 2), "USD", "0.92", "EUR"),
                commodity(date(2024, 1, 3), "USD"),
            ],
        });

        assert_eq!(report.issues.len(), 1);
        assert!(matches!(
            report.issues[0].error,
            ValidationError::CommodityNotDeclared { ref commodity, .. }
                if commodity.as_str() == "USD"
        ));
    }

    #[test]
    fn non_positive_market_prices_are_rejected() {
        let report = validate_journal_all(Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
                price(date(2024, 1, 2), "USD", "0", "EUR"),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::PriceNotPositive { .. },
                ..
            }]
        ));
    }

    #[test]
    fn conflicting_prices_on_the_same_day_are_rejected_but_identical_quotes_are_allowed() {
        let declarations = vec![
            commodity(date(2024, 1, 1), "EUR"),
            commodity(date(2024, 1, 1), "USD"),
        ];
        let conflicting = validate_journal_all(Journal {
            entries: [
                declarations.clone(),
                vec![
                    price(date(2024, 1, 2), "USD", "0.92", "EUR"),
                    price(date(2024, 1, 2), "USD", "0.93", "EUR"),
                ],
            ]
            .concat(),
        });
        assert!(matches!(
            conflicting.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::PriceConflict { .. },
                ..
            }]
        ));

        let identical = validate_journal_all(Journal {
            entries: [
                declarations,
                vec![
                    price(date(2024, 1, 2), "USD", "0.92", "EUR"),
                    price(date(2024, 1, 2), "USD", "0.920", "EUR"),
                    price(date(2024, 1, 2), "EUR", "1.08", "USD"),
                ],
            ]
            .concat(),
        });
        assert!(identical.issues.is_empty(), "{:?}", identical.issues);
    }

    #[test]
    fn price_directives_count_as_commodity_uses() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
                price(date(2024, 1, 2), "USD", "0.92", "EUR"),
            ],
        };

        assert!(unused_declarations(&journal).is_empty());
    }

    #[test]
    fn transaction_cost_quote_commodity_must_be_declared_and_counts_as_used() {
        let mut converted_posting = posting("Assets:USD", Some(("-100", "USD")));
        converted_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.93".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let exchange = Entry::Transaction(transaction(
            date(2024, 1, 3),
            vec![
                converted_posting,
                posting("Assets:Counter", Some(("93", "EUR"))),
            ],
        ));

        let undeclared = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:Counter"),
                commodity(date(2024, 1, 1), "USD"),
                exchange.clone(),
            ],
        });
        assert!(undeclared.issues.iter().any(|issue| matches!(
            &issue.error,
            ValidationError::CommodityNotDeclared { commodity, .. }
                if commodity.as_str() == "EUR"
        )));

        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:Counter"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                exchange,
            ],
        };
        assert!(validate_journal_all(journal.clone()).issues.is_empty());
        assert!(unused_declarations(&journal).is_empty());
    }

    #[test]
    fn total_transaction_cost_does_not_require_an_exact_unit_price() {
        let mut shares_posting = posting("Assets:Shares", Some(("-3", "AAPL")));
        shares_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "100".parse().unwrap(),
                commodity: CommodityName::new("USD").unwrap(),
            },
            annotation: TransactionCostAnnotation::Total,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:Shares"),
                open(date(2024, 1, 1), "Assets:Cash"),
                commodity(date(2024, 1, 1), "AAPL"),
                commodity(date(2024, 1, 1), "USD"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![shares_posting, posting("Assets:Cash", Some(("100", "USD")))],
                )),
            ],
        });

        assert!(
            report.issues.is_empty(),
            "a total annotation is authoritative even when its unit equivalent repeats: {:?}",
            report.issues
        );
    }

    #[test]
    fn total_transaction_cost_uses_the_posting_sign_for_positive_amounts() {
        let mut shares_posting = posting("Assets:Shares", Some(("3", "AAPL")));
        shares_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "100".parse().unwrap(),
                commodity: CommodityName::new("USD").unwrap(),
            },
            annotation: TransactionCostAnnotation::Total,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:Shares"),
                open(date(2024, 1, 1), "Assets:Cash"),
                commodity(date(2024, 1, 1), "AAPL"),
                commodity(date(2024, 1, 1), "USD"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![
                        shares_posting,
                        posting("Assets:Cash", Some(("-100", "USD"))),
                    ],
                )),
            ],
        });

        assert!(report.issues.is_empty(), "{:?}", report.issues);
    }

    #[test]
    fn unit_transaction_cost_balances_in_quote_commodity_without_changing_amounts() {
        let mut usd_posting = posting("Assets:USD", Some(("-100", "USD")));
        usd_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.92".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:EUR"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![usd_posting, posting("Assets:EUR", Some(("92", "EUR")))],
                )),
            ],
        });

        assert!(report.issues.is_empty(), "{:?}", report.issues);
        let transaction = report
            .journal
            .entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Transaction(transaction) => Some(transaction),
                _ => None,
            })
            .expect("expected the transaction to remain in the journal");
        assert_eq!(
            transaction.postings[0].amount.as_ref().unwrap().number,
            "-100".parse().unwrap()
        );
        assert_eq!(
            transaction.postings[0]
                .amount
                .as_ref()
                .unwrap()
                .commodity
                .as_str(),
            "USD"
        );
    }

    #[test]
    fn market_prices_do_not_balance_transactions() {
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:EUR"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                price(date(2024, 1, 2), "USD", "0.92", "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![
                        posting("Assets:USD", Some(("-100", "USD"))),
                        posting("Assets:EUR", Some(("92", "EUR"))),
                    ],
                )),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::MultiCommodityTransactionNotBalanced { imbalances, .. },
                ..
            }] if imbalances == &vec![
                (CommodityName::new("EUR").unwrap(), "92".parse().unwrap()),
                (CommodityName::new("USD").unwrap(), "-100".parse().unwrap()),
            ]
        ));
    }

    #[test]
    fn converted_weight_imbalance_is_reported_in_the_converted_commodity() {
        let mut usd_posting = posting("Assets:USD", Some(("-100", "USD")));
        usd_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.92".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:EUR"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![usd_posting, posting("Assets:EUR", Some(("90", "EUR")))],
                )),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::TransactionNotBalanced { commodity, sum, .. },
                ..
            }] if commodity == "EUR" && *sum == "-2".parse().unwrap()
        ));
    }

    #[test]
    fn auto_balance_infers_amount_from_the_single_weight_commodity() {
        let mut usd_posting = posting("Assets:USD", Some(("-100", "USD")));
        usd_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.92".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:EUR"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![usd_posting, posting("Assets:EUR", None)],
                )),
            ],
        });

        assert!(report.issues.is_empty(), "{:?}", report.issues);
        let transaction = report
            .journal
            .entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Transaction(transaction) => Some(transaction),
                _ => None,
            })
            .expect("expected the transaction to remain in the journal");
        let inferred = transaction.postings[1].amount.as_ref().unwrap();
        assert_eq!(inferred.number, "92".parse().unwrap());
        assert_eq!(inferred.commodity.as_str(), "EUR");
    }

    #[test]
    fn transaction_weight_sum_overflow_is_reported() {
        let mut transaction = transaction(
            date(2024, 1, 3),
            vec![
                posting("Assets:One", Some(("1", "EUR"))),
                posting("Assets:Two", Some(("1", "EUR"))),
            ],
        );
        for posting in &mut transaction.postings {
            posting.amount.as_mut().unwrap().number = Decimal::MAX;
        }
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:One"),
                open(date(2024, 1, 1), "Assets:Two"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::TransactionWeightUnrepresentable { commodity, .. },
                ..
            }] if commodity.as_str() == "EUR"
        ));
    }

    #[test]
    fn same_undeclared_commodity_is_reported_once_when_used_as_amount_and_cost() {
        let mut converted_posting = posting("Assets:Checking", Some(("100", "EUR")));
        converted_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "1".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:Checking"),
                open(date(2024, 1, 1), "Assets:Counter"),
                commodity(date(2024, 1, 1), "USD"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![
                        converted_posting,
                        posting("Assets:Counter", Some(("-100", "USD"))),
                    ],
                )),
            ],
        });
        let undeclared_eur = report
            .issues
            .iter()
            .filter(|issue| {
                matches!(
                    &issue.error,
                    ValidationError::CommodityNotDeclared { commodity, .. }
                        if commodity.as_str() == "EUR"
                )
            })
            .count();

        assert_eq!(undeclared_eur, 1);
    }

    #[test]
    fn non_positive_transaction_cost_is_rejected() {
        let mut converted_posting = posting("Assets:USD", Some(("-100", "USD")));
        converted_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "-0.93".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:Counter"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![
                        converted_posting,
                        posting("Assets:Counter", Some(("100", "USD"))),
                    ],
                )),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::TransactionCostNotPositive { .. },
                ..
            }]
        ));
    }

    #[test]
    fn transaction_cost_without_an_explicit_posting_amount_is_rejected() {
        let mut converted_posting = posting("Assets:USD", Some(("-100", "USD")));
        converted_posting.amount = None;
        converted_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.93".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:USD"),
                open(date(2024, 1, 1), "Assets:Counter"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 3),
                    vec![
                        converted_posting,
                        posting("Assets:Counter", Some(("100", "USD"))),
                    ],
                )),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::TransactionCostInconsistent { .. },
                ..
            }]
        ));
    }

    #[test]
    fn unit_transaction_cost_that_would_round_is_rejected() {
        let mut tiny_posting = posting(
            "Assets:Tiny",
            Some(("0.0000000000000000000000000001", "USD")),
        );
        tiny_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.0000000000000000000000000001".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });
        let report = validate_journal_all(Journal {
            entries: vec![
                open(date(2024, 1, 1), "Assets:Tiny"),
                commodity(date(2024, 1, 1), "USD"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(date(2024, 1, 3), vec![tiny_posting])),
            ],
        });

        assert!(matches!(
            report.issues.as_slice(),
            [ValidationIssue {
                error: ValidationError::TransactionCostInconsistent { .. },
                ..
            }]
        ));
    }

    #[test]
    fn exact_decimal_weight_arithmetic_handles_scale_and_order_exactly() {
        let left = Decimal::from_i128_with_scale(268_435_456, 28);
        let right = Decimal::from_i128_with_scale(37_252_902_984_619_140_625, 28);
        assert_eq!(
            checked_exact_decimal_product(left, right),
            Some(Decimal::new(1, 28))
        );
        assert_eq!(
            checked_exact_decimal_product(Decimal::new(1, 28), Decimal::new(1, 28)),
            None
        );
        for values in [
            [Decimal::MAX, Decimal::MAX, -Decimal::MAX, -Decimal::MAX],
            [-Decimal::MAX, Decimal::MAX, -Decimal::MAX, Decimal::MAX],
            [
                Decimal::MAX,
                Decimal::new(1, 1),
                -Decimal::MAX,
                Decimal::new(-1, 1),
            ],
        ] {
            let mut sum = ExactDecimalSum::default();
            for value in values {
                sum.add(value);
            }
            assert_eq!(sum.into_decimal(), Some(Decimal::ZERO));
        }

        let mut unrepresentable = ExactDecimalSum::default();
        unrepresentable.add(Decimal::MAX);
        unrepresentable.add(Decimal::new(1, 1));
        assert_eq!(unrepresentable.into_decimal(), None);
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
        assert_eq!(validated.entries[0].date(), Some(date(2024, 1, 1)));
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
    fn accepts_transaction_with_no_payee_without_checking_declarations() {
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

        assert!(validate_journal(journal).is_ok());
    }

    #[test]
    fn accepts_balance_assertion_matching_running_balance() {
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
                balance(date(2024, 1, 10), "Actifs:Compte", ("-100.00", "EUR")),
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
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("-50.00", "EUR")),
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
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
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
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
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
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 5), "Actifs:Compte", ("-100.00", "EUR")),
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
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
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
    fn allows_a_price_commodity_outside_the_posting_account_restriction() {
        let mut foreign_posting = posting("Actifs:Compte", Some(("-100.00", "USD")));
        foreign_posting.transaction_cost = Some(TransactionCost {
            amount: Amount {
                number: "0.90".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            },
            annotation: TransactionCostAnnotation::Unit,
        });

        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                commodity(date(2024, 1, 1), "USD"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        foreign_posting,
                        posting("Depenses:Divers", Some(("90.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert!(validate_journal(journal).is_ok());
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

    #[test]
    fn reports_nothing_when_every_declaration_is_used() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                payee("Test"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert!(unused_declarations(&journal).is_empty());
    }

    #[test]
    fn reports_unused_declarations_in_journal_order() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                open(date(2024, 1, 1), "Actifs:Compte"),
                payee("Jamais"),
            ],
        };

        let issues = unused_declarations(&journal);

        assert_eq!(
            issues,
            vec![
                ValidationIssue {
                    input_index: 0,
                    error: ValidationError::CommodityUnused {
                        commodity: CommodityName::new("EUR").unwrap(),
                        date: date(2024, 1, 1),
                    },
                },
                ValidationIssue {
                    input_index: 1,
                    error: ValidationError::AccountUnused {
                        account: AccountName::new("Actifs:Compte").unwrap(),
                        date: date(2024, 1, 1),
                    },
                },
                ValidationIssue {
                    input_index: 2,
                    error: ValidationError::PayeeUnused {
                        payee: "Jamais".to_string(),
                    },
                },
            ]
        );
    }

    #[test]
    fn a_balance_assertion_and_an_open_restriction_count_as_uses() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "USD"),
                // Restricting an account to USD references the commodity.
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
                open(date(2024, 1, 1), "Actifs:Vide"),
                balance(date(2024, 1, 2), "Actifs:Compte", ("0.00", "USD")),
            ],
        };

        let issues = unused_declarations(&journal);

        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].input_index, 2);
        assert!(matches!(
            issues[0].error,
            ValidationError::AccountUnused { .. }
        ));
    }

    #[test]
    fn closing_an_account_without_using_it_is_reported() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                close(date(2024, 6, 1), "Actifs:Compte"),
            ],
        };

        let issues = unused_declarations(&journal);

        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].input_index, 0);
        assert!(matches!(
            issues[0].error,
            ValidationError::AccountUnused { .. }
        ));
    }

    #[test]
    fn a_use_inside_an_otherwise_invalid_transaction_still_counts() {
        // The transaction is unbalanced (and its payee undeclared): it is
        // reported by `validate_journal_all`, but it does reference every
        // declaration below, so none of them is unused.
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                payee("Test"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("50.00", "EUR"))),
                    ],
                )),
            ],
        };

        assert!(unused_declarations(&journal).is_empty());
        assert!(
            validate_journal_all(journal)
                .issues
                .iter()
                .any(|issue| matches!(issue.error, ValidationError::TransactionNotBalanced { .. }))
        );
    }

    fn missing_account(account: &str) -> MissingData {
        let mut missing = MissingData::default();
        missing.accounts.insert(AccountName::new(account).unwrap());
        missing
    }

    fn missing_commodity(commodity: &str) -> MissingData {
        let mut missing = MissingData::default();
        missing
            .commodities
            .insert(CommodityName::new(commodity).unwrap());
        missing
    }

    fn missing_payee(payee: &str) -> MissingData {
        let mut missing = MissingData::default();
        missing.payees.insert(payee.to_string());
        missing
    }

    fn missing_used_account(account: &str) -> MissingData {
        let mut missing = MissingData::default();
        missing
            .used_accounts
            .insert(AccountName::new(account).unwrap());
        missing
    }

    fn errors_of(report: &ValidationReport) -> Vec<&ValidationError> {
        report.issues.iter().map(|issue| &issue.error).collect()
    }

    #[test]
    fn an_empty_missing_data_reproduces_the_complete_journal_behaviour() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Actifs:Compte"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("50.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("0.00", "EUR")),
            ],
        };

        let complete = validate_journal_all(journal.clone());
        let partial = validate_journal_all_with(journal, &MissingData::default());

        assert_eq!(complete, partial);
    }

    #[test]
    fn a_missing_declaration_suppresses_only_the_check_that_depends_on_it() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("50.00", "EUR"))),
                    ],
                )),
            ],
        };

        let report = validate_journal_all_with(journal.clone(), &missing_account("Actifs:Compte"));
        let errors = errors_of(&report);

        // The account may have been opened in the missing content: the
        // `account-not-open` error is not reported for it.
        assert!(
            !errors.iter().any(|error| matches!(
                error,
                ValidationError::AccountNotOpen { account, .. } if account.as_str() == "Actifs:Compte"
            )),
            "unexpected errors: {errors:?}"
        );
        // The payee and the other account are still checked.
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::PayeeNotDeclared { .. })),
            "unexpected errors: {errors:?}"
        );
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ValidationError::AccountNotOpen { account, .. } if account.as_str() == "Depenses:Divers"
            )),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_missing_commodity_declaration_suppresses_the_commodity_check() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        let report = validate_journal_all_with(journal.clone(), &missing_commodity("EUR"));
        let errors = errors_of(&report);

        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::CommodityNotDeclared { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_missing_payee_declaration_suppresses_the_payee_check() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
            ],
        };

        let report = validate_journal_all_with(journal.clone(), &missing_payee("Test"));
        let errors = errors_of(&report);

        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::PayeeNotDeclared { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_missing_open_suppresses_the_commodity_restriction_check() {
        let journal = Journal {
            entries: vec![
                open_restricted(date(2024, 1, 1), "Actifs:Compte", &["USD"]),
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

        let report = validate_journal_all_with(journal.clone(), &missing_account("Actifs:Compte"));
        let errors = errors_of(&report);

        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::CommodityNotAllowed { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_missing_posting_suppresses_the_balance_assertion_check() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("0.00", "EUR")),
            ],
        };

        let report =
            validate_journal_all_with(journal.clone(), &missing_used_account("Actifs:Compte"));
        let errors = errors_of(&report);

        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::BalanceAssertionFailed { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_partial_analysis_still_reports_the_independent_checks() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 1), "Actifs:Compte"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("50.00", "EUR"))),
                    ],
                )),
            ],
        };

        let report = validate_journal_all_with(
            journal,
            &MissingData {
                everything: true,
                ..MissingData::default()
            },
        );
        let errors = errors_of(&report);

        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::AccountAlreadyOpen { .. })),
            "unexpected errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::TransactionNotBalanced { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn a_possibly_open_account_is_still_validated_and_still_balances() {
        // The account is used but never opened: the transaction is invalid
        // and its postings are left out of the running balances, which makes
        // the balance assertion fail.
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Depenses:Divers"),
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction(
                    date(2024, 1, 5),
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("100.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("-100.00", "EUR")),
            ],
        };

        let complete_report = validate_journal_all(journal.clone());
        let complete = errors_of(&complete_report);
        assert!(
            complete
                .iter()
                .any(|error| matches!(error, ValidationError::AccountNotOpen { .. })),
            "unexpected errors: {complete:?}"
        );
        // The balance assertion is rejected before its amount is compared,
        // so it does not report a spurious mismatch either.
        assert!(
            !complete
                .iter()
                .any(|error| matches!(error, ValidationError::BalanceAssertionFailed { .. })),
            "unexpected errors: {complete:?}"
        );

        // The account may have been opened in the missing content: the
        // transaction is validated, and its postings count towards the
        // running balances, so the balance assertion holds.
        let report = validate_journal_all_with(journal, &missing_account("Actifs:Compte"));
        let partial = errors_of(&report);
        assert!(
            !partial
                .iter()
                .any(|error| matches!(error, ValidationError::AccountNotOpen { .. })),
            "unexpected errors: {partial:?}"
        );
        assert!(
            !partial
                .iter()
                .any(|error| matches!(error, ValidationError::BalanceAssertionFailed { .. })),
            "unexpected errors: {partial:?}"
        );
    }

    #[test]
    fn a_possibly_open_account_does_not_hide_a_duplicated_open() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                open(date(2024, 1, 2), "Actifs:Compte"),
            ],
        };

        let report = validate_journal_all_with(journal, &missing_account("Actifs:Compte"));
        let errors = errors_of(&report);

        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::AccountAlreadyOpen { .. })),
            "unexpected errors: {errors:?}"
        );
    }

    #[test]
    fn unused_declarations_ignore_the_names_the_missing_content_may_have_used() {
        let journal = Journal {
            entries: vec![
                open(date(2024, 1, 1), "Actifs:Compte"),
                commodity(date(2024, 1, 1), "EUR"),
                payee("Test"),
            ],
        };

        let complete = unused_declarations(&journal);
        assert_eq!(complete.len(), 3);

        let mut missing = MissingData::default();
        missing
            .used_accounts
            .insert(AccountName::new("Actifs:Compte").unwrap());
        missing
            .used_commodities
            .insert(CommodityName::new("EUR").unwrap());
        missing.used_payees.insert("Test".to_string());

        let partial = unused_declarations_with(&journal, &missing);
        assert!(partial.is_empty());
    }

    #[test]
    fn an_unbounded_loss_suppresses_every_completeness_dependent_check() {
        let journal = Journal {
            entries: vec![
                commodity(date(2024, 1, 1), "EUR"),
                Entry::Transaction(transaction_with_payee(
                    date(2024, 1, 5),
                    "Test",
                    vec![
                        posting("Actifs:Compte", Some(("-100.00", "EUR"))),
                        posting("Depenses:Divers", Some(("50.00", "EUR"))),
                    ],
                )),
                balance(date(2024, 1, 10), "Actifs:Compte", ("0.00", "EUR")),
            ],
        };

        let report = validate_journal_all_with(
            journal.clone(),
            &MissingData {
                everything: true,
                ..MissingData::default()
            },
        );
        let errors = errors_of(&report);

        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::AccountNotOpen { .. })),
            "unexpected errors: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::PayeeNotDeclared { .. })),
            "unexpected errors: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| matches!(error, ValidationError::BalanceAssertionFailed { .. })),
            "unexpected errors: {errors:?}"
        );
        // The transaction's own balancing does not depend on the missing
        // content: it is still reported.
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ValidationError::TransactionNotBalanced { .. })),
            "unexpected errors: {errors:?}"
        );
    }
}
