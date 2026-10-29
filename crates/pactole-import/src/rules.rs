use std::collections::{BTreeMap, HashSet};

use pactole_core::{
    AccountName, Amount, CommodityName, Metadata, MetadataKey, Posting, Transaction,
    TransactionStatus,
};

use crate::ImportWarning;
use crate::error::ImportError;
use crate::record::ImportRecord;
use pactole_config::{Direction, ImportConfig, Rule};
use regex::Regex;
use rust_decimal::Decimal;

/// Payee used when a transaction has neither a rule-provided payee nor a
/// source payee. A transaction's payee is mandatory in Pactole, so a
/// placeholder is required rather than leaving it empty.
const FALLBACK_PAYEE: &str = "Unknown";

/// Lowers each [`ImportRecord`] into a balanced [`Transaction`], resolving the
/// counter account (and optional canonical payee) through the configured
/// rules. Rules are evaluated in order; every matching rule is applied, and a
/// payee set by one rule is visible to subsequent rules.
///
/// Returns the transactions alongside any non-fatal warnings.
pub(crate) fn lower(
    records: Vec<ImportRecord>,
    config: &ImportConfig,
    no_imported_payee: bool,
    omit_negative_amount: bool,
) -> Result<(Vec<Transaction>, Vec<ImportWarning>), ImportError> {
    // ---------------------------------------------
    // Validate the configuration (account names ok)
    // ---------------------------------------------
    // Get bank account from config or return error
    let bank_account =
        AccountName::new(&config.bank_account).map_err(|source| ImportError::InvalidAccount {
            account: config.bank_account.clone(),
            source,
        })?;

    // Get default counter account from config or return error
    let default_counter_account =
        AccountName::new(&config.default_counter_account).map_err(|source| {
            ImportError::InvalidAccount {
                account: config.default_counter_account.clone(),
                source,
            }
        })?;

    // Resolve every rule's counter account up front so a typo fails the whole
    // import immediately, rather than midway through the record list.
    let mut rule_accounts = Vec::with_capacity(config.rules.len());
    for rule in &config.rules {
        let account = rule
            .counter_account
            .as_ref()
            .map(|name| {
                AccountName::new(name).map_err(|source| ImportError::InvalidAccount {
                    account: name.clone(),
                    source,
                })
            })
            .transpose()?;
        rule_accounts.push(account);
    }

    // Same up-front validation for the source-account -> Pactole-account
    // mapping: an invalid mapped account fails the whole import before any
    // transaction is lowered.
    let mut mapped_accounts = BTreeMap::new();
    for (source_account, account) in &config.accounts {
        let resolved = AccountName::new(account).map_err(|source| ImportError::InvalidAccount {
            account: account.clone(),
            source,
        })?;
        mapped_accounts.insert(source_account.clone(), resolved);
    }

    let fitid_key = MetadataKey::new("fitid")?;
    let payee_metadata_key = config
        .payee_metadata_key
        .as_deref()
        .map(|key| MetadataKey::new(key.to_string()))
        .transpose()?;

    let payee_regexes = config
        .rules
        .iter()
        .map(|rule| compile_regex(rule.payee_regex.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;
    let memo_regexes = config
        .rules
        .iter()
        .map(|rule| compile_regex(rule.memo_regex.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut transactions = Vec::with_capacity(records.len());
    let mut warnings = Vec::new();
    // Source account identifiers already reported as unmapped, so each one
    // is warned about at most once per import.
    let mut warned_accounts = HashSet::new();

    for record in records {
        // Resolve the bank-side account for this record: the mapping
        // configured for its source account, else the `bank_account`
        // fallback (with a single warning per distinct source account).
        let bank_account = match record.account_id.as_deref() {
            Some(account_id) => match mapped_accounts.get(account_id) {
                Some(account) => account.clone(),
                None => {
                    // With no `[import.accounts]` table at all, `bank_account`
                    // is the intended primary account: stay silent rather
                    // than warn on every import of a classic single-account
                    // configuration.
                    if !config.accounts.is_empty() && warned_accounts.insert(account_id.to_string())
                    {
                        warnings.push(ImportWarning::UnmappedAccount {
                            account_id: account_id.to_string(),
                        });
                    }
                    bank_account.clone()
                }
            },
            None => bank_account.clone(),
        };

        // Evaluate every rule in order. A rule's fixed payee replacement is
        // visible to the following rules, while all other match fields remain
        // properties of the imported source record.
        let mut current_payee = record.payee.clone();
        let mut counter_account = None;
        for (index, rule) in config.rules.iter().enumerate() {
            if matches_rule(
                rule,
                &record,
                current_payee.as_deref(),
                payee_regexes[index].as_ref(),
                memo_regexes[index].as_ref(),
            ) {
                if let Some(Some(account)) = rule_accounts.get(index) {
                    counter_account = Some(account.clone());
                }
                if let Some(rule_payee) = &rule.payee {
                    current_payee = Some(rule_payee.clone());
                }
            }
        }

        let counter_account = counter_account.unwrap_or_else(|| default_counter_account.clone());

        // Rule payee replacements, or the raw payee, or the fallback value.
        let payee = current_payee;
        let payee = match payee {
            Some(payee) => payee,
            None => {
                warnings.push(ImportWarning::MissingPayee {
                    fitid: record.fitid.clone(),
                });
                FALLBACK_PAYEE.to_string()
            }
        };

        // Evaluate commodity
        // Transaction commodity, or default from config, or error
        let commodity = record
            .currency
            .as_deref()
            .or(config.currency.as_deref())
            .ok_or(ImportError::MissingCurrency)?;
        let commodity =
            CommodityName::new(commodity).map_err(|source| ImportError::InvalidCommodity {
                commodity: commodity.to_string(),
                source,
            })?;

        // Evaluate transaction id if any
        let mut meta = Metadata::new();
        if let Some(fitid) = &record.fitid {
            meta.insert(fitid_key.clone(), fitid.clone());
        }
        if !no_imported_payee && let (Some(key), Some(payee)) = (&payee_metadata_key, &record.payee)
        {
            meta.insert(key.clone(), payee.clone());
        }

        let counter_posting = Posting {
            account: counter_account,
            amount: if omit_negative_amount && -record.amount < Decimal::ZERO {
                None
            } else {
                Some(Amount {
                    number: -record.amount,
                    commodity: commodity.clone(),
                })
            },
            transaction_cost: None,
            meta: Metadata::new(),
        };

        let bank_posting = Posting {
            account: bank_account.clone(),
            amount: if omit_negative_amount && record.amount < Decimal::ZERO {
                None
            } else {
                Some(Amount {
                    number: record.amount,
                    commodity,
                })
            },
            transaction_cost: None,
            meta: Metadata::new(),
        };

        let postings = if record.amount < Decimal::ZERO {
            vec![counter_posting, bank_posting]
        } else {
            vec![bank_posting, counter_posting]
        };

        transactions.push(Transaction {
            date: record.date,
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some(payee),
            narration: record.memo.clone(),
            postings,
            tags: Vec::new(),
            links: Vec::new(),
            reference: record.reference.clone(),
            meta,
        });
    }

    Ok((transactions, warnings))
}

/// Returns `true` when every criterion present in `rule` matches `record`.
///
/// A rule with no criteria matches every record.
fn matches_rule(
    rule: &Rule,
    record: &ImportRecord,
    current_payee: Option<&str>,
    payee_regex: Option<&Regex>,
    memo_regex: Option<&Regex>,
) -> bool {
    if let Some(needle) = &rule.payee_contains {
        let haystack = current_payee.unwrap_or("");
        if !contains_ignore_case(haystack, needle) {
            return false;
        }
    }

    if let Some(values) = &rule.payee_in
        && !values
            .iter()
            .any(|value| equals_ignore_case(current_payee.unwrap_or(""), value))
    {
        return false;
    }

    if let Some(regex) = payee_regex
        && !regex.is_match(current_payee.unwrap_or(""))
    {
        return false;
    }

    if let Some(needle) = &rule.memo_contains {
        let haystack = record.memo.as_deref().unwrap_or("");
        if !contains_ignore_case(haystack, needle) {
            return false;
        }
    }

    if let Some(values) = &rule.memo_in
        && !values
            .iter()
            .any(|value| equals_ignore_case(record.memo.as_deref().unwrap_or(""), value))
    {
        return false;
    }

    if let Some(regex) = memo_regex
        && !regex.is_match(record.memo.as_deref().unwrap_or(""))
    {
        return false;
    }

    if let Some(expected) = &rule.trntype {
        let actual = record.transaction_type.as_deref().unwrap_or("");
        if !actual.eq_ignore_ascii_case(expected) {
            return false;
        }
    }

    if let Some(values) = &rule.trntype_in
        && !values.iter().any(|value| {
            equals_ignore_case(record.transaction_type.as_deref().unwrap_or(""), value)
        })
    {
        return false;
    }

    if let Some(account) = &rule.account
        && !equals_ignore_case(record.account_id.as_deref().unwrap_or(""), account)
    {
        return false;
    }

    if let Some(expected) = rule.direction
        && direction_of(&record.amount) != expected
    {
        return false;
    }

    true
}

/// Classifies an amount as a debit or a credit from the imported account's
/// point of view. A zero amount is treated as a credit (neither money leaves
/// nor enters).
fn direction_of(amount: &rust_decimal::Decimal) -> Direction {
    if amount.is_sign_negative() && !amount.is_zero() {
        Direction::Debit
    } else {
        Direction::Credit
    }
}

/// Case-insensitive substring test, using Unicode-aware lowercasing so
/// accented merchant names match regardless of case.
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn equals_ignore_case(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn compile_regex(pattern: Option<&str>) -> Result<Option<Regex>, ImportError> {
    pattern
        .map(|pattern| {
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| ImportError::InvalidRuleRegex {
                    pattern: pattern.to_string(),
                    message: error.to_string(),
                })
        })
        .transpose()
}
