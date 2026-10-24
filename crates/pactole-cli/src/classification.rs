use pactole_config::ReportOptions;
use pactole_core::AccountClassification;

/// Turns the `[reports]` configuration into a core classification, keeping the
/// default roots for the categories the configuration does not override.
pub(crate) fn classification(options: &ReportOptions) -> AccountClassification {
    let mut classification = AccountClassification::default();

    if let Some(roots) = &options.assets {
        classification = classification.with_assets(roots.clone());
    }
    if let Some(roots) = &options.liabilities {
        classification = classification.with_liabilities(roots.clone());
    }
    if let Some(roots) = &options.equity {
        classification = classification.with_equity(roots.clone());
    }
    if let Some(roots) = &options.income {
        classification = classification.with_income(roots.clone());
    }
    if let Some(roots) = &options.expenses {
        classification = classification.with_expenses(roots.clone());
    }

    classification
}
