//! Server configuration, read once from the LSP `initialize` request's
//! `initializationOptions` and from the layered `pactole.toml` files.
//!
//! Two settings are supported:
//!
//! - `journal_file`: the path (relative to the workspace root, or absolute)
//!   to the root `.pactole` file that should be analyzed with full `include`
//!   resolution via [`pactole_storage_fs::analyze_file_with_loader`]. When
//!   absent, the server falls back to analyzing each open document on its
//!   own via [`pactole_storage_fs::analyze_file`], without following its
//!   `include`s.
//! - `validation`: the severity of configurable business checks (the
//!   undeclared-payee check and the unused-declaration checks), as either a
//!   single string applied to every check (`"validation": "warn"`) or a table
//!   (`"validation": { "payee": "warn", "unused": "off" }`).
//!
//! A relative `journal_file` is resolved against the workspace root when one
//! is known (from `workspace_folders`/`rootUri`/`rootPath`, see
//! [`workspace_root_from_initialize_params`]). When no workspace root is
//! known at all (e.g. a client that opens single files without a workspace),
//! it is instead resolved against the server process's current working
//! directory, rather than silently left relative: a relative path left
//! as-is would fail to resolve against whatever directory `include`
//! resolution happens to run from, silently dropping every diagnostic for
//! the whole include graph without any indication why.
//!
//! Validation settings are resolved like the CLI's: the global
//! `pactole.toml` (see [`pactole_config::config_dir`]) is overridden by the
//! project `pactole.toml` (next to `journal_file`, or at the workspace root
//! when no `journal_file` is configured), which is overridden by
//! `initializationOptions`. A malformed or unreadable configuration file is
//! logged and ignored rather than failing `initialize`, so the editor stays
//! usable.

use std::path::{Path, PathBuf};

use pactole_config::{DeclarationOptions, Overrides, ValidationLevel, ValidationOptions};
use serde::Deserialize;

/// Raw shape of the `initializationOptions` JSON value this server
/// understands. Unknown fields are ignored so future additions do not break
/// deserialization of older clients' payloads.
#[derive(Debug, Default, Deserialize)]
struct RawInitializationOptions {
    /// Path to the root `.pactole` journal file, relative to the workspace
    /// root if not absolute.
    journal_file: Option<String>,

    /// Validation override, as a level string or a table of options.
    validation: Option<RawValidation>,
}

/// Raw shape of the `validation` initialization option.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawValidation {
    /// A single level applying to every configurable check.
    Level(String),
    /// Per-check levels.
    Table {
        #[serde(default)]
        payee: Option<String>,
        #[serde(default)]
        unused: Option<String>,
    },
}

impl RawValidation {
    /// The level of the undeclared-payee check: the single level for the
    /// string form, the `payee` entry for the table form.
    fn payee(&self) -> Option<&str> {
        match self {
            Self::Level(level) => Some(level),
            Self::Table { payee, .. } => payee.as_deref(),
        }
    }

    /// The level of the unused-declaration checks: the single level for the
    /// string form, the `unused` entry for the table form.
    fn unused(&self) -> Option<&str> {
        match self {
            Self::Level(level) => Some(level),
            Self::Table { unused, .. } => unused.as_deref(),
        }
    }
}

/// Resolved server configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// The root journal file to analyze with include resolution, already
    /// resolved to an absolute-ish path (joined against the workspace root
    /// when it was given as relative, or against the current working
    /// directory when no workspace root is known at all). `None` means
    /// "analyze each open document standalone".
    pub journal_file: Option<PathBuf>,

    /// Validation settings from the layered configuration files, overridden
    /// by `initializationOptions`.
    pub validation: ValidationOptions,

    /// Declaration files from the layered configuration files, with their
    /// paths resolved against the journal's directory (see
    /// [`Config::resolve_declaration_paths`]).
    pub declarations: DeclarationTargets,

    /// The `initializationOptions` payee override, kept so
    /// [`Config::load_layered`] can apply it on top of the file layers.
    payee_override: Option<ValidationLevel>,

    /// The `initializationOptions` unused-declaration override, kept so
    /// [`Config::load_layered`] can apply it on top of the file layers.
    unused_override: Option<ValidationLevel>,

    /// The raw `[declaration]` paths from the layered configuration files,
    /// kept unresolved so [`Config::load_layered`] can resolve them once the
    /// journal location is known.
    declaration_paths: DeclarationOptions,
}

