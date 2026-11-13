//! References a resolved configuration makes to journal declarations.
//!
//! Several keys of `pactole.toml` name an account, a payee, a commodity or an
//! account root that must exist in the journal they are used with: the bank
//! account of `[import]`, the counter account of an import rule, the
//! uncategorized expense accounts of `[reports]`, and so on. This module
//! collects those references so a front-end can cross-check them against an
//! analyzed journal without re-reading the raw TOML.
//!
//! `pactole-config` remains a leaf crate: references carry plain strings, the
//! TOML key path they come from, and the path of the configuration layer that
//! supplied the effective value. They never depend on a journal type.

use std::fmt;
use std::path::PathBuf;

use crate::Config;

/// The kind of journal declaration a configuration value refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigReferenceKind {
    /// An exact account name, which must be opened by the journal.
    Account,
    /// An account root (the first `:`-separated segment), which must be used
    /// by at least one opened account.
    AccountRoot,
    /// A payee name, which must be declared by the journal.
    Payee,
    /// A commodity name, which must be declared by the journal.
    Commodity,
}

/// One segment of a TOML key path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConfigPathSegment {
    /// A table key.
    Key(String),
    /// An array index.
    Index(usize),
}

/// The TOML key path a configuration reference comes from.
///
/// The path is kept structured so that a front-end can locate the exact value
/// in the source file (for a positioned diagnostic or an editor completion)
/// without parsing a display string. Its [`Display`] form is the stable,
/// human-readable spelling used in messages: `import.rules[0].counter_account`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ConfigKeyPath {
    /// Ordered path segments, from the document root.
    segments: Vec<ConfigPathSegment>,
}

impl ConfigKeyPath {
    /// Builds a key path from its ordered segments.
    pub fn new(segments: Vec<ConfigPathSegment>) -> Self {
        Self { segments }
    }

    /// Returns the ordered path segments.
    pub fn segments(&self) -> &[ConfigPathSegment] {
        &self.segments
    }

    /// Appends a table key.
    pub(crate) fn key(&mut self, key: impl Into<String>) {
        self.segments.push(ConfigPathSegment::Key(key.into()));
    }

    /// Appends an array index.
    pub(crate) fn index(&mut self, index: usize) {
        self.segments.push(ConfigPathSegment::Index(index));
    }
}

impl fmt::Display for ConfigKeyPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, segment) in self.segments.iter().enumerate() {
            match segment {
                ConfigPathSegment::Key(key) => {
                    // A key following any segment is separated from it by a
                    // dot; an index attaches directly to the preceding key.
                    if position > 0 {
                        formatter.write_str(".")?;
                    }
                    write_key(formatter, key)?;
                }
                ConfigPathSegment::Index(index) => write!(formatter, "[{index}]")?,
            }
        }
        Ok(())
    }
}

/// Writes one key, quoting it when it is not a bare TOML key.
fn write_key(formatter: &mut fmt::Formatter<'_>, key: &str) -> fmt::Result {
    let bare = !key.is_empty()
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
    if bare {
        return formatter.write_str(key);
    }

    formatter.write_str("\"")?;
    for character in key.chars() {
        match character {
            '\\' => formatter.write_str("\\\\")?,
            '"' => formatter.write_str("\\\"")?,
            character => formatter.write_str(&character.to_string())?,
        }
    }
    formatter.write_str("\"")
}

/// A journal declaration referenced by the resolved configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigReference {
    /// TOML key path the value comes from.
    pub key_path: ConfigKeyPath,
    /// Kind of journal declaration the value names.
    pub kind: ConfigReferenceKind,
    /// The referenced name, exactly as written in `pactole.toml`.
    pub name: String,
    /// Configuration layer that supplied the effective value.
    pub origin: PathBuf,
}

