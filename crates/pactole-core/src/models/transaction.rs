use crate::errors::ModelError;
use crate::models::balance::Amount;
use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Uncleared,
    Pending,
    Cleared,
}

#[cfg(test)]
mod tests {
    use super::*;

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
