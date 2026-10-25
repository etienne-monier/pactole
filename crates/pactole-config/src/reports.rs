/// Account classification used by the reports (the `[reports]` section of
/// `pactole.toml`).
///
/// Each list holds account *root* names: the first `:`-separated segment of an
/// account. The balance sheet matches roots exactly and case-sensitively, and
/// a root matching no list lands in an `Unclassified` section instead of being
/// dropped.
///
/// A missing list keeps the default English convention (`Assets`,
/// `Liabilities`, `Equity`, `Income`, `Expenses`), so a journal written in
/// another language only has to configure the roots it actually uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportOptions {
    /// Roots of the asset accounts.
    pub assets: Option<Vec<String>>,
    /// Roots of the liability accounts.
    pub liabilities: Option<Vec<String>>,
    /// Roots of the equity accounts.
    pub equity: Option<Vec<String>>,
    /// Roots of the income accounts.
    pub income: Option<Vec<String>>,
    /// Roots of the expense accounts.
    pub expenses: Option<Vec<String>>,
    /// Accounts considered uncategorized (exact or by prefix). When omitted,
    /// the CLI falls back to `[import] default_counter_account` if present.
    pub uncategorized: Option<Vec<String>>,
}

impl ReportOptions {
    /// Returns whether no classification root is configured at all.
    pub fn is_empty(&self) -> bool {
        self.assets.is_none()
            && self.liabilities.is_none()
            && self.equity.is_none()
            && self.income.is_none()
            && self.expenses.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::ReportOptions;

    #[test]
    fn default_is_empty() {
        let options = ReportOptions::default();

        assert!(options.is_empty());
        assert_eq!(options.assets, None);
        assert_eq!(options.liabilities, None);
        assert_eq!(options.equity, None);
        assert_eq!(options.income, None);
        assert_eq!(options.expenses, None);
    }

    #[test]
    fn any_configured_root_list_is_not_empty() {
        let options = ReportOptions {
            assets: Some(vec!["Actifs".to_string()]),
            ..ReportOptions::default()
        };

        assert!(!options.is_empty());
    }
}