/// Provenance gathered while the configuration layers are merged.
///
/// Each entry records the highest-precedence layer that defined a key, so a
/// reference can be reported in the file the user actually has to edit.
#[derive(Debug, Default)]
pub(crate) struct Origins {
    /// Scalar import keys, by canonical key name.
    scalars: std::collections::BTreeMap<&'static str, PathBuf>,
    /// Per-entry provenance of the `[import.accounts]` table.
    accounts: std::collections::BTreeMap<String, PathBuf>,
    /// Layer that defined the `[[import.rules]]` block.
    rules: Option<PathBuf>,
    /// Layer of the last `[import]` section, used as a fallback.
    import: Option<PathBuf>,
    /// Report lists, by key name.
    reports: std::collections::BTreeMap<&'static str, PathBuf>,
    /// Layer that defined the effective `validation.unclassified_accounts`
    /// list.
    validation_unclassified_accounts: Option<PathBuf>,
}

impl Origins {
    /// Records the keys defined by one raw layer, overwriting lower layers.
    pub(crate) fn record(&mut self, raw: &crate::raw::RawFile, path: &std::path::Path) {
        if let Some(import) = &raw.import {
            let path = path.to_path_buf();
            self.import = Some(path.clone());

            for (key, value) in [
                ("import.bank_account", &import.bank_account),
                ("import.currency", &import.currency),
                (
                    "import.default_counter_account",
                    &import.default_counter_account,
                ),
            ] {
                if value.is_some() {
                    self.scalars.insert(key, path.clone());
                }
            }

            if let Some(accounts) = &import.accounts {
                for account in accounts.keys() {
                    self.accounts.insert(account.clone(), path.clone());
                }
            }

            if import.rules.is_some() {
                self.rules = Some(path);
            }
        }

        if raw
            .validation
            .as_ref()
            .and_then(|validation| validation.unclassified_accounts.as_ref())
            .is_some()
        {
            self.validation_unclassified_accounts = Some(path.to_path_buf());
        }

        if let Some(reports) = &raw.reports {
            let path = path.to_path_buf();
            for (key, value) in [
                ("reports.assets", &reports.assets),
                ("reports.liabilities", &reports.liabilities),
                ("reports.equity", &reports.equity),
                ("reports.income", &reports.income),
                ("reports.expenses", &reports.expenses),
                ("reports.uncategorized", &reports.uncategorized),
            ] {
                if value.is_some() {
                    self.reports.insert(key, path.clone());
                }
            }
        }
    }

    /// Returns the layer that supplied a scalar import key.
    fn scalar(&self, key: &'static str) -> PathBuf {
        self.scalars
            .get(key)
            .cloned()
            .or_else(|| self.import.clone())
            .unwrap_or_default()
    }

    /// Returns the layer that supplied an `[import.accounts]` entry.
    fn account(&self, identifier: &str) -> PathBuf {
        self.accounts
            .get(identifier)
            .cloned()
            .or_else(|| self.import.clone())
            .unwrap_or_default()
    }

    /// Returns the layer that supplied `[[import.rules]]`.
    fn rules(&self) -> PathBuf {
        self.rules
            .clone()
            .or_else(|| self.import.clone())
            .unwrap_or_default()
    }

    /// Returns the layer that supplied a `[reports]` list.
    fn report(&self, key: &str) -> PathBuf {
        self.reports.get(key).cloned().unwrap_or_default()
    }
}

