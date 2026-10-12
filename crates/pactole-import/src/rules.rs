use pactole_core::{
    AccountName, Amount, CommodityName, Metadata, MetadataKey, Posting, Transaction,
    TransactionStatus,
};

use crate::ImportWarning;
use crate::config::{Direction, ImportConfig, Rule};
use crate::error::ImportError;
use crate::record::ImportRecord;

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
    let bank_account =
        AccountName::new(&config.bank_account).map_err(|source| ImportError::InvalidAccount {
            account: config.bank_account.clone(),
            source,
        })?;

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

    let fitid_key = MetadataKey::new("fitid")?;

    let mut transactions = Vec::with_capacity(records.len());
    let mut warnings = Vec::new();

    for record in records {
        let rule_index = config
            .rules
            .iter()
            .position(|rule| matches_rule(rule, &record));

        let counter_account = rule_index
            .and_then(|index| rule_accounts.get(index).cloned())
            .unwrap_or_else(|| default_counter_account.clone());

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

        let mut meta = Metadata::new();
        if let Some(fitid) = &record.fitid {
            meta.insert(fitid_key.clone(), fitid.clone());
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
fn matches_rule(rule: &Rule, record: &ImportRecord) -> bool {
    if let Some(needle) = &rule.payee_contains {
        let haystack = record.payee.as_deref().unwrap_or("");
        if !contains_ignore_case(haystack, needle) {
            return false;
        }
    }

    if let Some(needle) = &rule.memo_contains {
        let haystack = record.memo.as_deref().unwrap_or("");
        if !contains_ignore_case(haystack, needle) {
            return false;
        }
    }

    if let Some(expected) = &rule.trntype {
        let actual = record.transaction_type.as_deref().unwrap_or("");
        if !actual.eq_ignore_ascii_case(expected) {
            return false;
        }
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
