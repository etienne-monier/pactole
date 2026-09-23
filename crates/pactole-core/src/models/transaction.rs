use crate::models::balance::Amount;
use crate::models::names::{AccountName, Metadata};
use chrono::NaiveDate;

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

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Uncleared,
    Pending,
    Cleared,
}
