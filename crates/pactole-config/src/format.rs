use std::collections::BTreeMap;

/// Formatter options (the `[format]` section of `pactole.toml`).
///
/// These used to live in `pactole-storage-fs`; they moved here so that a
/// single crate owns the whole `pactole.toml` schema. `pactole-storage-fs`
/// re-exports this type to keep its public API stable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatOptions {
    /// Column at which posting amounts are aligned.
    pub posting_width: usize,
    /// Number of decimal places used for currencies not listed explicitly.
    pub default_decimal_places: u32,
    /// Currency-specific number of decimal places.
    pub currency_decimal_places: BTreeMap<String, u32>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            posting_width: 50,
            default_decimal_places: 2,
            currency_decimal_places: BTreeMap::new(),
        }
    }
}

impl FormatOptions {
    /// Returns the configured number of decimal places for a commodity.
    pub fn decimal_places(&self, commodity: &str) -> u32 {
        self.currency_decimal_places
            .get(commodity)
            .copied()
            .unwrap_or(self.default_decimal_places)
    }
}
