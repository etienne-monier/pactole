//! Import external financial formats into Pactole domain models.
//!
//! This crate is the boundary adapter between a foreign input format (OFX,
//! and later CSV, QIF, ...) and the pure Pactole domain
//! ([`pactole_core`]). It performs no I/O: callers read the source bytes
//! themselves, pass them through [`decode_ofx_source`] when they may not be
//! UTF-8, and hand over the resulting string, which keeps the crate usable
//! by any front-end (the `pactole` CLI today, a future database-backed
//! storage and the language server tomorrow) and trivially unit-testable.
//!
//! The pipeline is deliberately split into two steps so that future importers
//! can reuse the rule engine:
//!
//! 1. a format-specific parser turns the source text into format-neutral
//!    [`ImportRecord`]s (an internal `ofx` module does this for OFX);
//! 2. the shared rules engine (internal `rules` module) turns
//!    [`ImportRecord`]s into [`pactole_core::Transaction`]s, resolving the
//!    counter account (and optional canonical payee) through the configured
//!    [`ImportConfig`].
//!
//! Transactions are marked [`pactole_core::TransactionStatus::Cleared`] and
//! carry their source identifier (the OFX `FITID`) as `fitid` metadata, so a
//! re-import can skip transactions already present.

mod encoding;
mod error;
mod ofx;
mod record;
mod rules;

use std::collections::HashSet;

use pactole_core::Transaction;

pub use crate::encoding::decode_ofx_source;
pub use crate::error::ImportError;
pub use crate::record::ImportRecord;
pub use pactole_config::{Direction, ImportConfig, Rule};

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

/// The result of importing a source document.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The transactions that were imported, in source order.
    pub transactions: Vec<Transaction>,

    /// Number of source transactions skipped because their identifier was
    /// already present in `known_fitids`.
    pub skipped: usize,

    /// Non-fatal issues encountered while lowering the kept transactions.
    pub warnings: Vec<ImportWarning>,
}

/// Imports an OFX document (1.x SGML or 2.x XML) into Pactole transactions.
///
/// `known_fitids` is the set of source transaction identifiers already present
/// in the target (e.g. gathered from an existing `.pactole` file or from a
/// database): any source transaction whose `FITID` is in this set is counted
/// in [`ImportOutcome::skipped`] and left out of the result, making re-imports
/// idempotent.
///
/// # Errors
///
/// Returns [`ImportError`] when the document cannot be parsed, contains no
/// statement, or when the configuration refers to invalid account/commodity
/// names.
pub fn import_ofx(
    source: &str,
    config: &ImportConfig,
    known_fitids: &HashSet<String>,
) -> Result<ImportOutcome, ImportError> {
    let records = ofx::parse_records(source)?;

    let mut kept = Vec::with_capacity(records.len());
    let mut skipped = 0usize;

    for record in records {
        if let Some(fitid) = record.fitid.as_deref()
            && known_fitids.contains(fitid)
        {
            skipped += 1;
            continue;
        }
        kept.push(record);
    }

    let (transactions, warnings) = rules::lower(kept, config)?;

    Ok(ImportOutcome {
        transactions,
        skipped,
        warnings,
    })
}
