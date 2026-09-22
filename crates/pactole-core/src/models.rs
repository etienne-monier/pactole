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

impl Transaction {
    /// Builds a new transaction, validating and auto-balancing its
    /// postings.
    ///
    /// At most one posting may be left without an amount: if that is the
    /// case, and the other postings all share the same commodity, the
    /// missing amount is computed and filled in so that the transaction
    /// balances. If more than one posting has no amount, or the known
    /// amounts use several commodities while one is missing, an error is
    /// returned. If every posting already has an amount, the postings must
    /// sum to zero for each commodity.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        date: NaiveDate,
        effective_date: Option<NaiveDate>,
        status: TransactionStatus,
        payee: Option<String>,
        narration: Option<String>,
        postings: Vec<Posting>,
        tags: Vec<String>,
        links: Vec<String>,
        reference: Option<String>,
        meta: Metadata,
    ) -> Result<Self, ModelError> {
        let mut transaction = Self {
            date,
            effective_date,
            status,
            payee,
            narration,
            postings,
            tags,
            links,
            reference,
            meta,
        };
        transaction.auto_balance()?;
        Ok(transaction)
    }

    /// Validates and auto-balances the transaction's postings. Called by
    /// [`Transaction::new`]; kept private so the two steps cannot be
    /// forgotten.
    fn auto_balance(&mut self) -> Result<(), ModelError> {
        let missing: Vec<usize> = self
            .postings
            .iter()
            .enumerate()
            .filter(|(_, posting)| posting.amount.is_none())
            .map(|(index, _)| index)
            .collect();

        if missing.len() > 1 {
            return Err(ModelError::TooManyPostingsWithoutAmount(missing.len()));
        }

        let mut sums: BTreeMap<&str, (CommodityName, Decimal)> = BTreeMap::new();
        for posting in &self.postings {
            if let Some(amount) = &posting.amount {
                sums.entry(amount.commodity.as_str())
                    .or_insert_with(|| (amount.commodity.clone(), Decimal::ZERO))
                    .1 += amount.number;
            }
        }

        if let Some(index) = missing.first().copied() {
            if sums.len() != 1 {
                let commodities: Vec<String> = sums.keys().map(|k| k.to_string()).collect();
                return Err(ModelError::AmbiguousAutoBalanceCommodity(commodities));
            }

            let (_, (commodity, sum)) = sums.into_iter().next().unwrap();
            self.postings[index].amount = Some(Amount {
                number: -sum,
                commodity,
            });
            return Ok(());
        }

        for (_, (commodity, sum)) in sums {
            if !sum.is_zero() {
                return Err(ModelError::TransactionNotBalanced(
                    commodity.as_str().to_string(),
                    sum,
                ));
            }
        }

        Ok(())
    }
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

    fn posting(account: &str, amount: Option<(&str, &str)>) -> Posting {
        Posting {
            account: AccountName::new(account).unwrap(),
            amount: amount.map(|(number, commodity)| Amount {
                number: number.parse().unwrap(),
                commodity: CommodityName::new(commodity).unwrap(),
            }),
            meta: Metadata::new(),
        }
    }

    fn transaction(postings: Vec<Posting>) -> Result<Transaction, ModelError> {
        Transaction::new(
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            None,
            TransactionStatus::Cleared,
            None,
            None,
            postings,
            Vec::new(),
            Vec::new(),
            None,
            Metadata::new(),
        )
    }

    #[test]
    fn auto_balance_fills_missing_amount() {
        let txn = transaction(vec![
            posting("Actifs:Compte", Some(("100.00", "EUR"))),
            posting("Depenses:Divers", None),
        ])
        .unwrap();

        assert_eq!(
            txn.postings[1].amount,
            Some(Amount {
                number: "-100.00".parse().unwrap(),
                commodity: CommodityName::new("EUR").unwrap(),
            })
        );
    }

    #[test]
    fn auto_balance_accepts_already_balanced_transaction() {
        assert!(transaction(vec![
            posting("Actifs:Compte", Some(("100.00", "EUR"))),
            posting("Depenses:Divers", Some(("-100.00", "EUR"))),
        ])
        .is_ok());
    }

    #[test]
    fn auto_balance_rejects_unbalanced_transaction() {
        assert_eq!(
            transaction(vec![
                posting("Actifs:Compte", Some(("100.00", "EUR"))),
                posting("Depenses:Divers", Some(("-50.00", "EUR"))),
            ]),
            Err(ModelError::TransactionNotBalanced(
                "EUR".to_string(),
                "50.00".parse().unwrap()
            ))
        );
    }

    #[test]
    fn auto_balance_rejects_more_than_one_missing_amount() {
        assert_eq!(
            transaction(vec![
                posting("Actifs:Compte", None),
                posting("Depenses:Divers", None),
            ]),
            Err(ModelError::TooManyPostingsWithoutAmount(2))
        );
    }

    #[test]
    fn auto_balance_rejects_ambiguous_commodity_for_missing_amount() {
        assert_eq!(
            transaction(vec![
                posting("Actifs:Compte", Some(("100.00", "EUR"))),
                posting("Actifs:Autre", Some(("50.00", "USD"))),
                posting("Depenses:Divers", None),
            ]),
            Err(ModelError::AmbiguousAutoBalanceCommodity(vec![
                "EUR".to_string(),
                "USD".to_string()
            ]))
        );
    }
}
