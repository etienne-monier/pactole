use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

// -------------------------------------------------
// Journal structure
// -------------------------------------------------

pub struct Journal {
    entries: Vec<Entry>,
}

enum Entry {
    Event(Event),
    Declaration(Declaration),
    Assertion(Assertion),
}

enum Declaration {
    Open(Open),
    Close(Close),
    Commodity(Commodity),
}

enum Event {
    Transaction(Transaction),
    BudgetAllocation,
}

enum Assertion {
    Balance(Balance),
    // Price,
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
    pub name: String,
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

pub type Metadata = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvelopeName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommodityName(pub String);