/// Files receiving new declarations, resolved to absolute-ish paths.
///
/// A field is `None` when its `[declaration]` key is unset, or when no
/// `journal_file` is configured (there is no directory to resolve a relative
/// path against, and no project to validate against anyway).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeclarationTargets {
    /// File receiving new `payee "..."` declarations.
    pub payees: Option<PathBuf>,
    /// File receiving new `DATE open ACCOUNT` declarations.
    pub accounts: Option<PathBuf>,
    /// File receiving new `DATE commodity COMMODITY` declarations.
    pub commodities: Option<PathBuf>,
}

impl Config {
    /// Builds a [`Config`] with the given declaration targets, for tests and
    /// callers that resolve paths themselves.
    pub fn with_declarations(declarations: DeclarationTargets) -> Self {
        Self {
            declarations,
            ..Self::default()
        }
    }

    /// Builds a [`Config`] from the raw `initializationOptions` JSON value
    /// (as received in [`lsp_types::InitializeParams::initialization_options`])
    /// and the workspace root directory (derived from `rootUri`/`rootPath`,
    /// see [`workspace_root_from_initialize_params`]).
    ///
    /// A missing or unparsable `initializationOptions` value is treated the
    /// same as an empty configuration, rather than failing `initialize`.
    /// Invalid validation levels are logged and ignored.
    pub fn from_initialization_options(
        initialization_options: Option<&serde_json::Value>,
        workspace_root: Option<&Path>,
    ) -> Self {
        let raw: RawInitializationOptions = initialization_options
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();

        let journal_file = raw.journal_file.map(|raw_path| {
            let path = PathBuf::from(&raw_path);
            if path.is_absolute() {
                return path;
            }
            match workspace_root {
                Some(root) => root.join(&path),
                // No workspace root at all: resolve against the current
                // working directory rather than leaving the path relative
                // (which would silently fail to resolve later, dropping
                // every diagnostic for the whole include graph with no
                // indication why). If the current working directory itself
                // cannot be determined, the path is kept relative as a last
                // resort; `analyze_file_with_loader` will then simply fail
                // to load it, which the server already falls back from
                // gracefully (see `server::publish_for`).
                None => std::env::current_dir()
                    .map(|cwd| cwd.join(&path))
                    .unwrap_or(path),
            }
        });

        let payee_override = raw
            .validation
            .as_ref()
            .and_then(|validation| parse_level(validation.payee()));
        let unused_override = raw
            .validation
            .as_ref()
            .and_then(|validation| parse_level(validation.unused()));

        // Each check falls back to its own default (an undeclared payee is
        // an error, an unused declaration a warning), not to a single level.
        let defaults = ValidationOptions::default();
        Config {
            journal_file,
            validation: ValidationOptions {
                payee: payee_override.unwrap_or(defaults.payee),
                unused: unused_override.unwrap_or(defaults.unused),
            },
            declarations: DeclarationTargets::default(),
            payee_override,
            unused_override,
            declaration_paths: DeclarationOptions::default(),
        }
    }

    /// Resolves validation settings from the layered `pactole.toml` files.
    ///
    /// The project layer lives next to `journal_file` when one is configured,
    /// and at `workspace_root` otherwise. A configuration error is logged and
    /// the current settings (defaults, or the `initializationOptions`
    /// override) are kept, so a broken configuration file never prevents the
    /// server from starting.
    pub fn load_layered(&mut self, workspace_root: Option<&Path>) {
        let project_dir = self
            .journal_file
            .as_deref()
            .and_then(Path::parent)
            .or(workspace_root);

        let project = project_dir.map(|dir| dir.join("pactole.toml"));
        let overrides = Overrides {
            payee: self.payee_override,
            unused: self.unused_override,
        };

        match pactole_config::Config::load(None, project.as_deref(), &overrides) {
            Ok(config) => {
                self.validation = config.validation;
                self.declaration_paths = config.declaration;
            }
            Err(error) => {
                log::warn!(
                    "cannot load `pactole.toml`, keeping current validation settings: {error}"
                );
            }
        }

        self.resolve_declaration_paths();
    }

