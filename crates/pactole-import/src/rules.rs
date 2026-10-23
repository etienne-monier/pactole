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

/// Payee used when a transaction has neither a rule-provided payee nor a
/// source payee. A transaction's payee is mandatory in Pactole, so a
/// placeholder is required rather than leaving it empty.
const FALLBACK_PAYEE: &str = "Unknown";

/// Lowers each [`ImportRecord`] into a balanced [`Transaction`], resolving the
/// counter account (and optional canonical payee) through the configured
/// rules.
///
/// Returns the transactions alongside any non-fatal warnings.
pub(crate) fn lower(
    records: Vec<ImportRecord>,
    config: &ImportConfig,
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
        let account = AccountName::new(&rule.counter_account).map_err(|source| {
            ImportError::InvalidAccount {
                account: rule.counter_account.clone(),
                source,
            }
        })?;
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

        // Locate a matching rule
        let rule_index = config.rules.iter().enumerate().position(|(index, rule)| {
            matches_rule(
                rule,
                &record,
                payee_regexes[index].as_ref(),
                memo_regexes[index].as_ref(),
            )
        });

        // Evaluate the counter account
        // Rule counter account, or default from config
        let counter_account = rule_index
            .and_then(|index| rule_accounts.get(index).cloned())
            .unwrap_or_else(|| default_counter_account.clone());

        // Evaluate the payee
        // Rule payee, or raw payee, or fallback value (with warning)
        let rule_payee = rule_index
            .and_then(|index| config.rules.get(index))
            .and_then(|rule| rule.payee.clone());

        let payee = rule_payee.or_else(|| record.payee.clone());
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
        if let (Some(key), Some(payee)) = (&payee_metadata_key, &record.payee) {
            meta.insert(key.clone(), payee.clone());
        }

        let counter_posting = Posting {
            account: counter_account,
            amount: Some(Amount {
                number: -record.amount,
                commodity: commodity.clone(),
            }),
            meta: Metadata::new(),
        };

        let bank_posting = Posting {
            account: bank_account.clone(),
            amount: Some(Amount {
                number: record.amount,
                commodity,
            }),
            meta: Metadata::new(),
        };

        transactions.push(Transaction {
            date: record.date,
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some(payee),
            narration: record.memo.clone(),
            postings: vec![counter_posting, bank_posting],
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
    payee_regex: Option<&Regex>,
    memo_regex: Option<&Regex>,
) -> bool {
    if let Some(needle) = &rule.payee_contains {
        let haystack = record.payee.as_deref().unwrap_or("");
        if !contains_ignore_case(haystack, needle) {
            return false;
        }
    }

    if let Some(values) = &rule.payee_in
        && !values
            .iter()
            .any(|value| equals_ignore_case(record.payee.as_deref().unwrap_or(""), value))
    {
        return false;
    }

    if let Some(regex) = payee_regex
        && !regex.is_match(record.payee.as_deref().unwrap_or(""))
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
