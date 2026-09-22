/// Represents currencies or assets.
use std::collections::BTreeMap;

use crate::{PactoleCoreError, PactoleCoreResult};

use once_cell::sync::Lazy;
use regex::Regex;

/* ---------------------------------------------
 * The commodity code
 * ---------------------------------------------*/

static COMMODITY_CODE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9._-]{0,31}$").expect("valid regex"));

/// Commodity identifier is its code (ISO-like or domain-specific): "EUR", "USD", "BTC", "kg".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct CommodityCode(pub String);

impl CommodityCode {
    pub fn new(code: String) -> PactoleCoreResult<Self> {
        if !COMMODITY_CODE_RE.is_match(code.as_str()) {
            return Err(PactoleCoreError::InvalidCommodityCode(code));
        }
        Ok(Self(code.into()))
    }
}

/* ---------------------------------------------
 * The currency rendering specification
 * ---------------------------------------------*/

/// Symbol position relative to amount
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolPosition {
    Left,
    Right,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commodity {
    /// Commodity identifier is its code (ISO-like or domain-specific): "EUR", "USD", "BTC", "kg".
    pub code: CommodityCode,

    /// Display symbol used when rendering (may be "$", "€", "BTC", "kg").
    pub symbol: Option<String>,

    /// Symbol position relative to amount
    pub symbol_position: SymbolPosition,

    /// Human name (optional): "US Dollar", "Euro".
    pub name: Option<String>,

    /// Alternative spellings/tickers: ["US$", "$US", "dollar"].
    pub aliases: Vec<String>,

    /// Default scale for arithmetic & display (money often 2).
    pub precision: u8,

    /// Extra metadata for UI and domain needs (issuer, ISO numeric, etc.).
    pub metadata: BTreeMap<String, String>,
}

impl Commodity {
    pub fn new(
        code: impl Into<String>,
        symbol: Option<String>,
        symbol_position: Option<SymbolPosition>,
        name: Option<String>,
        aliases: Vec<String>,
        precision: u8,
    ) -> PactoleCoreResult<Self> {
        Ok(Self {
            code: CommodityCode::new(code.into())?,
            symbol: symbol,
            symbol_position: symbol_position.unwrap_or(SymbolPosition::Right),
            name: name,
            aliases: aliases,
            precision,
            metadata: BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commodity_code() {
        assert_eq!(
            CommodityCode::new(String::from("EUR")),
            CommodityCode::new(String::from("EUR"))
        );
        assert_ne!(
            CommodityCode::new(String::from("EUR")),
            CommodityCode::new(String::from("USD"))
        );

        // Invalid code
        assert_eq!(
            CommodityCode::new(String::new()),
            PactoleCoreError::InvalidCommodityCode
        )
    }

    #[test]
    fn test_commodity_creation() {
        let commodity = Commodity::new(
            "EUR",
            Some(String::from("€")),
            None,
            Some("EURO".to_string()),
            vec![],
            2,
        )
        .unwrap();

        assert_eq!(commodity.code, CommodityCode::new("EUR"));
        assert_eq!(commodity.symbol_position, SymbolPosition::Right);
    }

    #[test]
    fn test_commodity_creation_no_symbol() {
        let commodity = Commodity::new("EUR", None, None, None, vec![], 2);

        assert_eq!(commodity.code, CommodityCode::new("EUR"));
    }

    #[test]
    fn test_unvalid_code() {}
}