    /// Resolves the raw `[declaration]` paths against the journal's directory.
    ///
    /// Declaration files are looked up the same way an `include` written in
    /// the root journal is: relative to the journal's own directory. A
    /// relative path is therefore meaningful only when a `journal_file` is
    /// configured, which is also the only situation where business
    /// diagnostics exist at all — without a journal every target stays `None`
    /// and code actions keep editing the current document.
    fn resolve_declaration_paths(&mut self) {
        let Some(base) = self.journal_file.as_deref().and_then(Path::parent) else {
            self.declarations = DeclarationTargets::default();
            return;
        };

        let resolve = |raw: &Option<String>| {
            let raw = raw.as_deref()?;
            let path = PathBuf::from(raw);
            let path = if path.is_absolute() {
                path
            } else {
                base.join(path)
            };
            Some(pactole_storage_fs::normalize_lexically(&path))
        };

        self.declarations = DeclarationTargets {
            payees: resolve(&self.declaration_paths.payees),
            accounts: resolve(&self.declaration_paths.accounts),
            commodities: resolve(&self.declaration_paths.commodities),
        };
    }
}

/// Parses one `initializationOptions` validation level, logging and ignoring
/// an invalid value so that a client-side typo never fails `initialize`.
fn parse_level(value: Option<&str>) -> Option<ValidationLevel> {
    let value = value?;
    match ValidationLevel::parse(value) {
        Some(level) => Some(level),
        None => {
            log::warn!(
                "ignoring invalid `validation` initialization option `{value}` \
                 (expected `off`, `warn` or `error`)"
            );
            None
        }
    }
}

