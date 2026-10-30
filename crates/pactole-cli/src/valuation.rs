use pactole_core::Valuation;

/// Renders valuation provenance and missing price information.
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
