//! One-shot migration of an Actual Budget SQLite database.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use chrono::NaiveDate;
use pactole_core::{
    AccountName, Amount, CommodityName, Metadata, MetadataKey, Posting, Transaction,
    TransactionStatus,
};
use rusqlite::{Connection, OpenFlags};
use rust_decimal::Decimal;

use crate::ImportError;

/// The complete result of an Actual Budget migration.
#[derive(Debug)]
pub struct ActualMigration {
    /// Pactole account names used by the generated postings.
    pub accounts: BTreeSet<AccountName>,
    /// Payees declared by the generated journal.
    pub payees: BTreeSet<String>,
    /// Generated transactions, including the opening transaction.
    pub transactions: Vec<Transaction>,
}

/// Imports an Actual Budget database as a new Pactole journal.
///
/// This deliberately has no deduplication or append mode: it is intended for a
/// one-time migration. Amounts are read as euros (the unit used by the
/// migration command), and the caller supplies the commodity and opening
/// equity account.
pub fn migrate_actual_database(
    path: &Path,
    currency: &str,
    opening_equity: &str,
    opening_date: NaiveDate,
    payee_metadata_key: Option<&str>,
) -> Result<ActualMigration, ImportError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let commodity = CommodityName::new(currency.to_string()).map_err(|source| {
        ImportError::InvalidCommodity {
            commodity: currency.to_string(),
            source,
        }
    })?;
    let equity = account_name(opening_equity)?;
    let payee_metadata_key = payee_metadata_key
        .map(|key| MetadataKey::new(key.to_string()))
        .transpose()?;

    let payees = load_payees(&conn)?;
    let accounts = load_accounts(&conn)?;
    let categories = load_categories(&conn)?;
    let rows = load_transactions(&conn)?;
    let rules = load_rules(&conn)?;

    let mut used_accounts = BTreeSet::new();
    used_accounts.insert(equity.clone());
    let mut declared_payees = BTreeSet::new();
    let mut transactions = Vec::new();
    let mut opening_postings = Vec::new();
    let mut handled = HashSet::new();

    // Actual stores transfers as two linked rows. Emit one double-entry and
    // mark both rows handled, rather than importing the movement twice.
    let by_id: HashMap<_, _> = rows.iter().map(|row| (row.id.as_str(), row)).collect();

    for row in &rows {
        if row.tombstone != 0 || !handled.insert(row.id.clone()) {
            continue;
        }
        let date = parse_date(row.date)?;
        let source_payee = resolve_payee(row, &payees);
        let payee = apply_payee_rules(&rules, row, &payees).or_else(|| source_payee.clone());
        if let Some(name) = &payee {
            declared_payees.insert(name.clone());
        }

        if row.starting_balance != 0 {
            let account = resolve_account(&accounts, &row.account)?;
            used_accounts.insert(account.clone());
            opening_postings.push(posting(account, amount(row.amount), &commodity));
            continue;
        }

        if let Some(target_id) = &row.transferred_id
            && let Some(target) = by_id.get(target_id.as_str())
        {
            handled.insert(target.id.clone());
            let left = resolve_account(&accounts, &row.account)?;
            let right = resolve_account(&accounts, &target.account)?;
            used_accounts.insert(left.clone());
            used_accounts.insert(right.clone());
            let left_amount = amount(row.amount);
            let right_amount = amount(target.amount);
            let right_amount = if left_amount + right_amount == Decimal::ZERO {
                right_amount
            } else {
                -left_amount
            };
            transactions.push(transaction(
                date,
                "Transfer".to_string(),
                Some("Actual Budget transfer".to_string()),
                vec![
                    posting(left, left_amount, &commodity),
                    posting(right, right_amount, &commodity),
                ],
            ));
            continue;
        }

        let account = resolve_account(&accounts, &row.account)?;
        used_accounts.insert(account.clone());
        let children: Vec<_> = rows
            .iter()
            .filter(|child| {
                child.parent_id.as_deref() == Some(row.id.as_str()) && child.tombstone == 0
            })
            .collect();

        if row.is_parent != 0 && !children.is_empty() {
            let mut postings = Vec::with_capacity(children.len() + 1);
            let mut total = Decimal::ZERO;
            for child in children {
                handled.insert(child.id.clone());
                let category = resolve_category(&categories, child.category.as_deref());
                let category = account_name(&category)?;
                let child_amount = amount(child.amount);
                total += child_amount;
                used_accounts.insert(category.clone());
                postings.push(posting(category, -child_amount, &commodity));
            }
            postings.push(posting(account, total, &commodity));
            let tx = transaction(
                date,
                payee.unwrap_or_else(|| "Unknown".to_string()),
                Some(row.imported_description.clone()),
                postings,
            );
            transactions.push(with_payee_metadata(
                tx,
                source_payee.as_deref(),
                payee_metadata_key.as_ref(),
            ));
            continue;
        }

        let category = apply_rules(&rules, row, &categories, &payees)
            .unwrap_or_else(|| resolve_category(&categories, row.category.as_deref()));
        let category = account_name(&category)?;
        used_accounts.insert(category.clone());
        let value = amount(row.amount);
        let tx = transaction(
            date,
            payee.unwrap_or_else(|| "Unknown".to_string()),
            Some(row.imported_description.clone()),
            vec![
                posting(account, value, &commodity),
                posting(category, -value, &commodity),
            ],
        );
        transactions.push(with_payee_metadata(
            tx,
            source_payee.as_deref(),
            payee_metadata_key.as_ref(),
        ));
    }

    if !opening_postings.is_empty() {
        let total = opening_postings
            .iter()
            .filter_map(|p| p.amount.as_ref())
            .map(|a| a.number)
            .sum::<Decimal>();
        opening_postings.push(posting(equity, -total, &commodity));
        transactions.insert(
            0,
            transaction(
                opening_date,
                "Opening".to_string(),
                Some("Opening balances".to_string()),
                opening_postings,
            ),
        );
    }

    for transaction in &transactions {
        if let Some(payee) = &transaction.payee {
            declared_payees.insert(payee.clone());
        }
    }
    Ok(ActualMigration {
        accounts: used_accounts,
        payees: declared_payees,
        transactions,
    })
}