/// Derives the workspace root directory from an [`lsp_types::InitializeParams`],
/// preferring `workspace_folders`, then the deprecated `root_uri`, then the
/// even-older `root_path`.
pub fn workspace_root_from_initialize_params(
    params: &lsp_types::InitializeParams,
) -> Option<PathBuf> {
    if let Some(folders) = &params.workspace_folders
        && let Some(first) = folders.first()
        && let Ok(path) = first.uri.to_file_path()
    {
        return Some(path);
    }

    #[allow(deprecated)]
    if let Some(root_uri) = &params.root_uri
        && let Ok(path) = root_uri.to_file_path()
    {
        return Some(path);
    }

    #[allow(deprecated)]
    if let Some(root_path) = &params.root_path {
        return Some(PathBuf::from(root_path));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_initialization_options_yields_no_journal_file() {
        let config = Config::from_initialization_options(None, Some(Path::new("/workspace")));
        assert_eq!(config.journal_file, None);
        assert_eq!(config.validation.payee, ValidationLevel::Error);
    }

    #[test]
    fn relative_journal_file_is_resolved_against_the_workspace_root() {
        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/workspace/main.pactole"))
        );
    }

    #[test]
    fn absolute_journal_file_is_kept_as_is() {
        let options = serde_json::json!({ "journal_file": "/etc/journal.pactole" });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/etc/journal.pactole"))
        );
    }

    #[test]
    fn relative_journal_file_without_a_workspace_root_is_resolved_against_the_cwd() {
        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let config = Config::from_initialization_options(Some(&options), None);
        assert_eq!(
            config.journal_file,
            Some(std::env::current_dir().unwrap().join("main.pactole"))
        );
    }

    #[test]
    fn unknown_fields_in_initialization_options_are_ignored() {
        let options = serde_json::json!({ "journal_file": "main.pactole", "unknown": 42 });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/workspace/main.pactole"))
        );
    }

    #[test]
    fn malformed_initialization_options_falls_back_to_defaults() {
        let options = serde_json::json!("not an object");
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(config.journal_file, None);
    }

    #[test]
    fn validation_can_be_given_as_a_level_string() {
        let options = serde_json::json!({ "validation": "warn" });
        let config = Config::from_initialization_options(Some(&options), None);

        assert_eq!(config.validation.payee, ValidationLevel::Warning);
        // A single level applies to every configurable check.
        assert_eq!(config.validation.unused, ValidationLevel::Warning);
    }

    #[test]
    fn validation_can_be_given_as_a_table() {
        let options = serde_json::json!({ "validation": { "payee": "error" } });
        let config = Config::from_initialization_options(Some(&options), None);

        assert_eq!(config.validation.payee, ValidationLevel::Error);
    }

    #[test]
    fn the_unused_level_defaults_to_a_warning_and_can_be_set_per_check() {
        let config = Config::from_initialization_options(None, None);
        assert_eq!(config.validation.unused, ValidationLevel::Warning);

        let options = serde_json::json!({ "validation": { "unused": "off" } });
        let config = Config::from_initialization_options(Some(&options), None);

        assert_eq!(config.validation.unused, ValidationLevel::Off);
        // The other check keeps its own default.
        assert_eq!(config.validation.payee, ValidationLevel::Error);
    }

    #[test]
    fn off_level_is_accepted() {
        let options = serde_json::json!({ "validation": "off" });
        let config = Config::from_initialization_options(Some(&options), None);

        assert_eq!(config.validation.payee, ValidationLevel::Off);
    }

    #[test]
    fn load_layered_applies_the_project_file_next_to_the_journal() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-lsp-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pactole.toml"), "[validation]\npayee = \"warn\"\n").unwrap();

        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let mut config = Config::from_initialization_options(Some(&options), Some(&dir));
        config.load_layered(Some(&dir));

        assert_eq!(config.validation.payee, ValidationLevel::Warning);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_layered_applies_the_unused_level_from_the_project_file() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-lsp-config-unused-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pactole.toml"), "[validation]\nunused = \"off\"\n").unwrap();

        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let mut config = Config::from_initialization_options(Some(&options), Some(&dir));
        config.load_layered(Some(&dir));

        assert_eq!(config.validation.unused, ValidationLevel::Off);
        // The payee key stays at its own default.
        assert_eq!(config.validation.payee, ValidationLevel::Error);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn declaration_paths_are_resolved_against_the_journal_directory() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-lsp-declaration-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(dir.join("journal")).unwrap();
        std::fs::write(
            dir.join("journal").join("pactole.toml"),
            "[declaration]\npayees = \"payees.pactole\"\naccounts = \"/etc/pactole/accounts.pactole\"\n",
        )
        .unwrap();

        let options = serde_json::json!({ "journal_file": "journal/main.pactole" });
        let mut config = Config::from_initialization_options(Some(&options), Some(&dir));
        config.load_layered(Some(&dir));

        assert_eq!(
            config.declarations.payees,
            Some(dir.join("journal").join("payees.pactole"))
        );
        assert_eq!(
            config.declarations.accounts,
            Some(PathBuf::from("/etc/pactole/accounts.pactole"))
        );
        assert_eq!(config.declarations.commodities, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn declaration_paths_stay_unresolved_without_a_journal_file() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-lsp-declaration-no-journal-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pactole.toml"),
            "[declaration]\npayees = \"payees.pactole\"\n",
        )
        .unwrap();

        let mut config = Config::from_initialization_options(None, Some(&dir));
        config.load_layered(Some(&dir));

        assert_eq!(config.declarations, DeclarationTargets::default());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn initialization_options_override_the_project_file() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-lsp-config-override-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pactole.toml"),
            "[validation]\npayee = \"error\"\nunused = \"error\"\n",
        )
        .unwrap();

        let options = serde_json::json!({
            "journal_file": "main.pactole",
            "validation": "warn"
        });
        let mut config = Config::from_initialization_options(Some(&options), Some(&dir));
        config.load_layered(Some(&dir));

        assert_eq!(config.validation.payee, ValidationLevel::Warning);
        // A single `validation` level overrides every check, `unused` too.
        assert_eq!(config.validation.unused, ValidationLevel::Warning);

        std::fs::remove_dir_all(&dir).ok();
    }
}
