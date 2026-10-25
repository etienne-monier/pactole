use std::collections::BTreeMap;

use crate::models::{Amount, Entry, Journal};

use super::balance_sheet::{Category, root};
use super::{AccountClassification, Period, ReportLine};

/// Transaction metadata key used to break reports down.
#[derive(Debug, Clone, Copy)]
pub(super) enum TransactionGrouping {
    Payee,
    Tag,
}

/// Visits each in-period income or expense posting with its report labels.
pub(super) fn for_each_grouped_posting(
    journal: &Journal,
    period: Period,
    classification: &AccountClassification,
    grouping: TransactionGrouping,
    mut visit: impl FnMut(Category, &[String], &Amount),
) {
    for entry in journal.entries() {
        let Entry::Transaction(transaction) = entry else {
            continue;
        };
        if !period.contains(transaction.date) {
            continue;
        }

        let labels = match grouping {
            TransactionGrouping::Payee => vec![
                transaction
                    .payee
                    .clone()
                    .unwrap_or_else(|| "(no payee)".to_string()),
            ],
            TransactionGrouping::Tag if transaction.tags.is_empty() => {
                vec!["(untagged)".to_string()]
            }
            TransactionGrouping::Tag => transaction.tags.clone(),
        };

        for posting in &transaction.postings {
            let Some(amount) = &posting.amount else {
                continue;
            };
            let category = classification.category(root(&posting.account));
            if matches!(category, Category::Income | Category::Expenses) {
                visit(category, &labels, amount);
            }
        }
    }
}

/// Sorts keyed report lines, placing placeholder labels after named values.
pub(super) fn sort_keyed(lines: &mut [ReportLine]) {
    lines.sort_by(|left, right| {
        let left_key = left.label.as_str();
        let right_key = right.label.as_str();
        let left_placeholder = left_key.starts_with('(');
        let right_placeholder = right_key.starts_with('(');
        left_placeholder
            .cmp(&right_placeholder)
            .then(left_key.cmp(right_key))
    });
}

/// Collects a keyed map into report lines, dropping keys with zero totals.
pub(super) fn keyed_lines(lines: BTreeMap<String, super::Amounts>) -> Vec<ReportLine> {
    let mut report_lines: Vec<ReportLine> = lines
        .into_iter()
        .filter_map(|(label, amounts)| {
            if amounts.is_zero() {
                None
            } else {
                Some(ReportLine::new(label, 0, amounts))
            }
        })
        .collect();
    sort_keyed(&mut report_lines);
    report_lines
}