#[derive(Debug)]
/// A row loaded from Actual Budget's transactions table.
struct Row {
    /// Actual transaction identifier.
    id: String,
    /// Actual account identifier.
    account: String,
    /// Actual category identifier, when assigned.
    category: Option<String>,
    /// Signed amount in Actual minor units.
    amount: i64,
    /// Date encoded as `YYYYMMDD`.
    date: i64,
    /// Payee identifier stored by Actual.
    description: Option<String>,
    /// Imported transaction description.
    imported_description: String,
    /// Linked transfer transaction identifier.
    transferred_id: Option<String>,
    /// Parent transaction identifier for split transactions.
    parent_id: Option<String>,
    /// Whether this row owns child split rows.
    is_parent: i64,
    /// Whether this row is an opening-balance row.
    starting_balance: i64,
    /// Whether Actual marked this row as deleted.
    tombstone: i64,
}

/// Loads non-deleted payees keyed by their Actual identifiers.
fn load_payees(conn: &Connection) -> Result<HashMap<String, String>, ImportError> {
    let mut stmt = conn.prepare("SELECT id, name FROM payees WHERE tombstone = 0")?;
    Ok(stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

/// Loads non-deleted accounts keyed by their Actual identifiers.
fn load_accounts(conn: &Connection) -> Result<HashMap<String, String>, ImportError> {
    let mut stmt = conn.prepare("SELECT id, name FROM accounts WHERE tombstone = 0")?;
    Ok(stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

/// Loads non-deleted categories with their group-qualified names.
fn load_categories(conn: &Connection) -> Result<HashMap<String, String>, ImportError> {
    let mut stmt = conn.prepare("SELECT categories.id, category_groups.name || ':' || categories.name FROM categories LEFT JOIN category_groups ON category_groups.id = categories.cat_group WHERE categories.tombstone = 0")?;
    Ok(stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

/// Loads transactions in the order used by the migration output.
fn load_transactions(conn: &Connection) -> Result<Vec<Row>, ImportError> {
    let mut stmt = conn.prepare("SELECT id, acct, category, amount, date, description, COALESCE(imported_description, ''), transferred_id, parent_id, isParent, starting_balance_flag, tombstone FROM transactions ORDER BY date, sort_order")?;
    Ok(stmt
        .query_map([], |r| {
            Ok(Row {
                id: r.get(0)?,
                account: r.get(1)?,
                category: r.get(2)?,
                amount: r.get(3)?,
                date: r.get(4)?,
                description: r.get(5)?,
                imported_description: r.get(6)?,
                transferred_id: r.get(7)?,
                parent_id: r.get(8)?,
                is_parent: r.get(9)?,
                starting_balance: r.get(10)?,
                tombstone: r.get(11)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

#[derive(Debug)]
/// A supported Actual Budget pre-stage rule.
struct Rule {
    /// Regex matched against the imported description.
    pattern: regex::Regex,
    /// Actual payee selected when the rule matches.
    payee_id: String,
}

/// Loads supported pre-stage Actual rules and compiles their match patterns.
fn load_rules(conn: &Connection) -> Result<Vec<Rule>, ImportError> {
    let mut stmt = conn.prepare("SELECT conditions, actions FROM rules WHERE tombstone = 0 AND (stage = 'pre' OR stage IS NULL)")?;
    let mut result = Vec::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (conditions, actions) = row?;
        let conditions: serde_json::Value = serde_json::from_str(&conditions)
            .map_err(|e| ImportError::ActualRule(e.to_string()))?;
        let actions: serde_json::Value =
            serde_json::from_str(&actions).map_err(|e| ImportError::ActualRule(e.to_string()))?;
        let Some(condition) = conditions.as_array().and_then(|v| v.first()) else {
            continue;
        };
        if condition.get("field").and_then(|v| v.as_str()) != Some("imported_description") {
            continue;
        }
        let pattern = match condition.get("op").and_then(|v| v.as_str()) {
            Some("matches") => condition
                .get("value")
                .and_then(|v| v.as_str())
                .map(regex::Regex::new),
            Some("contains") => condition
                .get("value")
                .and_then(|v| v.as_str())
                .map(|value| regex::Regex::new(&regex::escape(value))),
            Some("oneOf") => condition
                .get("value")
                .and_then(|v| v.as_array())
                .map(|values| {
                    let alternatives = values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .map(regex::escape)
                        .collect::<Vec<_>>()
                        .join("|");
                    regex::Regex::new(&format!("^(?:{alternatives})$"))
                }),
            _ => None,
        };
        let Some(pattern) = pattern else { continue };
        let pattern = pattern.map_err(|e| ImportError::ActualRule(e.to_string()))?;
        let Some(payee_id) = actions
            .as_array()
            .and_then(|v| v.first())
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        result.push(Rule {
            pattern,
            payee_id: payee_id.to_string(),
        });
    }
    Ok(result)
}

/// Resolves a matching Actual rule to a category account.
fn apply_rules(
    rules: &[Rule],
    row: &Row,
    categories: &HashMap<String, String>,
    _payees: &HashMap<String, String>,
) -> Option<String> {
    let rule = rules
        .iter()
        .find(|rule| rule.pattern.is_match(&row.imported_description))?;
    let _ = &rule.payee_id;
    Some(resolve_category(categories, row.category.as_deref()))
}

/// Resolves a matching Actual rule to its configured payee.
fn apply_payee_rules(
    rules: &[Rule],
    row: &Row,
    payees: &HashMap<String, String>,
) -> Option<String> {
    rules
        .iter()
        .find(|rule| rule.pattern.is_match(&row.imported_description))
        .and_then(|rule| payees.get(&rule.payee_id))
        .cloned()
}

/// Resolves the preferred payee name for an Actual transaction row.
fn resolve_payee(row: &Row, payees: &HashMap<String, String>) -> Option<String> {
    row.description
        .as_ref()
        .and_then(|id| payees.get(id))
        .cloned()
        .or_else(|| {
            (!row.imported_description.is_empty()).then(|| row.imported_description.clone())
        })
}

/// Converts an Actual account identifier into a valid Pactole asset account.
fn resolve_account(
    accounts: &HashMap<String, String>,
    id: &str,
) -> Result<AccountName, ImportError> {
    account_name(&format!(
        "Assets:{}",
        accounts.get(id).map(String::as_str).unwrap_or(id)
    ))
}

/// Converts an Actual category into an Income or Expenses account path.
fn resolve_category(categories: &HashMap<String, String>, id: Option<&str>) -> String {
    id.and_then(|id| categories.get(id))
        .map(|name| {
            let mut parts = name.splitn(2, ':');
            let group = parts.next().unwrap_or("Other");
            let category = parts.next().unwrap_or(group);
            format!(
                "{}:{}:{}",
                if group.to_lowercase().contains("reven") {
                    "Income"
                } else {
                    "Expenses"
                },
                group,
                category
            )
        })
        .unwrap_or_else(|| "Expenses:Uncategorized".to_string())
}

/// Sanitizes and validates a colon-separated Pactole account name.
fn account_name(value: &str) -> Result<AccountName, ImportError> {
    let value = value
        .split(':')
        .map(sanitize_segment)
        .collect::<Vec<_>>()
        .join(":");
    AccountName::new(value.clone()).map_err(|source| ImportError::InvalidAccount {
        account: value,
        source,
    })
}

/// Replaces characters that are invalid in an account segment.
fn sanitize_segment(value: &str) -> String {
    let mut result: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if result.is_empty() {
        result.push_str("Unknown");
    }
    result
}

/// Converts Actual's numeric date representation to a calendar date.
fn parse_date(value: i64) -> Result<NaiveDate, ImportError> {
    NaiveDate::parse_from_str(&value.to_string(), "%Y%m%d")
        .map_err(|e| ImportError::InvalidTransaction(e.to_string()))
}

/// Converts Actual minor units into Pactole's decimal major units.
fn amount(value: i64) -> Decimal {
    // Actual stores transaction amounts as integer minor units (cents for
    // the currencies supported by this migration), while Pactole amounts are
    // expressed in the currency's major unit. Keep the conversion in one
    // place so opening balances, regular transactions, splits and transfers
    // all use the same scale.
    Decimal::new(value, 2)
}

/// Builds a posting with the supplied account, amount and commodity.
fn posting(account: AccountName, number: Decimal, commodity: &CommodityName) -> Posting {
    Posting {
        account,
        amount: Some(Amount {
            number,
            commodity: commodity.clone(),
        }),
        transaction_cost: None,
        meta: Metadata::new(),
    }
}

/// Builds a cleared transaction from migrated postings.
fn transaction(
    date: NaiveDate,
    payee: String,
    narration: Option<String>,
    postings: Vec<Posting>,
) -> Transaction {
    Transaction {
        date,
        effective_date: None,
        status: TransactionStatus::Cleared,
        payee: Some(payee),
        narration,
        postings,
        tags: Vec::new(),
        links: Vec::new(),
        reference: None,
        meta: Metadata::new(),
    }
}

/// Adds the source payee as metadata when migration settings request it.
fn with_payee_metadata(
    mut transaction: Transaction,
    payee: Option<&str>,
    key: Option<&MetadataKey>,
) -> Transaction {
    if let (Some(payee), Some(key)) = (payee, key) {
        transaction.meta.insert(key.clone(), payee.to_string());
    }
    transaction
}

/// Kept as a small compatibility helper for the original spike.
pub fn parse_actual_database(filepath: &Path) -> Result<(), ImportError> {
    let _ = migrate_actual_database(
        filepath,
        "EUR",
        "Equity:Opening",
        NaiveDate::from_ymd_opt(1970, 1, 1)
            .ok_or_else(|| ImportError::InvalidTransaction("invalid date".into()))?,
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::amount;
    use rust_decimal::Decimal;

    #[test]
    fn converts_actual_minor_units_to_currency_units() {
        assert_eq!(amount(1_000), Decimal::new(10, 0));
        assert_eq!(amount(-1_050), Decimal::new(-105, 1));
        assert_eq!(amount(0), Decimal::ZERO);
    }
}
