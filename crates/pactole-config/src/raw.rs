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
    pub(crate) validation: Option<RawValidation>,
    pub(crate) declaration: Option<RawDeclaration>,
    pub(crate) format: Option<RawFormat>,
    pub(crate) reports: Option<RawReports>,
    pub(crate) import: Option<RawImport>,
}

/// The `[validation]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawValidation {
    pub(crate) payee: Option<String>,
    pub(crate) unused: Option<String>,
}

/// The `[declaration]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawDeclaration {
    pub(crate) payees: Option<String>,
    pub(crate) accounts: Option<String>,
    pub(crate) commodities: Option<String>,
}

/// The `[format]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFormat {
    pub(crate) posting_width: Option<usize>,
}

/// The `[reports]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawReports {
    pub(crate) assets: Option<Vec<String>>,
    pub(crate) liabilities: Option<Vec<String>>,
    pub(crate) equity: Option<Vec<String>>,
    pub(crate) income: Option<Vec<String>>,
    pub(crate) expenses: Option<Vec<String>>,
}

/// The `[import]` section, unmerged.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawImport {
    pub(crate) bank_account: Option<String>,
    pub(crate) accounts: Option<BTreeMap<String, String>>,
    pub(crate) currency: Option<String>,
    pub(crate) default_counter_account: Option<String>,
    /// Rules are replaced in block by the highest-precedence layer that
    /// defines them, never merged element-wise.
    pub(crate) rules: Option<Vec<RawRule>>,
}

/// A single `[[import.rules]]` entry, unmerged.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRule {
    #[serde(default)]
    pub(crate) payee_contains: Option<String>,
    #[serde(default)]
    pub(crate) memo_contains: Option<String>,
    #[serde(default)]
    pub(crate) trntype: Option<String>,
    #[serde(default)]
    pub(crate) direction: Option<RawDirection>,
    pub(crate) counter_account: String,
    #[serde(default)]
    pub(crate) payee: Option<String>,
}

/// The direction of a money movement, as spelled in `pactole.toml`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RawDirection {
    Debit,
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
            if let Some(rules) = import.rules {
                target.rules = Some(rules);
            }
        }
    }
}
