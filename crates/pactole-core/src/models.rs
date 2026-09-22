use crate::errors::ModelError;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;

// -------------------------------------------------
// Journal structure
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Journal {
    pub entries: Vec<Entry>,
}

impl Journal {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Open(Open),
    Close(Close),
    Commodity(Commodity),
    Transaction(Transaction),
    Balance(Balance),
}

// -------------------------------------------------
// Declarations
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub date: NaiveDate,
    pub account: AccountName,
    pub description: Option<String>,
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub date: NaiveDate,
    pub account: AccountName,
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Commodity {
    pub date: NaiveDate,
    pub name: CommodityName,
    pub meta: Metadata,
}

// -------------------------------------------------
// Events
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub date: NaiveDate,
    pub effective_date: Option<NaiveDate>,

    pub status: TransactionStatus,
    pub payee: Option<String>,
    pub narration: Option<String>,

    pub postings: Vec<Posting>,

    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub reference: Option<String>,
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub account: AccountName,
    pub amount: Option<Amount>,
    pub meta: Metadata,
}

// -------------------------------------------------
// Assertions
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    pub date: NaiveDate,
    pub account: AccountName,
    pub amount: Amount,
    pub tolerance: Option<Decimal>,
    pub meta: Metadata,
}

// -------------------------------------------------
// Misc
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Amount {
    pub number: Decimal,
    pub commodity: CommodityName,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Uncleared,
    Pending,
    Cleared,
}

pub type Metadata = BTreeMap<MetadataKey, String>;

/// A metadata key: only lowercase letters, `_` or `-` (see the `key` rule
/// in `grammar.js`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MetadataKey(String);

impl MetadataKey {
    /// Build a new metadata key, validating it against the grammar's `key`
    /// rule: `/[a-z_\-]+/`.
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        if is_valid_metadata_key(&value) {
            Ok(Self(value))
        } else {
            Err(ModelError::InvalidMetadataKey(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for MetadataKey {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MetadataKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An account name: colon-separated segments, the first one starting with
/// an uppercase letter, each made of letters, digits, `_` or `-` (see the
/// `account` rule in `grammar.js`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountName(String);

impl AccountName {
    /// Build a new account name, validating it against the grammar's
    /// `account` rule: `/[A-Z][A-Za-z0-9_\-]*(?::[A-Za-z0-9_\-]+)*/`.
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        if is_valid_account_name(&value) {
            Ok(Self(value))
        } else {
            Err(ModelError::InvalidAccountName(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A commodity name: a Beancount-like currency code of 2 to 24 characters,
/// starting with an uppercase letter, ending with an uppercase letter or
/// digit, and made of uppercase letters, digits, `'`, `.`, `_` or `-` in
/// between (see the `commodity_name` rule in `grammar.js`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommodityName(String);

impl CommodityName {
    /// Build a new commodity name, validating it against the grammar's
    /// `commodity_name` rule: `/[A-Z][A-Z0-9'._\-]{0,22}[A-Z0-9]/`.
    pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
        let value = value.into();
        if is_valid_commodity_name(&value) {
            Ok(Self(value))
        } else {
            Err(ModelError::InvalidCommodityName(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommodityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid_metadata_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
}

fn is_valid_account_name(value: &str) -> bool {
    let mut segments = value.split(':');

    let Some(first) = segments.next() else {
        return false;
    };
    if !is_valid_account_first_segment(first) {
        return false;
    }

    segments.all(is_valid_account_other_segment)
}

fn is_valid_account_first_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(is_account_char)
}

fn is_valid_account_other_segment(segment: &str) -> bool {
    !segment.is_empty() && segment.chars().all(is_account_char)
}

fn is_account_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_valid_commodity_name(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();

    // Beancount-like currency code: 2 to 24 characters, starting with an
    // uppercase letter, ending with an uppercase letter or digit, and made
    // of uppercase letters, digits, `'`, `.`, `_` or `-` in between.
    if !(2..=24).contains(&len) {
        return false;
    }
    if !chars[0].is_ascii_uppercase() {
        return false;
    }
    let last = chars[len - 1];
    if !(last.is_ascii_uppercase() || last.is_ascii_digit()) {
        return false;
    }
    chars[1..len - 1]
        .iter()
        .all(|&c| c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '\'' | '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_name_accepts_valid_values() {
        assert!(AccountName::new("Actifs").is_ok());
        assert!(AccountName::new("Actifs:Compte-Joint").is_ok());
        assert!(AccountName::new("Actifs:Compte_Joint:sous2").is_ok());
    }

    #[test]
    fn account_name_rejects_invalid_values() {
        assert!(AccountName::new("").is_err());
        assert!(AccountName::new("actifs").is_err());
        assert!(AccountName::new("Actifs:").is_err());
        assert!(AccountName::new(":Actifs").is_err());
        assert!(AccountName::new("Actifs::Compte").is_err());
        assert!(AccountName::new("Actifs Compte").is_err());
    }

    #[test]
    fn commodity_name_accepts_valid_values() {
        assert!(CommodityName::new("EUR").is_ok());
        assert!(CommodityName::new("BTC.SAT-2").is_ok());
        assert!(CommodityName::new("BRK'A").is_ok());
        assert!(CommodityName::new("AB").is_ok());
    }

    #[test]
    fn commodity_name_rejects_invalid_values() {
        assert!(CommodityName::new("").is_err());
        assert!(CommodityName::new("A").is_err()); // too short (min 2 chars)
        assert!(CommodityName::new("eur").is_err());
        assert!(CommodityName::new("1EUR").is_err());
        assert!(CommodityName::new("EU R").is_err());
        assert!(CommodityName::new("EUR-").is_err()); // must end with letter/digit
        assert!(CommodityName::new(&format!("A{}", "B".repeat(24))).is_err()); // too long (> 24 chars)
    }

    #[test]
    fn metadata_key_accepts_valid_values() {
        assert!(MetadataKey::new("note").is_ok());
        assert!(MetadataKey::new("opened_on").is_ok());
        assert!(MetadataKey::new("some-key").is_ok());
    }

    #[test]
    fn metadata_key_rejects_invalid_values() {
        assert!(MetadataKey::new("").is_err());
        assert!(MetadataKey::new("Note").is_err());
        assert!(MetadataKey::new("note:").is_err());
        assert!(MetadataKey::new("note key").is_err());
    }
}
