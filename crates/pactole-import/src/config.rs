use serde::Deserialize;

/// Configuration for importing an external statement (currently OFX) into
/// Pactole transactions.
///
/// This type only describes the mapping; it performs no I/O and knows nothing
/// about where the configuration comes from. Callers (e.g. `pactole-cli`)
/// deserialize it from their configuration file, typically
/// `~/.config/pactole/import-ofx.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportConfig {
    /// The Pactole account corresponding to the statement being imported
    /// (e.g. `Actifs:Banque`). Money movements are recorded against this
    /// account with the signed amount from the source.
    pub bank_account: String,

    /// Fallback commodity (e.g. `EUR`) used when the source transaction
    /// carries no currency of its own.
    #[serde(default)]
    pub currency: Option<String>,

    /// Counter account used for transactions that no [`Rule`] matches (e.g.
    /// `Depenses:A-Classer`). This posting is the "unknown category" side of
    /// the transaction, to be categorized later.
    pub default_counter_account: String,

    /// Rules evaluated in order, first match wins, used to determine the
    /// counter account and optionally the canonical payee of each
    /// transaction.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// A single import rule: a set of optional, conjunctive match criteria over an
/// [`crate::ImportRecord`], plus the counter account (and optional canonical
/// payee) to use when it matches.
///
/// A rule with no criteria matches every transaction (useful as a catch-all
/// placed after more specific rules).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Case-insensitive substring matched against the source payee/name.
    #[serde(default)]
    pub payee_contains: Option<String>,

    /// Case-insensitive substring matched against the source memo.
    #[serde(default)]
    pub memo_contains: Option<String>,

    /// Source transaction type to match exactly, case-insensitively (e.g.
    /// `DEBIT`, `POS`, `XFER`).
    #[serde(default)]
    pub trntype: Option<String>,

    /// Direction of the money movement to match.
    #[serde(default)]
    pub direction: Option<Direction>,

    /// Pactole counter account to post the opposite side of a matching
    /// transaction to (e.g. `Depenses:Alimentation`).
    pub counter_account: String,

    /// Optional canonical payee to use instead of the raw source payee.
    #[serde(default)]
    pub payee: Option<String>,
}

/// The direction of a money movement, as seen from the imported account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Money leaving the imported account (negative amount).
    Debit,
    /// Money entering the imported account (positive amount).
    Credit,
}