/// Collects every journal reference of a resolved configuration.
pub(crate) fn collect(config: &Config, origins: &Origins) -> Vec<ConfigReference> {
    let mut references = Vec::new();

    if let Some(import) = &config.import {
        push(
            &mut references,
            key(&["import", "bank_account"]),
            ConfigReferenceKind::Account,
            &import.bank_account,
            origins.scalar("import.bank_account"),
        );

        for (identifier, account) in &import.accounts {
            push(
                &mut references,
                key(&["import", "accounts", identifier]),
                ConfigReferenceKind::Account,
                account,
                origins.account(identifier),
            );
        }

        if let Some(currency) = &import.currency {
            push(
                &mut references,
                key(&["import", "currency"]),
                ConfigReferenceKind::Commodity,
                currency,
                origins.scalar("import.currency"),
            );
        }

        push(
            &mut references,
            key(&["import", "default_counter_account"]),
            ConfigReferenceKind::Account,
            &import.default_counter_account,
            origins.scalar("import.default_counter_account"),
        );

        for (position, rule) in import.rules.iter().enumerate() {
            if let Some(account) = &rule.counter_account {
                push(
                    &mut references,
                    rule_key(position, "counter_account"),
                    ConfigReferenceKind::Account,
                    account,
                    origins.rules(),
                );
            }
            if let Some(payee) = &rule.payee {
                push(
                    &mut references,
                    rule_key(position, "payee"),
                    ConfigReferenceKind::Payee,
                    payee,
                    origins.rules(),
                );
            }
        }
    }

    for (index, account) in config.validation.unclassified_accounts.iter().enumerate() {
        let mut key_path = key(&["validation", "unclassified_accounts"]);
        key_path.index(index);
        push(
            &mut references,
            key_path,
            ConfigReferenceKind::Account,
            account,
            origins
                .validation_unclassified_accounts
                .clone()
                .unwrap_or_default(),
        );
    }

    for (key_name, kind, values) in [
        (
            "assets",
            ConfigReferenceKind::AccountRoot,
            &config.reports.assets,
        ),
        (
            "liabilities",
            ConfigReferenceKind::AccountRoot,
            &config.reports.liabilities,
        ),
        (
            "equity",
            ConfigReferenceKind::AccountRoot,
            &config.reports.equity,
        ),
        (
            "income",
            ConfigReferenceKind::AccountRoot,
            &config.reports.income,
        ),
        (
            "expenses",
            ConfigReferenceKind::AccountRoot,
            &config.reports.expenses,
        ),
        (
            "uncategorized",
            ConfigReferenceKind::Account,
            &config.reports.uncategorized,
        ),
    ] {
        let Some(values) = values else {
            continue;
        };
        for (position, value) in values.iter().enumerate() {
            let mut path = key(&["reports", key_name]);
            path.index(position);
            push(
                &mut references,
                path,
                kind,
                value,
                origins.report(&format!("reports.{key_name}")),
            );
        }
    }

    references
}

/// Builds a key path from table keys.
fn key(keys: &[&str]) -> ConfigKeyPath {
    let mut path = ConfigKeyPath::default();
    for key in keys {
        path.key(*key);
    }
    path
}

/// Builds the key path of one `[[import.rules]]` entry.
fn rule_key(position: usize, field: &str) -> ConfigKeyPath {
    let mut path = key(&["import", "rules"]);
    path.index(position);
    path.key(field);
    path
}

/// Appends one reference.
fn push(
    references: &mut Vec<ConfigReference>,
    key_path: ConfigKeyPath,
    kind: ConfigReferenceKind,
    name: &str,
    origin: PathBuf,
) {
    references.push(ConfigReference {
        key_path,
        kind,
        name: name.to_string(),
        origin,
    });
}

#[cfg(test)]
mod tests {
    use super::{ConfigKeyPath, ConfigPathSegment};

    #[test]
    fn displays_table_keys_and_array_indices() {
        let path = ConfigKeyPath::new(vec![
            ConfigPathSegment::Key("import".to_string()),
            ConfigPathSegment::Key("rules".to_string()),
            ConfigPathSegment::Index(1),
            ConfigPathSegment::Key("counter_account".to_string()),
        ]);

        assert_eq!(path.to_string(), "import.rules[1].counter_account");
    }

    #[test]
    fn quotes_keys_that_are_not_bare_toml_keys() {
        let path = ConfigKeyPath::new(vec![
            ConfigPathSegment::Key("import".to_string()),
            ConfigPathSegment::Key("accounts".to_string()),
            ConfigPathSegment::Key("000 123".to_string()),
        ]);

        assert_eq!(path.to_string(), "import.accounts.\"000 123\"");
    }
}
