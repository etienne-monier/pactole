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
    /// Validation level for configuration references that name a missing
    /// journal declaration.
    pub(crate) config: Option<String>,
    /// Validation level for an unused account.
    #[serde(rename = "account-unused")]
    pub(crate) account_unused: Option<String>,
    /// Validation level for an unused commodity.
    #[serde(rename = "commodity-unused")]
    pub(crate) commodity_unused: Option<String>,
    /// Validation level for an unused payee declaration.
    #[serde(rename = "payee-unused")]
    pub(crate) payee_unused: Option<String>,
    /// Validation level for an inactive tag pop.
    #[serde(rename = "tag-not-active")]
    pub(crate) tag_not_active: Option<String>,
    /// Validation level for use of configured unclassified accounts.
    pub(crate) unclassified: Option<String>,
    /// Accounts whose use should be reported as unclassified.
    pub(crate) unclassified_accounts: Option<Vec<String>>,
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
    /// Default number of decimal places for amounts.
    pub(crate) default_decimal_places: Option<u32>,
    /// Currency-specific number of decimal places.
    #[serde(default)]
    pub(crate) currencies: BTreeMap<String, u32>,
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
    /// Primary OFX date fields, in priority order.
    pub(crate) date_field: Option<Vec<String>>,
    /// Effective OFX date fields, in priority order.
    pub(crate) effective_date_field: Option<Vec<String>>,
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
            // A group-level override in a higher-precedence layer must not be
            // shadowed by individual overrides inherited from a lower layer.
            // Individual keys from this same layer are applied immediately
            // afterwards and therefore still take precedence over the group.
            if validation.unused.is_some() {
                target.account_unused = None;
                target.commodity_unused = None;
                target.payee_unused = None;
            }
            if let Some(payee) = validation.payee {
                target.payee = Some(payee);
            }
            if let Some(unused) = validation.unused {
                target.unused = Some(unused);
            }
            if let Some(config) = validation.config {
                target.config = Some(config);
            }
            if let Some(account_unused) = validation.account_unused {
                target.account_unused = Some(account_unused);
            }
            if let Some(commodity_unused) = validation.commodity_unused {
                target.commodity_unused = Some(commodity_unused);
            }
            if let Some(payee_unused) = validation.payee_unused {
                target.payee_unused = Some(payee_unused);
            }
            if let Some(tag_not_active) = validation.tag_not_active {
                target.tag_not_active = Some(tag_not_active);
            }
            if let Some(unclassified) = validation.unclassified {
                target.unclassified = Some(unclassified);
            }
            if let Some(accounts) = validation.unclassified_accounts {
                target.unclassified_accounts = Some(accounts);
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

        if let Some(format) = upper.format {
            let target = self.format.get_or_insert_with(RawFormat::default);
            if let Some(posting_width) = format.posting_width {
                target.posting_width = Some(posting_width);
            }
            if let Some(default_decimal_places) = format.default_decimal_places {
                target.default_decimal_places = Some(default_decimal_places);
            }
            target.currencies.extend(format.currencies);
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
            if let Some(date_field) = import.date_field {
                target.date_field = Some(date_field);
            }
            if let Some(effective_date_field) = import.effective_date_field {
                target.effective_date_field = Some(effective_date_field);
            }
            if let Some(rules) = import.rules {
                target.rules = Some(rules);
            }
        }
    }
}
