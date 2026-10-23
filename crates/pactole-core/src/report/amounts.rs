use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::models::{Amount, CommodityName};

/// A set of amounts holding at most one value per commodity.
///
/// Commodities are kept separate: Pactole never adds EUR to USD. The entries
/// are sorted by commodity name, so report output is deterministic.
///
/// Zero values are kept until [`Amounts::retain_non_zero`] is called; report
/// builders use that to hide empty commodities from their output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Amounts {
    values: BTreeMap<CommodityName, Decimal>,
}

impl Amounts {
    /// Builds an empty set of amounts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an amount, accumulating it with the same commodity already
    /// present.
    pub fn add(&mut self, amount: &Amount) {
        *self.values.entry(amount.commodity.clone()).or_default() += amount.number;
    }

    /// Merges every amount of `other` into this set.
    pub fn merge(&mut self, other: &Self) {
        for (commodity, number) in &other.values {
            *self.values.entry(commodity.clone()).or_default() += *number;
        }
    }

    /// Negates every value, keeping the same commodities.
    ///
    /// Balance-sheet sections use this to present credit balances (liabilities
    /// and equity) as positive amounts.
    pub fn negate(&mut self) {
        for number in self.values.values_mut() {
            *number = -*number;
        }
    }

    /// Returns the value held for `commodity`, if any.
    pub fn get(&self, commodity: &CommodityName) -> Option<Decimal> {
        self.values.get(commodity).copied()
    }

    /// Iterates over the values in commodity order.
    pub fn iter(&self) -> impl Iterator<Item = (&CommodityName, Decimal)> + '_ {
        self.values
            .iter()
            .map(|(commodity, number)| (commodity, *number))
    }

    /// Returns every amount as a [`Amount`], in commodity order.
    ///
    /// Zero values are included; use [`Amounts::retain_non_zero`] first to
    /// drop them.
    pub fn to_vec(&self) -> Vec<Amount> {
        self.iter()
            .map(|(commodity, number)| Amount {
                number,
                commodity: commodity.clone(),
            })
            .collect()
    }

    /// Returns whether the set holds no commodity at all.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns whether every value is zero (an empty set counts as zero).
    pub fn is_zero(&self) -> bool {
        self.values.values().all(Decimal::is_zero)
    }

    /// Drops the commodities whose value is zero.
    pub fn retain_non_zero(&mut self) {
        self.values.retain(|_, number| !number.is_zero());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commodity(name: &str) -> CommodityName {
        CommodityName::new(name).unwrap()
    }

    fn amount(number: &str, commodity_name: &str) -> Amount {
        Amount {
            number: number.parse().unwrap(),
            commodity: commodity(commodity_name),
        }
    }

    #[test]
    fn accumulates_amounts_of_the_same_commodity() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("10.00", "EUR"));
        amounts.add(&amount("-2.50", "EUR"));

        assert_eq!(
            amounts.get(&commodity("EUR")),
            Some("7.50".parse().unwrap())
        );
    }

    #[test]
    fn keeps_commodities_separate() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("10.00", "EUR"));
        amounts.add(&amount("20.00", "USD"));

        assert_eq!(
            amounts.get(&commodity("EUR")),
            Some("10.00".parse().unwrap())
        );
        assert_eq!(
            amounts.get(&commodity("USD")),
            Some("20.00".parse().unwrap())
        );
    }

    #[test]
    fn orders_commodities_deterministically() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("1.00", "USD"));
        amounts.add(&amount("2.00", "EUR"));
        amounts.add(&amount("3.00", "GBP"));

        let commodities: Vec<_> = amounts
            .iter()
            .map(|(commodity, _)| commodity.as_str())
            .collect();

        assert_eq!(commodities, vec!["EUR", "GBP", "USD"]);
    }

    #[test]
    fn merge_sums_each_commodity() {
        let mut left = Amounts::new();
        left.add(&amount("1.00", "EUR"));
        let mut right = Amounts::new();
        right.add(&amount("2.00", "EUR"));
        right.add(&amount("3.00", "USD"));

        left.merge(&right);

        assert_eq!(left.get(&commodity("EUR")), Some("3.00".parse().unwrap()));
        assert_eq!(left.get(&commodity("USD")), Some("3.00".parse().unwrap()));
    }

    #[test]
    fn retain_non_zero_drops_zero_commodities() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("1.00", "EUR"));
        amounts.add(&amount("0.00", "USD"));

        amounts.retain_non_zero();

        assert_eq!(amounts.iter().count(), 1);
        assert_eq!(
            amounts.get(&commodity("EUR")),
            Some("1.00".parse().unwrap())
        );
    }

    #[test]
    fn an_empty_set_is_zero_but_not_non_empty() {
        let amounts = Amounts::new();

        assert!(amounts.is_empty());
        assert!(amounts.is_zero());
    }

    #[test]
    fn negate_flips_every_value_and_keeps_the_commodities() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("10.00", "EUR"));
        amounts.add(&amount("-2.50", "USD"));

        amounts.negate();

        assert_eq!(
            amounts.get(&commodity("EUR")),
            Some("-10.00".parse().unwrap())
        );
        assert_eq!(
            amounts.get(&commodity("USD")),
            Some("2.50".parse().unwrap())
        );
    }

    #[test]
    fn to_vec_preserves_the_commodity_of_each_amount() {
        let mut amounts = Amounts::new();
        amounts.add(&amount("1.00", "EUR"));
        amounts.add(&amount("2.00", "USD"));

        let amounts = amounts.to_vec();

        assert_eq!(amounts.len(), 2);
        assert_eq!(amounts[0].commodity.as_str(), "EUR");
        assert_eq!(amounts[1].commodity.as_str(), "USD");
    }
}
