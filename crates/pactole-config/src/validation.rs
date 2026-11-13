/// A configurable validation severity level (the values accepted by the
/// keys of the `[validation]` section of `pactole.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Do not report the check at all.
    Off,
    /// Report the issue as a warning: visible everywhere, but never
    /// blocking (except under a strict flag such as `pactole check --strict`).
    Warning,
    /// Report the issue as an error: blocking (the default for the
    /// undeclared-payee check).
    #[default]
    Error,
}

impl ValidationLevel {
    /// Returns the canonical `pactole.toml` spelling of this level.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warning => "warn",
            Self::Error => "error",
        }
    }

    /// Parses a level spelling (`off`, `warn` or `error`).
    ///
    /// Returns `None` for any other value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "warn" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// How strictly business validation rules are enforced (the `[validation]`
/// section of `pactole.toml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOptions {
    /// Severity of a transaction whose payee has no matching `payee`
    /// declaration in the journal (the `payee` key).
    pub payee: ValidationLevel,
    /// Severity of a declaration that no entry of the journal ever exercises
    /// (the `unused` key): an account opened but never used by a transaction
    /// or balance assertion, a commodity declared but never used by any
    /// amount, a payee declared but never used by any transaction.
    pub unused: ValidationLevel,
    /// Severity of a `pactole.toml` value that references a journal
    /// declaration which does not exist (the `config` key): an account that
    /// is not opened, a payee or commodity that is not declared, or an account
    /// root that no opened account uses.
    pub config: ValidationLevel,
    /// Severity of an opened account that is never used.
    pub account_unused: Option<ValidationLevel>,
    /// Severity of a declared commodity that is never used.
    pub commodity_unused: Option<ValidationLevel>,
    /// Severity of a declared payee that is never used.
    pub payee_unused: Option<ValidationLevel>,
    /// Severity of a `poptag` that removes an inactive tag.
    pub tag_not_active: Option<ValidationLevel>,
    /// Severity of a posting using one of the configured unclassified
    /// accounts.
    pub unclassified: ValidationLevel,
    /// Accounts whose use means that a transaction still needs to be
    /// classified. Descendants of a configured account are included.
    pub unclassified_accounts: Vec<String>,
}

impl Default for ValidationOptions {
    /// Undeclared payees are errors, unused declarations are warnings: the
    /// undeclared-payee check is a correctness rule that has always blocked
    /// `pactole check`, whereas unused declarations are only reported for
    /// cleanup, so they never block unless explicitly raised to `error`.
    ///
    /// A configuration reference is also a warning by default: a global
    /// `pactole.toml` may legitimately be shared by several journals, so a
    /// name it references is not necessarily an error in every project.
    fn default() -> Self {
        Self {
            payee: ValidationLevel::Error,
            unused: ValidationLevel::Warning,
            config: ValidationLevel::Warning,
            account_unused: None,
            commodity_unused: None,
            payee_unused: None,
            tag_not_active: None,
            unclassified: ValidationLevel::Warning,
            unclassified_accounts: Vec::new(),
        }
    }
}
