#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Import external financial formats into Pactole domain models.
//!
//! This crate is the boundary adapter between a foreign input format (OFX,
//! and later CSV, QIF, ...) and the pure Pactole domain
//! ([`pactole_core`]). It performs no I/O: callers read the raw source bytes
//! themselves and hand them to `ofx-kit`, which performs encoding detection and
//! parsing while keeping the crate usable by any front-end.
//!
//! The pipeline is deliberately split into two steps so that future importers
//! can reuse the rule engine:
//!
//! 1. a format-specific parser turns the source bytes into format-neutral
//!    [`ImportRecord`]s (an internal `ofx` module does this for OFX);
//! 2. the shared rules engine (internal `rules` module) turns
//!    [`ImportRecord`]s into [`pactole_core::Transaction`]s, resolving the
//!    counter account (and optional canonical payee) through the configured
//!    [`ImportConfig`].
//!
//! Transactions are marked [`pactole_core::TransactionStatus::Cleared`] and
//! carry their source identifier as `fitid` metadata on the bank-side posting,
//! so a re-import can skip source records already present. For OFX, each
//! identifier is namespaced with the source account:
//! `ofx:<ACCTID>:<FITID>`.

/// Actual Budget database migration support.
mod actual;
/// Import error definitions.
mod error;
/// OFX document parsing.
mod ofx;
/// Format-neutral imported transaction records.
mod record;
/// Rule matching and transaction construction.
mod rules;

use std::collections::HashSet;

use chrono::NaiveDate;
use pactole_core::Transaction;

pub use crate::actual::{ActualMigration, migrate_actual_database, parse_actual_database};
pub use crate::error::ImportError;
pub use crate::record::{ImportRecord, StatementBalance};
pub use pactole_config::{Direction, ImportConfig, ImportDateField, Rule};

/// Builds the stored provenance identifier for an OFX transaction.
pub fn qualified_fitid(account_id: &str, fitid: &str) -> String {
    format!("ofx:{account_id}:{fitid}")
}

/// A non-fatal issue encountered while importing; the affected transaction is
/// still produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWarning {
    /// The source transaction carried no payee and no rule supplied one, so
    /// the placeholder payee was used.
    MissingPayee {
        /// The source transaction identifier, when known.
        fitid: Option<String>,
    },

    /// The transaction's source account (for OFX: the statement's `ACCTID`)
    /// is not covered by `ImportConfig::accounts`, so
    /// `ImportConfig::bank_account` was used as the fallback. Emitted at
    /// most once per distinct source account identifier, and only when a
    /// mapping table is configured: with no table at all, `bank_account` is
    /// the intended primary account rather than a fallback.
    UnmappedAccount {
        /// The source account identifier that matched no configured mapping.
        account_id: String,
    },
}

/// Why an imported source transaction was left out of the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The qualified source identifier was already present in the target.
    AlreadyImported,
    /// The transaction date was earlier than the requested `--from` date.
    BeforeFrom,
}

/// Details about a source transaction omitted during import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTransaction {
    /// Date reported by the source.
    pub date: NaiveDate,
    /// Signed amount reported by the source.
    pub amount: rust_decimal::Decimal,
    /// Source currency, if available.
    pub currency: Option<String>,
    /// Source payee or name, if available.
    pub payee: Option<String>,
    /// Qualified source identifier, when the source provided a FITID.
    pub fitid: Option<String>,
    /// Reason the transaction was omitted.
    pub reason: SkipReason,
}

/// The result of importing a source document.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The transactions that were imported, ordered from oldest to newest.
    pub transactions: Vec<Transaction>,

    /// Number of source transactions skipped because their identifier was
    /// already present in `known_fitids`.
    pub skipped: usize,

    /// Details for every source transaction omitted by date filtering or
    /// duplicate FITID detection.
    pub skipped_transactions: Vec<SkippedTransaction>,

    /// Non-fatal issues encountered while lowering the kept transactions.
    pub warnings: Vec<ImportWarning>,

    /// Closing balances reported by the source statement.
    pub balances: Vec<StatementBalance>,
}

