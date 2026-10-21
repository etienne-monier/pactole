//! Layered `pactole.toml` configuration resolution for Pactole tools.
//!
//! Every Pactole front-end (the `pactole` CLI and the `pactole-lsp` language
//! server) shares a single configuration file format and a single resolution
//! rule: a **global** layer (`$XDG_CONFIG_HOME/pactole/pactole.toml`, falling
//! back to `%APPDATA%` on Windows and `~/.config/pactole/` otherwise) is
//! overridden by a **project** layer (`pactole.toml` next to the journal),
//! which is in turn overridden by in-memory **overrides** (the CLI's global
//! `--config`/`--validation` flags, the LSP's `initializationOptions`).
//!
//! Layers merge per key: a key absent from a higher layer keeps the value
//! from the lower one. Tables merge per entry (`[import.accounts]` unions),
//! except `[[import.rules]]`, which is replaced in block by the highest
//! layer defining it.
//!
//! ```toml
//! [validation]
//! payee = "warn" # `off`, `warn` or `error`
//! unused = "warn" # severity of never-used declarations
//!
//! [declaration]
//! payees = "payees.pactole"
//! accounts = "accounts.pactole"
//!
//! [format]
//! posting_width = 50
//!
//! [import]
//! bank_account = "Assets:Checking"
//! currency = "EUR"
//! default_counter_account = "Expenses:To-Be-Classified"
//! # [import.accounts]
//! # "00012345678" = "Assets:Checking"
//! # [[import.rules]]
//! # payee_contains = "supermarket"
//! # counter_account = "Expenses:Groceries"
//! ```
//!
//! This crate is a leaf: it depends on no other Pactole crate. All other
//! crates depend on it for their configuration needs.

mod declaration;
mod format;
mod import;
mod raw;
mod validation;

use std::path::{Path, PathBuf};

use crate::raw::{RawDirection, RawFile, RawRule};

pub use crate::declaration::DeclarationOptions;
pub use crate::format::FormatOptions;
pub use crate::import::{Direction, ImportConfig, Rule};
pub use crate::validation::{ValidationLevel, ValidationOptions};

/// Errors raised while loading or resolving a `pactole.toml` file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A configuration file could not be read.
    #[error("cannot read configuration {path}: {source}")]
    Read {
        /// Path of the file that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// A configuration file is not valid TOML, or a section contains an
    /// unknown key.
    #[error("cannot parse configuration {path}: {message}")]
    Parse {
        /// Path of the file that failed to parse.
        path: PathBuf,
        /// The underlying parse error.
        message: String,
    },

    /// A configuration value is invalid (unknown validation level,
    /// non-positive posting width, incomplete `[import]` section, ...).
    #[error("invalid configuration {path}: {message}")]
    Invalid {
        /// Path of the file holding the invalid value.
        path: PathBuf,
        /// Human-readable explanation of the problem.
        message: String,
    },
}

/// A single configuration layer about to be merged.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    /// Path of the layer's file, used in error messages.
    pub path: &'a Path,
    /// Raw TOML contents of the layer.
    pub contents: &'a str,
}

/// In-memory overrides applied on top of every configuration file layer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Overrides {
    /// Overrides the `payee` validation level (CLI `--validation`, LSP
    /// `initializationOptions`). `None` leaves the file layers untouched.
    pub payee: Option<ValidationLevel>,
    /// Overrides the `unused` validation level (LSP `initializationOptions`).
    /// `None` leaves the file layers untouched.
    pub unused: Option<ValidationLevel>,
}

/// The fully resolved Pactole configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Validation strictness (the `[validation]` section).
    pub validation: ValidationOptions,
    /// Declaration files (the `[declaration]` section), with paths kept
    /// unresolved.
    pub declaration: DeclarationOptions,
    /// Formatter options (the `[format]` section).
    pub format: FormatOptions,
    /// Import settings (the `[import]` section), or `None` when no layer
    /// defines it — an import command then fails with a clear message.
    pub import: Option<ImportConfig>,
}

impl Config {
    /// Merges the given layers in order (lowest precedence first), then
    /// applies `overrides` on top.
    ///
    /// This performs no I/O, so it is trivially testable; use [`Config::load`]
    /// to read the layers from disk.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Parse`] when a layer is not valid TOML or
    /// carries an unknown section key, and [`ConfigError::Invalid`] when a
    /// value is invalid (e.g. an unsupported validation level, a zero
    /// `posting_width`, or an `[import]` section missing one of its required
    /// keys).
    pub fn resolve(layers: &[Source<'_>], overrides: &Overrides) -> Result<Self, ConfigError> {
        let mut merged = RawFile::default();
        let mut import_path: Option<&Path> = None;

        for layer in layers {
            let raw: RawFile =
                toml::from_str(layer.contents).map_err(|error| ConfigError::Parse {
                    path: layer.path.to_path_buf(),
                    message: error.to_string(),
                })?;
            // Validate each layer's own values so that an error always names
            // the file actually holding the bad value, not the highest
            // precedence layer.
            validate_layer(&raw, layer.path)?;
            if raw.import.is_some() {
                import_path = Some(layer.path);
            }
            merged.merge(raw);
        }

        Ok(Config {
            validation: resolve_validation(&merged, overrides),
            declaration: resolve_declaration(&merged),
            format: resolve_format(&merged),
            import: resolve_import(&merged, import_path)?,
        })
    }

    /// Resolves the configuration from disk.
    ///
    /// - When `explicit` is given, that single file is the only layer used
    ///   (the CLI's `--config <path>` flag); an unreadable file is an error.
    /// - Otherwise the global layer (see [`config_dir`]) and the `project`
    ///   file are merged, each skipped silently when absent.
    /// - `overrides` are applied on top in both cases.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Read`] when an explicitly requested file (or a
    /// discovered one that exists) cannot be read, and the resolution errors
    /// of [`Config::resolve`] otherwise.
    pub fn load(
        explicit: Option<&Path>,
        project: Option<&Path>,
        overrides: &Overrides,
    ) -> Result<Self, ConfigError> {
        if let Some(path) = explicit {
            let contents = read_file(path)?;
            return Self::resolve(
                &[Source {
                    path,
                    contents: &contents,
                }],
                overrides,
            );
        }

        // Owned layer contents must outlive the `Source` borrows below.
        let mut owned: Vec<(PathBuf, String)> = Vec::new();

        if let Some(path) = default_config_path()
            && let Some(contents) = read_file_if_exists(&path)?
        {
            owned.push((path, contents));
        }

        if let Some(path) = project
            && let Some(contents) = read_file_if_exists(path)?
        {
            owned.push((path.to_path_buf(), contents));
        }

        let layers: Vec<Source<'_>> = owned
            .iter()
            .map(|(path, contents)| Source { path, contents })
            .collect();

        Self::resolve(&layers, overrides)
    }
}

/// Directory where Pactole user configuration files live: `$XDG_CONFIG_HOME`
/// when set and non-empty, `%APPDATA%` on Windows, otherwise
/// `~/.config/pactole/`.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("pactole"));
    }

    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA")
        && !appdata.is_empty()
    {
        return Some(PathBuf::from(appdata).join("pactole"));
    }

    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("pactole"))
}

