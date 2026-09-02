use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;

// -------------------------------------------------
// Journal structure
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Journal {
    entries: Vec<Entry>,
}

impl Journal {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Event(Event),
    Declaration(Declaration),
    Assertion(Assertion),
}

// `Entry` groups three different sub-enums (`Declaration`, `Event`,
// `Assertion`) but the TOML source flattens all of them under a single
// `type = "..."` tag. We deserialize into a flat, tagged helper enum and
// then re-nest the result into the proper `Entry` variant.
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum TaggedEntry {
            Commodity(Commodity),
            Open(Open),
            Close(Close),
            Transaction(Transaction),
            Balance(Balance),
        }

        Ok(match TaggedEntry::deserialize(deserializer)? {
            TaggedEntry::Commodity(c) => Entry::Declaration(Declaration::Commodity(c)),
            TaggedEntry::Open(o) => Entry::Declaration(Declaration::Open(o)),
            TaggedEntry::Close(c) => Entry::Declaration(Declaration::Close(c)),
            TaggedEntry::Transaction(t) => Entry::Event(Event::Transaction(t)),
            TaggedEntry::Balance(b) => Entry::Assertion(Assertion::Balance(b)),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    Open(Open),
    Close(Close),
    Commodity(Commodity),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Transaction(Transaction),
    BudgetAllocation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    Balance(Balance),
    // Price,
}

// -------------------------------------------------
// Declarations
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Open {
    pub date: NaiveDate,
    pub account: AccountName,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Close {
    pub date: NaiveDate,
    pub account: AccountName,
    #[serde(default)]
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Commodity {
    pub date: NaiveDate,
    pub name: String,
    #[serde(default)]
    pub meta: Metadata,
}

// -------------------------------------------------
// Events
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Transaction {
    pub date: NaiveDate,
    #[serde(default)]
    pub effective_date: Option<NaiveDate>,

    pub status: TransactionStatus,
    #[serde(default)]
    pub payee: Option<String>,
    #[serde(default)]
    pub narration: Option<String>,

    #[serde(default)]
    pub postings: Vec<Posting>,

    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Posting {
    pub account: AccountName,
    #[serde(default)]
    pub amount: Option<Amount>,
    #[serde(default)]
    pub meta: Metadata,
}

// -------------------------------------------------
// Assertions
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Balance {
    pub date: NaiveDate,
    pub account: AccountName,
    pub amount: Amount,
    #[serde(default)]
    pub tolerance: Option<Decimal>,
    #[serde(default)]
    pub meta: Metadata,
}

// -------------------------------------------------
// Misc
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Amount {
    pub number: Decimal,
    pub commodity: CommodityName,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    Uncleared,
    Pending,
    Cleared,
}

pub type Metadata = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct AccountName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct EnvelopeName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct CommodityName(pub String);
