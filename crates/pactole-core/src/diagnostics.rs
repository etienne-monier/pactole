//! The stable, machine-readable catalogue of Pactole diagnostic codes.
//!
//! A [`DiagnosticCode`] identifies *what kind* of problem a diagnostic
//! reports, independently of where it was produced (syntax analysis,
//! lowering, include resolution or business validation) and of the message
//! rendered to the user. The text form returned by [`DiagnosticCode::as_str`]
//! is what the `pactole` CLI prints in brackets and what `pactole-lsp`
//! publishes as an LSP diagnostic code, so it is a stable public contract:
//! rename a variant freely, never change its string.
//!
//! The catalogue lives here, in the leaf `pactole-core` crate, so that every
//! layer can name a code without depending on the layer that produces it.
//! `pactole-core` itself only produces the business codes (see
//! [`crate::ValidationError::code`]); `pactole-storage-fs` maps the syntax,
//! lowering and include issues onto the remaining variants.

use std::fmt;

/// A stable, machine-readable identifier for a kind of diagnostic.
///
/// The enum is deliberately closed: adding a variant is a compile error in
/// every exhaustive mapping, which is the intended reminder to document the
/// new code in the diagnostics reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    // ---- Syntax and lowering ----
    /// The tolerant parser found an `ERROR` or a `MISSING` node.
    SyntaxError,
    /// A syntactically recognizable directive could not be converted into a
    /// domain entry.
    LoweringError,

    // ---- Include graph ----
    /// An included file does not exist.
    IncludeNotFound,
    /// An included file exists but could not be loaded.
    IncludeLoadFailed,
    /// A relative include has no usable base directory.
    IncludeNoBaseDirectory,
    /// Include recursion exceeded the safety limit.
    IncludeDepthExceeded,
    /// Resolving the include would revisit a file already being analyzed.
    IncludeCycle,

    // ---- Analysis completeness ----
    /// Part of the project could not be analyzed, so the business checks
    /// that depend on the missing content were skipped.
    AnalysisIncomplete,

    // ---- Business validation ----
    /// An account is opened while it is already open.
    AccountAlreadyOpen,
    /// An account is used or closed while it is not open.
    AccountNotOpen,
    /// A commodity is declared more than once.
    CommodityAlreadyDeclared,
    /// A commodity is used before being declared.
    CommodityNotDeclared,
    /// A payee is declared more than once.
    PayeeAlreadyDeclared,
    /// A transaction uses a payee that has not been declared.
    PayeeNotDeclared,
    /// More than one posting of a transaction omits its amount.
    TooManyPostingsWithoutAmount,
    /// The commodity of an inferred amount cannot be determined.
    AmbiguousAutoBalanceCommodity,
    /// The postings of a transaction do not sum to zero for a commodity.
    TransactionNotBalanced,
    /// A posting uses a commodity excluded by the account's `open`
    /// restriction.
    CommodityNotAllowed,
    /// A `balance` assertion differs from the computed balance beyond its
    /// tolerance.
    BalanceAssertionFailed,
    /// An opened account is never used by a transaction nor a balance
    /// assertion.
    AccountUnused,
    /// A declared commodity is never used by any amount.
    CommodityUnused,
    /// A declared payee is never used by any transaction.
    PayeeUnused,
}

impl DiagnosticCode {
    /// Every diagnostic code, in catalogue order.
    ///
    /// This is the exhaustive list the documentation is checked against, so a
    /// new variant must be added here together with its reference entry.
    pub const ALL: &[Self] = &[
        Self::SyntaxError,
        Self::LoweringError,
        Self::IncludeNotFound,
        Self::IncludeLoadFailed,
        Self::IncludeNoBaseDirectory,
        Self::IncludeDepthExceeded,
        Self::IncludeCycle,
        Self::AnalysisIncomplete,
        Self::AccountAlreadyOpen,
        Self::AccountNotOpen,
        Self::CommodityAlreadyDeclared,
        Self::CommodityNotDeclared,
        Self::PayeeAlreadyDeclared,
        Self::PayeeNotDeclared,
        Self::TooManyPostingsWithoutAmount,
        Self::AmbiguousAutoBalanceCommodity,
        Self::TransactionNotBalanced,
        Self::CommodityNotAllowed,
        Self::BalanceAssertionFailed,
        Self::AccountUnused,
        Self::CommodityUnused,
        Self::PayeeUnused,
    ];

    /// Returns the stable text form of this code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SyntaxError => "syntax-error",
            Self::LoweringError => "lowering-error",
            Self::IncludeNotFound => "include-not-found",
            Self::IncludeLoadFailed => "include-load-failed",
            Self::IncludeNoBaseDirectory => "include-no-base-directory",
            Self::IncludeDepthExceeded => "include-depth-exceeded",
            Self::IncludeCycle => "include-cycle",
            Self::AnalysisIncomplete => "analysis-incomplete",
            Self::AccountAlreadyOpen => "account-already-open",
            Self::AccountNotOpen => "account-not-open",
            Self::CommodityAlreadyDeclared => "commodity-already-declared",
            Self::CommodityNotDeclared => "commodity-not-declared",
            Self::PayeeAlreadyDeclared => "payee-already-declared",
            Self::PayeeNotDeclared => "payee-not-declared",
            Self::TooManyPostingsWithoutAmount => "too-many-postings-without-amount",
            Self::AmbiguousAutoBalanceCommodity => "ambiguous-auto-balance-commodity",
            Self::TransactionNotBalanced => "transaction-not-balanced",
            Self::CommodityNotAllowed => "commodity-not-allowed",
            Self::BalanceAssertionFailed => "balance-assertion-failed",
            Self::AccountUnused => "account-unused",
            Self::CommodityUnused => "commodity-unused",
            Self::PayeeUnused => "payee-unused",
        }
    }

    /// Parses a stable text form back into a code.
    ///
    /// Returns `None` for any text that is not part of the catalogue, which is
    /// what a front-end must tolerate when it reads back a code it did not
    /// produce (for example an editor sending back an older diagnostic).
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == value)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticCode;
    use std::collections::HashSet;

    #[test]
    fn the_catalogue_is_exhaustive_and_its_texts_are_unique() {
        let texts: HashSet<&str> = DiagnosticCode::ALL
            .iter()
            .map(|code| code.as_str())
            .collect();

        assert_eq!(texts.len(), DiagnosticCode::ALL.len());
        assert!(
            DiagnosticCode::ALL
                .iter()
                .all(|code| !code.as_str().is_empty())
        );
    }

    #[test]
    fn parse_round_trips_every_code() {
        for code in DiagnosticCode::ALL {
            assert_eq!(DiagnosticCode::parse(code.as_str()), Some(*code));
            assert_eq!(code.to_string(), code.as_str());
        }
    }

    #[test]
    fn parse_rejects_an_unknown_code() {
        assert_eq!(DiagnosticCode::parse("not-a-code"), None);
        assert_eq!(DiagnosticCode::parse(""), None);
    }
}
