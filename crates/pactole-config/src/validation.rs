/// A configurable validation severity level (the values accepted by
/// `payee` in the `[validation]` section of `pactole.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Do not report undeclared payees.
    Off,
    /// Report the issue as a warning: visible everywhere, but never
    /// blocking (except under a strict flag such as `pactole check --strict`).
    Warning,
    /// Report the issue as an error: blocking (the default).
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

    /// Parses a level spelling (`warn` or `error`).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidationOptions {
    /// Severity of a transaction whose payee has no matching `payee`
    /// declaration in the journal.
    pub payee: ValidationLevel,
}
