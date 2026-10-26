use chrono::NaiveDate;
use pactole_core::{CommodityName, Entry, Journal, Report, Valuation, Valuator};

pub(crate) fn valuator(
    journal: &Journal,
    convert_to: Option<&str>,
    at: Option<NaiveDate>,
    default_date: NaiveDate,
) -> Result<Option<Valuator>, String> {
    let Some(currency) = convert_to else {
        return if at.is_some() {
            Err("--at requires --convert-to".to_string())
        } else {
            Ok(None)
        };
    };
    let currency = CommodityName::new(currency).map_err(|error| error.to_string())?;
    if !journal
        .entries()
        .iter()
        .any(|entry| matches!(entry, Entry::Commodity(declaration) if declaration.name == currency))
    {
        return Err(format!(
            "commodity {currency} is not declared in the journal"
        ));
    }
    Ok(Some(Valuator::new(
        journal,
        currency,
        at.unwrap_or(default_date),
    )))
}

pub(crate) fn value_report(
    report: &mut Report,
    valuator: &mut Option<Valuator>,
) -> Result<(), String> {
    if let Some(valuator) = valuator {
        report
            .value_in(valuator)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn render_valuation(valuation: &Valuation) -> String {
    let mut output = format!("Valuation: {} at {}\n", valuation.currency, valuation.date);
    for price in &valuation.prices {
        output.push_str(&format!(
            "Price: {} → {} = {} ({})\n",
            price.base, price.quote, price.rate, price.date
        ));
    }
    for commodity in &valuation.missing {
        output.push_str(&format!(
            "Missing price: {} → {}; amount remains in {}\n",
            commodity, valuation.currency, commodity
        ));
    }
    output
}
