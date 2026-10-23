//! Render-agnostic reporting primitives.
//!
//! The types in this module describe *what* a report contains, never how it
//! looks. [`Report`] and its [`ReportSection`]s are plain data; the text, JSON
//! and CSV representations are produced by [`Report::render`] from the same
//! data.
//!
//! A report is made of named sections (the balance sheet uses *Assets*,
//! *Liabilities* and *Equity*) or of a single anonymous section (the register
//! and the spending breakdowns). The balance sheet is built by
//! [`balance_sheet`] from a [`Journal`](crate::Journal); every future report
//! builds an [`AccountBalances`], groups it with [`AccountGrouping`], and
//! wraps the result in a [`Report`] carrying an inclusive [`ReportScope`].

mod accounts;
mod amounts;
mod balance_sheet;
mod error;
mod period;
mod render;

pub use accounts::{AccountBalances, AccountGrouping};
pub use amounts::Amounts;
pub use balance_sheet::{AccountClassification, balance_sheet};
pub use error::ReportError;
pub use period::{Period, ReferenceDate, ReportScope};

/// One line of a report.
///
/// The label is deliberately generic: it is an account for the balance sheet
/// and the income statement, but it can also be a payee, a tag or a month for
/// expense breakdowns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportLine {
    /// Text displayed for this line.
    pub label: String,
    /// Hierarchical indentation, `0` being the top level.
    pub depth: usize,
    /// Amounts of the line, at most one per commodity.
    pub amounts: Amounts,
}

impl ReportLine {
    /// Builds a report line.
    ///
    /// Zero amounts are removed from `amounts`, so an empty [`Amounts`] means
    /// that the line carries no value (an ancestor whose subtree nets to
    /// zero, for instance).
    pub fn new(label: impl Into<String>, depth: usize, mut amounts: Amounts) -> Self {
        amounts.retain_non_zero();
        Self {
            label: label.into(),
            depth,
            amounts,
        }
    }
}

/// One named section of a report.
///
/// The balance sheet uses one section per accounting category, each carrying
/// its own subtotal. A report built from a single anonymous section (see
/// [`ReportSection::anonymous`]) renders exactly like a flat report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSection {
    /// Section heading, or `None` for an anonymous section.
    pub title: Option<String>,
    /// Lines of the section, in display order.
    pub lines: Vec<ReportLine>,
    /// Subtotal of the section, at most one value per commodity.
    pub subtotal: Amounts,
}

impl ReportSection {
    /// Builds a named section, removing its zero subtotals.
    pub fn new(title: impl Into<String>, lines: Vec<ReportLine>, mut subtotal: Amounts) -> Self {
        subtotal.retain_non_zero();
        Self {
            title: Some(title.into()),
            lines,
            subtotal,
        }
    }

    /// Builds an anonymous section, used by the flat reports.
    ///
    /// Zero subtotals are removed, like for a named section.
    pub fn anonymous(lines: Vec<ReportLine>, mut subtotal: Amounts) -> Self {
        subtotal.retain_non_zero();
        Self {
            title: None,
            lines,
            subtotal,
        }
    }
}

/// A complete report result, independent of any rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Human-readable report title.
    pub title: String,
    /// Date span the report covers.
    pub scope: ReportScope,
    /// Sections in display order.
    pub sections: Vec<ReportSection>,
    /// Total per commodity, independent of the line grouping.
    ///
    /// For a balance sheet this is the algebraic sum of the exact ledger
    /// balances, so it is zero for a balanced, fully classified journal. It is
    /// not the sum of the displayed lines, which carry presentation signs.
    pub totals: Amounts,
}

impl Report {
    /// Builds a flat report made of a single anonymous section.
    ///
    /// Zero totals are removed, so a report whose movements cancel out has no
    /// total line.
    pub fn new(
        title: impl Into<String>,
        scope: ReportScope,
        lines: Vec<ReportLine>,
        totals: Amounts,
    ) -> Self {
        Self::from_sections(
            title,
            scope,
            vec![ReportSection::anonymous(lines, totals.clone())],
            totals,
        )
    }

    /// Builds a report from already computed sections and totals.
    ///
    /// Zero totals are removed.
    pub fn from_sections(
        title: impl Into<String>,
        scope: ReportScope,
        sections: Vec<ReportSection>,
        mut totals: Amounts,
    ) -> Self {
        totals.retain_non_zero();
        Self {
            title: title.into(),
            scope,
            sections,
            totals,
        }
    }

