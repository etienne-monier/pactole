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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationOptions {
    /// Severity of a transaction whose payee has no matching `payee`
    /// declaration in the journal (the `payee` key).
    pub payee: ValidationLevel,
    /// Severity of a declaration that no entry of the journal ever exercises
    /// (the `unused` key): an account opened but never used by a transaction
    /// or balance assertion, a commodity declared but never used by any
    /// amount, a payee declared but never used by any transaction.
    pub unused: ValidationLevel,
}

impl Default for ValidationOptions {
    /// Undeclared payees are errors, unused declarations are warnings: the
    /// undeclared-payee check is a correctness rule that has always blocked
    /// `pactole check`, whereas unused declarations are only reported for
    /// cleanup, so they never block unless explicitly raised to `error`.
    fn default() -> Self {
        Self {
            payee: ValidationLevel::Error,
            unused: ValidationLevel::Warning,
        }
    }
}
