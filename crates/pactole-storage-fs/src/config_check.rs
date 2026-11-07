//! Cross-checks the names referenced by `pactole.toml` against a journal.
//!
//! `pactole-config` extracts the account, payee and commodity references of a
//! resolved configuration (see [`pactole_config::Config::references`]). This
//! module compares them with the declarations of an analyzed
//! [`Journal`](pactole_core::Journal), so a typo in the configuration is
//! reported by `pactole check` and by the language server instead of being
//! discovered later, during an import or a report.
//!
//! The check is deliberately read-only and lives here rather than in
//! `pactole-config`: this crate already depends on both the configuration
//! schema and the domain model, so neither leaf crate has to grow a
//! dependency on the other.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pactole_config::{
    Config, ConfigKeyPath, ConfigReference, ConfigReferenceKind, ValidationLevel,
};
use pactole_core::{AccountName, CommodityName, DiagnosticCode, Entry, Journal, MissingData};

use crate::analysis::DiagnosticSeverity;

/// A configuration value that does not match the analyzed journal.
///
/// Unlike [`crate::Diagnostic`], this diagnostic points at a key of a
/// `pactole.toml` file rather than at a source span: the configuration is not
/// a Pactole document, so its position is expressed as a TOML key path. A
/// front-end that has the raw file (the language server) can resolve that path
/// to a precise editor range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    /// Configuration file holding the offending value.
    pub path: PathBuf,
    /// TOML key path of the offending value.
    pub key_path: ConfigKeyPath,
    /// Whether the diagnostic blocks the operation by default.
    pub severity: DiagnosticSeverity,
    /// Stable machine-readable code for this diagnostic.
    pub code: DiagnosticCode,
    /// Human-readable explanation of the problem.
    pub message: String,
}

impl ConfigDiagnostic {
    /// Returns whether this diagnostic is an error.
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Error)
    }

    /// Returns whether this diagnostic is a warning.
    pub const fn is_warning(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Warning)
    }
}

/// Compares every journal reference of `config` with the declarations of
/// `journal`, skipping the references whose outcome depends on content a
/// partial analysis could not recover.
///
/// The severity comes from `[validation] config`: `off` yields no diagnostic
/// at all, `warn` (the default) reports warnings, and `error` reports errors.
/// The returned diagnostics are ordered like `Config::references`.
pub fn check_config_references(
    config: &Config,
    journal: &Journal,
    missing: &MissingData,
) -> Vec<ConfigDiagnostic> {
    let severity = match config.validation.config {
        ValidationLevel::Off => return Vec::new(),
        ValidationLevel::Warning => DiagnosticSeverity::Warning,
        ValidationLevel::Error => DiagnosticSeverity::Error,
    };

    let declarations = Declarations::from_journal(journal);

    config
        .references()
        .iter()
        .filter_map(|reference| check_reference(reference, &declarations, missing, severity))
        .collect()
}

/// The declarations available in a journal, as seen by the configuration
/// check.
#[derive(Debug, Default)]
struct Declarations {
    /// Accounts opened by an `open` directive.
    accounts: BTreeSet<AccountName>,
    /// Roots (first `:`-separated segment) of the opened accounts.
    roots: BTreeSet<String>,
    /// Payees declared by a `payee` directive.
    payees: BTreeSet<String>,
    /// Commodities declared by a `commodity` directive.
    commodities: BTreeSet<CommodityName>,
}

impl Declarations {
    /// Collects the declarations of one journal.
    fn from_journal(journal: &Journal) -> Self {
        let mut declarations = Self::default();
        for entry in &journal.entries {
            match entry {
                Entry::Open(open) => {
                    declarations.accounts.insert(open.account.clone());
                    declarations
                        .roots
                        .insert(account_root(open.account.as_str()).to_string());
                }
                Entry::Payee(payee) => {
                    declarations.payees.insert(payee.name.clone());
                }
                Entry::Commodity(commodity) => {
                    declarations.commodities.insert(commodity.name.clone());
                }
                Entry::Close(_) | Entry::Price(_) | Entry::Transaction(_) | Entry::Balance(_) => {}
            }
        }
        declarations
    }
}