/// Default path of the global `pactole.toml` file.
pub fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("pactole.toml"))
}

/// Validates the values defined by a single layer, so that an error names the
/// file that actually holds the bad value rather than the highest-precedence
/// layer.
fn validate_layer(raw: &RawFile, path: &Path) -> Result<(), ConfigError> {
    let invalid = |message: String| ConfigError::Invalid {
        path: path.to_path_buf(),
        message,
    };

    if let Some(value) = raw.validation.as_ref().and_then(|v| v.payee.as_deref())
        && ValidationLevel::parse(value).is_none()
    {
        return Err(invalid(format!(
            "invalid validation level `{value}`: expected `off`, `warn` or `error`"
        )));
    }

    if let Some(value) = raw.validation.as_ref().and_then(|v| v.unused.as_deref())
        && ValidationLevel::parse(value).is_none()
    {
        return Err(invalid(format!(
            "invalid validation level `{value}`: expected `off`, `warn` or `error`"
        )));
    }

    if let Some(posting_width) = raw.format.as_ref().and_then(|f| f.posting_width)
        && posting_width == 0
    {
        return Err(invalid(
            "posting_width must be greater than zero".to_string(),
        ));
    }

    Ok(())
}

fn resolve_validation(merged: &RawFile, overrides: &Overrides) -> ValidationOptions {
    // Values are already validated layer by layer (`validate_layer`).
    let payee = merged
        .validation
        .as_ref()
        .and_then(|v| v.payee.as_deref())
        .and_then(ValidationLevel::parse);
    let unused = merged
        .validation
        .as_ref()
        .and_then(|v| v.unused.as_deref())
        .and_then(ValidationLevel::parse);

    // Each key falls back to the default of its own check (`payee` is an
    // error, `unused` a warning) rather than to a single global default.
    let defaults = ValidationOptions::default();
    ValidationOptions {
        payee: overrides.payee.or(payee).unwrap_or(defaults.payee),
        unused: overrides.unused.or(unused).unwrap_or(defaults.unused),
    }
}

fn resolve_declaration(merged: &RawFile) -> DeclarationOptions {
    let Some(declaration) = &merged.declaration else {
        return DeclarationOptions::default();
    };

    DeclarationOptions {
        payees: declaration.payees.clone(),
        accounts: declaration.accounts.clone(),
        commodities: declaration.commodities.clone(),
    }
}

fn resolve_format(merged: &RawFile) -> FormatOptions {
    let mut options = FormatOptions::default();

    // A zero width is rejected by `validate_layer`, so any surviving value is
    // valid to apply.
    if let Some(posting_width) = merged.format.as_ref().and_then(|f| f.posting_width)
        && posting_width > 0
    {
        options.posting_width = posting_width;
    }

    options
}

fn resolve_import(
    merged: &RawFile,
    path: Option<&Path>,
) -> Result<Option<ImportConfig>, ConfigError> {
    let Some(import) = &merged.import else {
        return Ok(None);
    };

    let invalid = |message: String| ConfigError::Invalid {
        path: path.unwrap_or(Path::new("<configuration>")).to_path_buf(),
        message,
    };

    let bank_account = import
        .bank_account
        .clone()
        .ok_or_else(|| invalid("`[import]` requires `bank_account`".to_string()))?;
    let default_counter_account = import
        .default_counter_account
        .clone()
        .ok_or_else(|| invalid("`[import]` requires `default_counter_account`".to_string()))?;

    Ok(Some(ImportConfig {
        bank_account,
        accounts: import.accounts.clone().unwrap_or_default(),
        currency: import.currency.clone(),
        default_counter_account,
        rules: import
            .rules
            .as_ref()
            .map(|rules| rules.iter().map(rule_from_raw).collect())
            .unwrap_or_default(),
    }))
}

fn rule_from_raw(rule: &RawRule) -> Rule {
    Rule {
        payee_contains: rule.payee_contains.clone(),
        memo_contains: rule.memo_contains.clone(),
        trntype: rule.trntype.clone(),
        direction: rule.direction.map(|direction| match direction {
            RawDirection::Debit => Direction::Debit,
            RawDirection::Credit => Direction::Credit,
        }),
        counter_account: rule.counter_account.clone(),
        payee: rule.payee.clone(),
    }
}

fn read_file(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn read_file_if_exists(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}
