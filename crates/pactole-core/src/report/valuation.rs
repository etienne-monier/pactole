use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::models::{Amount, CommodityName, Entry, Journal};

use super::{Amounts, ReportError};

/// A direct market quote used to value a report amount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuationPrice {
    /// Commodity being converted.
    pub base: CommodityName,
    /// Target commodity of the quote.
    pub quote: CommodityName,
    /// Number of quote units per base unit.
    pub rate: Decimal,
    /// Date of the source price directive.
    pub date: NaiveDate,
}

/// Provenance and incomplete conversions for a valued report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valuation {
    /// Requested report commodity.
    pub currency: CommodityName,
    /// Date at which prices were selected.
    pub date: NaiveDate,
    /// Distinct direct prices used, sorted by base commodity.
    pub prices: Vec<ValuationPrice>,
    /// Commodities that had no direct price to `currency` by `date`.
    pub missing: Vec<CommodityName>,
}

/// Applies a journal's direct market prices to report amounts.
///
/// For each source commodity, the latest quote directly to the requested target
/// on or before `date` is used. Inverse and multi-step conversions are not
/// performed. Amounts with no applicable quote remain in their source
/// commodity and are listed in [`Valuation::missing`].
#[derive(Debug, Clone)]
pub struct Valuator {
    /// Commodity into which amounts are converted.
    currency: CommodityName,
    /// Latest date eligible for price selection.
    date: NaiveDate,
    /// Latest direct price per source commodity.
    rates: BTreeMap<CommodityName, ValuationPrice>,
    /// Commodities converted during the current scope.
    used: BTreeSet<CommodityName>,
    /// Commodities without a usable direct price.
    missing: BTreeSet<CommodityName>,
}

impl Valuator {
    /// Creates a valuation context from the journal's market-price directives.
    pub fn new(journal: &Journal, currency: CommodityName, date: NaiveDate) -> Self {
        let mut rates = BTreeMap::new();
        for entry in journal.entries() {
            let Entry::Price(price) = entry else {
                continue;
            };
            if price.date > date || price.quote.commodity != currency {
                continue;
            }
            let candidate = ValuationPrice {
                base: price.base.clone(),
                quote: price.quote.commodity.clone(),
                rate: price.quote.number,
                date: price.date,
            };
            let replace = rates
                .get(&price.base)
                .is_none_or(|current: &ValuationPrice| current.date < candidate.date);
            if replace {
                rates.insert(price.base.clone(), candidate);
            }
        }

        Self {
            currency,
            date,
            rates,
            used: BTreeSet::new(),
            missing: BTreeSet::new(),
        }
    }

    /// Converts all non-zero amounts, retaining commodities with no quote.
    pub fn convert_amounts(&mut self, amounts: &Amounts) -> Result<Amounts, ReportError> {
        let mut converted = Amounts::new();
        for (commodity, number) in amounts.iter() {
            if number.is_zero() {
                continue;
            }
            let (target, converted_number) = if commodity == &self.currency {
                (self.currency.clone(), number)
            } else if let Some(price) = self.rates.get(commodity) {
                self.used.insert(commodity.clone());
                let Some(value) =
                    crate::validation::checked_exact_decimal_product(number, price.rate)
                else {
                    return Err(ReportError::ValuationOverflow {
                        from_commodity: commodity.clone(),
                        target: self.currency.clone(),
                    });
                };
                (self.currency.clone(), value)
            } else {
                self.missing.insert(commodity.clone());
                (commodity.clone(), number)
            };
            let amount = Amount {
                number: converted_number,
                commodity: target,
            };
            if !converted.add_checked(&amount) {
                return Err(ReportError::ValuationOverflow {
                    from_commodity: commodity.clone(),
                    target: self.currency.clone(),
                });
            }
        }
        converted.retain_non_zero();
        Ok(converted)
    }

    /// Returns the price provenance and missing conversions encountered so far.
    pub fn valuation(&self) -> Valuation {
        Valuation {
            currency: self.currency.clone(),
            date: self.date,
            prices: self
                .used
                .iter()
                .filter_map(|commodity| self.rates.get(commodity).cloned())
                .collect(),
            missing: self.missing.iter().cloned().collect(),
        }
    }