/// Optional controls for an OFX import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// Keep transactions on or after this date.
    pub from: Option<NaiveDate>,
    /// Overrides the configuration value when present.
    pub no_imported_payee: Option<bool>,
    /// Overrides the configuration value when present.
    pub omit_negative_amount: Option<bool>,
}

/// Imports an OFX document (1.x SGML or 2.x XML) into Pactole transactions.
///
/// `known_fitids` is the set of qualified source transaction identifiers
/// already present in the target (e.g. gathered from an existing `.pactole`
/// file or from a database): any source transaction whose qualified `FITID` is
/// in this set is counted in [`ImportOutcome::skipped`] and left out of the
/// result, making re-imports idempotent. The values are produced by
/// [`qualified_fitid`].
///
/// # Errors
///
/// Returns [`ImportError`] when the document cannot be parsed, contains no
/// statement, or when the configuration refers to invalid account/commodity
/// names.
pub fn import_ofx(
    source: &[u8],
    config: &ImportConfig,
    known_fitids: &HashSet<String>,
) -> Result<ImportOutcome, ImportError> {
    import_ofx_with_options(source, config, known_fitids, ImportOptions::default())
}

/// Imports an OFX document with optional date and source-payee controls.
pub fn import_ofx_with_options(
    source: &[u8],
    config: &ImportConfig,
    known_fitids: &HashSet<String>,
    options: ImportOptions,
) -> Result<ImportOutcome, ImportError> {
    let (records, balances) = ofx::parse_records(source)?;

    let mut kept = Vec::with_capacity(records.len());
    let mut skipped = 0usize;
    let mut skipped_transactions = Vec::new();

    for record in records {
        let selected_date = rules::select_date(&record, &config.date_field).ok_or_else(|| {
            ImportError::MissingDate {
                fields: config
                    .date_field
                    .iter()
                    .map(|field| match field {
                        pactole_config::ImportDateField::Posted => "posted",
                        pactole_config::ImportDateField::User => "user",
                        pactole_config::ImportDateField::Available => "available",
                    })
                    .collect(),
            }
        })?;
        if options.from.is_some_and(|from| selected_date < from) {
            skipped_transactions.push(SkippedTransaction {
                date: selected_date,
                amount: record.amount,
                currency: record.currency.clone(),
                payee: record.payee.clone(),
                fitid: record.fitid.as_deref().map(|fitid| {
                    qualified_fitid(record.account_id.as_deref().unwrap_or("unknown"), fitid)
                }),
                reason: SkipReason::BeforeFrom,
            });
            continue;
        }
        if let Some(fitid) = record.fitid.as_deref()
            && known_fitids.contains(&qualified_fitid(
                record.account_id.as_deref().unwrap_or("unknown"),
                fitid,
            ))
        {
            skipped += 1;
            skipped_transactions.push(SkippedTransaction {
                date: selected_date,
                amount: record.amount,
                currency: record.currency.clone(),
                payee: record.payee.clone(),
                fitid: record.fitid.as_deref().map(|fitid| {
                    qualified_fitid(record.account_id.as_deref().unwrap_or("unknown"), fitid)
                }),
                reason: SkipReason::AlreadyImported,
            });
            continue;
        }
        kept.push(record);
    }

    kept.sort_by_key(|record| rules::select_date(record, &config.date_field));

    let no_imported_payee = options
        .no_imported_payee
        .unwrap_or(config.no_imported_payee);
    let omit_negative_amount = options
        .omit_negative_amount
        .unwrap_or(config.omit_negative_amount);
    let (transactions, warnings) =
        rules::lower(kept, config, no_imported_payee, omit_negative_amount)?;

    Ok(ImportOutcome {
        transactions,
        skipped,
        skipped_transactions,
        warnings,
        balances,
    })
}