/// Checks one configuration reference, returning a diagnostic when it names
/// something the journal does not declare.
fn check_reference(
    reference: &ConfigReference,
    declarations: &Declarations,
    missing: &MissingData,
    severity: DiagnosticSeverity,
) -> Option<ConfigDiagnostic> {
    let key = &reference.key_path;
    let name = &reference.name;

    let diagnostic = |code, message| ConfigDiagnostic {
        path: reference.origin.clone(),
        key_path: key.clone(),
        severity,
        code,
        message,
    };

    match reference.kind {
        ConfigReferenceKind::Account => {
            let account = match AccountName::new(name.clone()) {
                Ok(account) => account,
                Err(error) => {
                    return Some(diagnostic(
                        DiagnosticCode::ConfigInvalidName,
                        format!("`{key}` is not a valid account name: {error}"),
                    ));
                }
            };
            if declarations.accounts.contains(&account)
                || missing.everything
                || missing.accounts.contains(&account)
            {
                return None;
            }
            Some(diagnostic(
                DiagnosticCode::ConfigAccountNotDeclared,
                format!(
                    "`{key}` references the account `{name}`, \
                     which no `open` directive declares"
                ),
            ))
        }
        ConfigReferenceKind::AccountRoot => {
            if AccountName::new(name.clone()).is_err() || name.contains(':') {
                return Some(diagnostic(
                    DiagnosticCode::ConfigInvalidName,
                    format!("`{key}` is not a valid account root: `{name}`"),
                ));
            }
            if declarations.roots.contains(name) || root_may_be_declared(missing, name) {
                return None;
            }
            Some(diagnostic(
                DiagnosticCode::ConfigAccountRootUnmatched,
                format!(
                    "`{key}` references the account root `{name}`, \
                     which no opened account uses"
                ),
            ))
        }
        ConfigReferenceKind::Payee => {
            if !is_valid_payee(name) {
                return Some(diagnostic(
                    DiagnosticCode::ConfigInvalidName,
                    format!(
                        "`{key}` is not a valid payee name: \
                         it must be non-empty and contain no double quote or newline"
                    ),
                ));
            }
            if declarations.payees.contains(name)
                || missing.everything
                || missing.payees.contains(name)
            {
                return None;
            }
            Some(diagnostic(
                DiagnosticCode::ConfigPayeeNotDeclared,
                format!(
                    "`{key}` references the payee `{name}`, \
                     which no `payee` directive declares"
                ),
            ))
        }
        ConfigReferenceKind::Commodity => {
            let commodity = match CommodityName::new(name.clone()) {
                Ok(commodity) => commodity,
                Err(error) => {
                    return Some(diagnostic(
                        DiagnosticCode::ConfigInvalidName,
                        format!("`{key}` is not a valid commodity name: {error}"),
                    ));
                }
            };
            if declarations.commodities.contains(&commodity)
                || missing.everything
                || missing.commodities.contains(&commodity)
            {
                return None;
            }
            Some(diagnostic(
                DiagnosticCode::ConfigCommodityNotDeclared,
                format!(
                    "`{key}` references the commodity `{name}`, \
                     which no `commodity` directive declares"
                ),
            ))
        }
    }
}

/// Returns the root (first `:`-separated segment) of an account name.
pub(crate) fn account_root(account: &str) -> &str {
    account.split(':').next().unwrap_or(account)
}

/// Whether a partial analysis may have opened an account under this root.
fn root_may_be_declared(missing: &MissingData, root: &str) -> bool {
    missing.everything
        || missing
            .accounts
            .iter()
            .any(|account| account_root(account.as_str()) == root)
}