    /// Creates an empty provenance scope sharing the same conversion rates.
    pub(super) fn fresh_scope(&self) -> Self {
        let mut scope = self.clone();
        scope.used.clear();
        scope.missing.clear();
        scope
    }

    /// Merges the provenance collected by a nested valuation scope.
    pub(super) fn record_scope(&mut self, scope: &Self) {
        self.used.extend(scope.used.iter().cloned());
        self.missing.extend(scope.missing.iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Metadata, Price};
    use crate::report::{Period, Report, ReportLine, ReportScope, ReportSection};
    use rust_decimal::Decimal;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }

    fn commodity(name: &str) -> CommodityName {
        CommodityName::new(name).unwrap()
    }

    fn price(day: u32, base: &str, rate: &str, quote: &str) -> Entry {
        Entry::Price(Price {
            date: date(day),
            base: commodity(base),
            quote: Amount {
                number: rate.parse::<Decimal>().unwrap(),
                commodity: commodity(quote),
            },
            meta: Metadata::default(),
        })
    }

    fn amounts(values: &[(&str, &str)]) -> Amounts {
        let mut result = Amounts::new();
        for (number, currency) in values {
            result.add(&Amount {
                number: number.parse().unwrap(),
                commodity: commodity(currency),
            });
        }
        result
    }

    #[test]
    fn uses_latest_direct_price_on_or_before_valuation_date() {
        let journal = Journal {
            entries: vec![
                price(1, "USD", "0.9", "EUR"),
                price(5, "USD", "0.92", "EUR"),
                price(8, "USD", "0.95", "EUR"),
            ],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));

        let converted = valuator
            .convert_amounts(&amounts(&[("10", "USD")]))
            .unwrap();

