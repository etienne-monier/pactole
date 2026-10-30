//! Raw, unmerged TOML shapes of every `pactole.toml` section.
//!
//! Each field is optional so that any layer (global or project) can override
//! only the keys it defines. Unknown keys are rejected per section
//! (`deny_unknown_fields`) so that typos fail loudly instead of being
//! silently ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The whole of a `pactole.toml` file, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFile {
    /// Optional validation settings from this layer.
    pub(crate) validation: Option<RawValidation>,
    /// Optional declaration-file settings from this layer.
    pub(crate) declaration: Option<RawDeclaration>,
    /// Optional formatting settings from this layer.
    pub(crate) format: Option<RawFormat>,
    /// Optional reporting settings from this layer.
    pub(crate) reports: Option<RawReports>,
    /// Optional import settings from this layer.
    pub(crate) import: Option<RawImport>,
}

/// The `[validation]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawValidation {
    /// Validation level for undeclared payees.
    pub(crate) payee: Option<String>,
    /// Validation level for unused declarations.
    pub(crate) unused: Option<String>,
}

/// The `[declaration]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDeclaration {
    /// File receiving inserted payee declarations.
    pub(crate) payees: Option<String>,
    /// File receiving inserted account declarations.
    pub(crate) accounts: Option<String>,
    /// File receiving inserted commodity declarations.
    pub(crate) commodities: Option<String>,
}

/// The `[format]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFormat {
    /// Column at which posting amounts are aligned.
    pub(crate) posting_width: Option<usize>,
}

/// The `[reports]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawReports {
    /// Account roots classified as assets.
    pub(crate) assets: Option<Vec<String>>,
    /// Account roots classified as liabilities.
    pub(crate) liabilities: Option<Vec<String>>,
    /// Account roots classified as equity.
    pub(crate) equity: Option<Vec<String>>,
    /// Account roots classified as income.
    pub(crate) income: Option<Vec<String>>,
    /// Account roots classified as expenses.
    pub(crate) expenses: Option<Vec<String>>,
    /// Accounts shown in the uncategorized expense section.
    pub(crate) uncategorized: Option<Vec<String>>,
}

/// The `[import]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawImport {
    /// Default bank-side account for imported transactions.
    pub(crate) bank_account: Option<String>,
    /// Source-account to Pactole-account mappings.
    pub(crate) accounts: Option<BTreeMap<String, String>>,
    /// Fallback currency for records without a source currency.
    pub(crate) currency: Option<String>,
    /// Counter-account used when no rule matches.
    pub(crate) default_counter_account: Option<String>,
    #[serde(default)]
    /// Metadata key used to retain the source payee.
    pub(crate) payee_metadata_key: Option<String>,
    #[serde(default)]
    /// Whether imported payee metadata is disabled.
    pub(crate) no_imported_payee: Option<bool>,
    #[serde(default)]
    /// Whether negative posting amounts are omitted from output.
    pub(crate) omit_negative_amount: Option<bool>,
    /// Rules are replaced in block by the highest-precedence layer that
    /// defines them, never merged element-wise.
    pub(crate) rules: Option<Vec<RawRule>>,
}

/// A single `[[import.rules]]` entry, unmerged.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRule {
    #[serde(default)]
    /// Case-insensitive substring required in the imported payee.
    pub(crate) payee_contains: Option<String>,
    #[serde(default)]
    /// Allowed imported payee values.
    pub(crate) payee_in: Option<Vec<String>>,
    #[serde(default)]
    /// Regular expression matched against the imported payee.
    pub(crate) payee_regex: Option<String>,
    #[serde(default)]
    /// Case-insensitive substring required in the imported memo.
    pub(crate) memo_contains: Option<String>,
    #[serde(default)]
    /// Allowed imported memo values.
    pub(crate) memo_in: Option<Vec<String>>,
    #[serde(default)]
    /// Regular expression matched against the imported memo.
    pub(crate) memo_regex: Option<String>,
    #[serde(default)]
    /// Exact imported transaction type to match.
    pub(crate) trntype: Option<String>,
    #[serde(default)]
    /// Allowed imported transaction types.
    pub(crate) trntype_in: Option<Vec<String>>,
    #[serde(default)]
    /// Counter account selected by this rule.
    pub(crate) account: Option<String>,
    #[serde(default)]
    /// Optional debit/credit direction restriction.
    pub(crate) direction: Option<RawDirection>,
    #[serde(default)]
    /// Explicit counter-account override.
    pub(crate) counter_account: Option<String>,
    #[serde(default)]
    /// Payee override emitted by this rule.
    pub(crate) payee: Option<String>,
}

/// The direction of a money movement, as spelled in `pactole.toml`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RawDirection {
    /// Outgoing movement from the source account.
    Debit,
    /// Incoming movement to the source account.
    Credit,
}

impl RawFile {
    /// Merges `upper` over `self`: every key defined in `upper` wins, keys
    /// absent from `upper` keep their current value. Tables merge per key;
    /// `[[import.rules]]` is replaced in block.
    pub(crate) fn merge(&mut self, upper: RawFile) {
        if let Some(validation) = upper.validation {
            let target = self.validation.get_or_insert_with(RawValidation::default);
            if let Some(payee) = validation.payee {
                target.payee = Some(payee);
            }
            if let Some(unused) = validation.unused {
                target.unused = Some(unused);
            }
        }

        if let Some(declaration) = upper.declaration {
            let target = self.declaration.get_or_insert_with(RawDeclaration::default);
            if let Some(payees) = declaration.payees {
                target.payees = Some(payees);
            }
            if let Some(accounts) = declaration.accounts {
                target.accounts = Some(accounts);
            }
            if let Some(commodities) = declaration.commodities {
                target.commodities = Some(commodities);
            }
        }

        if let Some(format) = upper.format
            && let Some(posting_width) = format.posting_width
        {
            self.format
                .get_or_insert_with(RawFormat::default)
                .posting_width = Some(posting_width);
        }

        if let Some(reports) = upper.reports {
            let target = self.reports.get_or_insert_with(RawReports::default);
            if let Some(assets) = reports.assets {
                target.assets = Some(assets);
            }
            if let Some(liabilities) = reports.liabilities {
                target.liabilities = Some(liabilities);
            }
            if let Some(equity) = reports.equity {
                target.equity = Some(equity);
            }
            if let Some(income) = reports.income {
                target.income = Some(income);
            }
            if let Some(expenses) = reports.expenses {
                target.expenses = Some(expenses);
            }
            if let Some(uncategorized) = reports.uncategorized {
                target.uncategorized = Some(uncategorized);
            }
        }

        if let Some(import) = upper.import {
            let target = self.import.get_or_insert_with(RawImport::default);
            if let Some(bank_account) = import.bank_account {
                target.bank_account = Some(bank_account);
            }
            if let Some(accounts) = import.accounts {
                target
                    .accounts
                    .get_or_insert_with(Default::default)
                    .extend(accounts);
            }
            if let Some(currency) = import.currency {
                target.currency = Some(currency);
            }
            if let Some(counter) = import.default_counter_account {
                target.default_counter_account = Some(counter);
            }
            if let Some(key) = import.payee_metadata_key {
                target.payee_metadata_key = Some(key);
            }
            if let Some(no_imported_payee) = import.no_imported_payee {
                target.no_imported_payee = Some(no_imported_payee);
            }
            if let Some(omit_negative_amount) = import.omit_negative_amount {
                target.omit_negative_amount = Some(omit_negative_amount);
            }
            if let Some(rules) = import.rules {
                target.rules = Some(rules);
            }
        }
    }
}