/// Whether a configured payee can be written as a `payee` directive.
fn is_valid_payee(payee: &str) -> bool {
    !payee.is_empty() && !payee.contains('"') && !payee.contains(['\n', '\r'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_config::{Config, Overrides, Source};
    use pactole_core::{Commodity, Open, Payee};
    use std::path::Path;

    fn config(contents: &str) -> Config {
        Config::resolve(
            &[Source {
                path: Path::new("/project/pactole.toml"),
                contents,
            }],
            &Overrides::default(),
        )
        .unwrap()
    }

    fn journal() -> Journal {
        Journal {
            entries: vec![
                Entry::Open(Open {
                    date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    account: AccountName::new("Assets:Checking").unwrap(),
                    commodities: Vec::new(),
                    description: None,
                    meta: Default::default(),
                }),
                Entry::Commodity(Commodity {
                    date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    name: CommodityName::new("EUR").unwrap(),
                    meta: Default::default(),
                }),
                Entry::Payee(Payee {
                    name: "Employer".to_string(),
                    meta: Default::default(),
                }),
            ],
        }
    }

    #[test]
    fn a_valid_configuration_has_no_diagnostic() {
        let config = config(
            "[reports]\nassets = [\"Assets\"]\n\n\
             [import]\nbank_account = \"Assets:Checking\"\ncurrency = \"EUR\"\n\
             default_counter_account = \"Assets:Checking\"\n\n\
             [[import.rules]]\ncounter_account = \"Assets:Checking\"\npayee = \"Employer\"\n",
        );

        let diagnostics = check_config_references(&config, &journal(), &MissingData::default());

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn missing_account_payee_and_commodity_are_reported_with_their_key_path() {
        let config = config(
            "[reports]\nassets = [\"Actifs\"]\n\n\
             [import]\nbank_account = \"Assets:Savings\"\ncurrency = \"USD\"\n\
             default_counter_account = \"Expenses:Food\"\n\n\
             [[import.rules]]\npayee = \"Unknown\"\n",
        );

        let diagnostics = check_config_references(&config, &journal(), &MissingData::default());
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();

        assert_eq!(
            codes,
            vec![
                "config-account-not-declared",
                "config-commodity-not-declared",
                "config-account-not-declared",
                "config-payee-not-declared",
                "config-account-root-unmatched",
            ]
        );
        assert_eq!(diagnostics[0].key_path.to_string(), "import.bank_account");
        assert_eq!(diagnostics[3].key_path.to_string(), "import.rules[0].payee");
        assert_eq!(diagnostics[4].key_path.to_string(), "reports.assets[0]");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.path == Path::new("/project/pactole.toml"))
        );
    }

    #[test]
    fn the_configured_level_controls_the_severity() {
        let contents = "[validation]\nconfig = \"error\"\n\n\
             [import]\nbank_account = \"Assets:Missing\"\n\
             default_counter_account = \"Assets:Checking\"\n";

        let diagnostics =
            check_config_references(&config(contents), &journal(), &MissingData::default());

        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
        assert!(diagnostics[0].is_error());
    }

    #[test]
    fn an_off_level_disables_the_check() {
        let contents = "[validation]\nconfig = \"off\"\n\n\
             [import]\nbank_account = \"Assets:Missing\"\n\
             default_counter_account = \"Assets:Checking\"\n";

        let diagnostics =
            check_config_references(&config(contents), &journal(), &MissingData::default());

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_invalid_name_is_reported_instead_of_a_missing_declaration() {
        let contents = "[import]\nbank_account = \"not an account\"\n\
             default_counter_account = \"Assets:Checking\"\n";

        let diagnostics =
            check_config_references(&config(contents), &journal(), &MissingData::default());

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_str(), "config-invalid-name");
    }

    #[test]
    fn missing_content_suppresses_the_references_it_may_declare() {
        let contents = "[import]\nbank_account = \"Assets:Missing\"\n\
             default_counter_account = \"Assets:Checking\"\n";
        let mut missing = MissingData::default();
        missing
            .accounts
            .insert(AccountName::new("Assets:Missing").unwrap());

        let diagnostics = check_config_references(&config(contents), &journal(), &missing);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn an_unknown_amount_of_missing_content_suppresses_every_reference() {
        let contents = "[import]\nbank_account = \"Assets:Missing\"\n\
             default_counter_account = \"Assets:Checking\"\n";
        let missing = MissingData {
            everything: true,
            ..MissingData::default()
        };

        let diagnostics = check_config_references(&config(contents), &journal(), &missing);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn a_missing_account_under_a_configured_root_suppresses_the_root_check() {
        let contents = "[reports]\nassets = [\"Actifs\"]\n";
        let mut missing = MissingData::default();
        missing
            .accounts
            .insert(AccountName::new("Actifs:Bank").unwrap());

        let diagnostics = check_config_references(&config(contents), &journal(), &missing);

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