    /// Builds a report from account balances and a grouping.
    ///
    /// The totals are computed from the exact balances, not from the
    /// displayed lines: in [`AccountGrouping::Full`] mode the lines are
    /// cumulative and summing them would double-count amounts.
    pub fn from_balances(
        title: impl Into<String>,
        scope: ReportScope,
        balances: &AccountBalances,
        grouping: AccountGrouping,
    ) -> Self {
        Self::new(title, scope, balances.lines(grouping), balances.totals())
    }

    /// Iterates over the lines of every section, in order.
    pub fn lines(&self) -> impl Iterator<Item = &ReportLine> {
        self.sections
            .iter()
            .flat_map(|section| section.lines.iter())
    }

    /// Renders this report in `format`.
    ///
    /// The returned string always ends with a newline.
    pub fn render(&self, format: ReportFormat) -> String {
        render::render(self, format)
    }
}

/// The supported report renderings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFormat {
    /// Human-readable, aligned text.
    Text,
    /// Machine-readable JSON with decimal amounts as strings.
    Json,
    /// Tidy CSV with one row per commodity.
    Csv,
}

impl ReportFormat {
    /// Every supported format.
    pub const ALL: [Self; 3] = [Self::Text, Self::Json, Self::Csv];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountName, Amount, CommodityName};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn account(name: &str) -> AccountName {
        AccountName::new(name).unwrap()
    }

    fn amount(number: &str, commodity: &str) -> Amount {
        Amount {
            number: number.parse::<Decimal>().unwrap(),
            commodity: CommodityName::new(commodity).unwrap(),
        }
    }

    #[test]
    fn from_balances_builds_lines_and_totals() {
        let mut balances = AccountBalances::new();
        balances.add(&account("Actifs:Banque"), &amount("10.00", "EUR"));
        balances.add(&account("Depenses:Cafe"), &amount("-4.00", "EUR"));

        let report = Report::from_balances(
            "Balance",
            ReportScope::as_of(date(2024, 1, 31)),
            &balances,
            AccountGrouping::Exact,
        );

        assert_eq!(report.title, "Balance");
        assert_eq!(report.scope, ReportScope::as_of(date(2024, 1, 31)));
        assert_eq!(report.lines().count(), 2);
        assert_eq!(report.sections.len(), 1);
        assert_eq!(report.sections[0].title, None);
        assert_eq!(
            report
                .totals
                .get(&CommodityName::new("EUR").unwrap())
                .unwrap(),
            "6.00".parse().unwrap()
        );
    }

    #[test]
    fn from_balances_removes_zero_totals() {
        let mut balances = AccountBalances::new();
        balances.add(&account("Actifs:Banque"), &amount("10.00", "EUR"));
        balances.add(&account("Depenses:Cafe"), &amount("-10.00", "EUR"));

        let report = Report::from_balances(
            "Balance",
            ReportScope::as_of(date(2024, 1, 31)),
            &balances,
            AccountGrouping::Exact,
        );

        assert!(report.totals.is_empty());
        assert!(report.sections[0].subtotal.is_empty());
    }

    #[test]
    fn new_removes_zero_totals() {
        let mut totals = Amounts::new();
        amounts_with_zero(&mut totals);

        let report = Report::new(
            "Empty",
            ReportScope::as_of(date(2024, 1, 31)),
            Vec::new(),
            totals,
        );

        assert!(report.totals.is_empty());
    }

    #[test]
    fn sections_keep_their_own_subtotals() {
        let report = Report::from_sections(
            "Balance sheet",
            ReportScope::as_of(date(2024, 1, 31)),
            vec![
                ReportSection::new(
                    "Assets",
                    vec![ReportLine::new("Actifs:Banque", 0, {
                        let mut amounts = Amounts::new();
                        amounts.add(&amount("10.00", "EUR"));
                        amounts
                    })],
                    {
                        let mut amounts = Amounts::new();
                        amounts.add(&amount("10.00", "EUR"));
                        amounts
                    },
                ),
                ReportSection::new("Liabilities", Vec::new(), Amounts::new()),
            ],
            Amounts::new(),
        );

        assert_eq!(report.sections.len(), 2);
        assert_eq!(report.sections[0].title.as_deref(), Some("Assets"));
        assert_eq!(report.sections[1].title.as_deref(), Some("Liabilities"));
        assert!(report.sections[1].subtotal.is_empty());
    }

    fn amounts_with_zero(amounts: &mut Amounts) {
        amounts.add(&amount("1.00", "EUR"));
        amounts.add(&amount("-1.00", "EUR"));
    }
}