        assert_eq!(
            converted.get(&commodity("EUR")),
            Some("9.2".parse().unwrap())
        );
        assert_eq!(valuator.valuation().prices[0].date, date(5));
    }

    #[test]
    fn does_not_use_inverse_or_multistep_prices_and_retains_missing_amounts() {
        let journal = Journal {
            entries: vec![
                price(1, "EUR", "1.1", "USD"),
                price(1, "GBP", "1.2", "USD"),
                price(1, "USD", "0.9", "CAD"),
            ],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));

        let converted = valuator
            .convert_amounts(&amounts(&[("10", "USD"), ("3", "GBP")]))
            .unwrap();

        assert_eq!(converted.get(&commodity("USD")), Some(Decimal::from(10)));
        assert_eq!(converted.get(&commodity("GBP")), Some(Decimal::from(3)));
        assert_eq!(
            valuator.valuation().missing,
            vec![commodity("GBP"), commodity("USD")]
        );
    }

    #[test]
    fn does_not_require_a_quote_for_the_target_currency() {
        let journal = Journal {
            entries: Vec::new(),
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));

        let converted = valuator.convert_amounts(&amounts(&[("4", "EUR")])).unwrap();

        assert_eq!(converted.get(&commodity("EUR")), Some(Decimal::from(4)));
        assert!(valuator.valuation().missing.is_empty());
    }

    #[test]
    fn values_report_lines_subtotals_and_totals_and_records_provenance() {
        let journal = Journal {
            entries: vec![price(1, "USD", "0.9", "EUR")],
        };
        let mut report = Report::from_sections(
            "Report",
            ReportScope::Over(Period::single(date(6))),
            vec![ReportSection::new(
                "Expenses",
                vec![ReportLine::new(
                    "Food",
                    0,
                    amounts(&[("10", "USD"), ("2", "GBP")]),
                )],
                amounts(&[("10", "USD"), ("2", "GBP")]),
            )],
            amounts(&[("10", "USD"), ("2", "GBP")]),
        );
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));

        report.value_in(&mut valuator).unwrap();

        assert_eq!(
            report.sections[0].lines[0].amounts.get(&commodity("EUR")),
            Some("9".parse().unwrap())
        );
        assert_eq!(
            report.sections[0].subtotal.get(&commodity("EUR")),
            Some("9".parse().unwrap())
        );
        assert_eq!(
            report.totals.get(&commodity("EUR")),
            Some("9".parse().unwrap())
        );
        assert_eq!(report.totals.get(&commodity("GBP")), Some(Decimal::from(2)));
        let valuation = report.valuation.as_ref().unwrap();
        assert_eq!(valuation.prices[0].rate, "0.9".parse().unwrap());
        assert_eq!(valuation.missing, vec![commodity("GBP")]);
        assert!(
            report
                .render(crate::report::ReportFormat::Text)
                .contains("Missing price: GBP → EUR")
        );
        let json: serde_json::Value =
            serde_json::from_str(&report.render(crate::report::ReportFormat::Json)).unwrap();
        assert_eq!(json["valuation"]["prices"][0]["date"], "2026-01-01");
        assert_eq!(json["valuation"]["missing"][0], "GBP");
        let csv = report.render(crate::report::ReportFormat::Csv);
        assert!(csv.contains(",price,USD → EUR at 2026-01-01,"));
        assert!(csv.contains(",missing_price,GBP → EUR; retained as GBP,"));
    }

    #[test]
    fn reports_overflow_instead_of_wrapping_or_panicking() {
        let journal = Journal {
            entries: vec![price(1, "USD", "2", "EUR")],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));
        let amounts = amounts(&[("79228162514264337593543950335", "USD")]);

        assert_eq!(
            valuator.convert_amounts(&amounts),
            Err(ReportError::ValuationOverflow {
                from_commodity: commodity("USD"),
                target: commodity("EUR"),
            })
        );
    }

    #[test]
    fn rejects_inexact_decimal_conversion_instead_of_rounding() {
        let journal = Journal {
            entries: vec![price(1, "USD", "0.2", "EUR")],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));
        let amounts = amounts(&[("0.0000000000000000000000000001", "USD")]);

        assert!(matches!(
            valuator.convert_amounts(&amounts),
            Err(ReportError::ValuationOverflow { .. })
        ));
    }

    #[test]
    fn rejects_inexact_sum_of_converted_commodities() {
        let journal = Journal {
            entries: vec![price(1, "USD", "1", "EUR"), price(1, "GBP", "1", "EUR")],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));
        let amounts = amounts(&[("79228162514264337593543950335", "USD"), ("0.1", "GBP")]);

        assert!(matches!(
            valuator.convert_amounts(&amounts),
            Err(ReportError::ValuationOverflow { .. })
        ));
    }

    #[test]
    fn report_valuation_metadata_is_scoped_per_report() {
        let journal = Journal {
            entries: vec![price(1, "USD", "0.9", "EUR"), price(1, "GBP", "1.1", "EUR")],
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));
        let mut first = Report::new(
            "First",
            ReportScope::Over(Period::single(date(6))),
            Vec::new(),
            amounts(&[("2", "USD")]),
        );
        let mut second = Report::new(
            "Second",
            ReportScope::Over(Period::single(date(6))),
            Vec::new(),
            amounts(&[("3", "GBP")]),
        );

        first.value_in(&mut valuator).unwrap();
        second.value_in(&mut valuator).unwrap();

        assert_eq!(first.valuation.unwrap().prices[0].base, commodity("USD"));
        assert_eq!(second.valuation.unwrap().prices[0].base, commodity("GBP"));
        assert_eq!(valuator.valuation().prices.len(), 2);
    }

    #[test]
    fn report_cannot_be_valued_twice() {
        let journal = Journal {
            entries: Vec::new(),
        };
        let mut valuator = Valuator::new(&journal, commodity("EUR"), date(6));
        let mut report = Report::new(
            "Report",
            ReportScope::Over(Period::single(date(6))),
            Vec::new(),
            amounts(&[("2", "EUR")]),
        );

        report.value_in(&mut valuator).unwrap();

        assert_eq!(
            report.value_in(&mut valuator),
            Err(ReportError::AlreadyValued)
        );
    }
}
