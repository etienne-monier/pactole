use chrono::NaiveDate;
use ofx_rs::aggregates::TransactionList;
use ofx_rs::parse;
use ofx_rs::types::OfxDateTime;

use crate::error::ImportError;
use crate::record::ImportRecord;

/// Parses an OFX document (1.x SGML or 2.x XML) into format-neutral
/// [`ImportRecord`]s.
///
/// Both `BANKMSGSRSV1` (bank statements) and `CREDITCARDMSGSRSV1` (credit
/// card statements) are supported; every statement response present in the
/// document contributes its transactions.
pub(crate) fn parse_records(source: &str) -> Result<Vec<ImportRecord>, ImportError> {
    let document = parse(source).map_err(|error| ImportError::Ofx(error.to_string()))?;

    let mut records = Vec::new();
    let mut found_statement = false;

    if let Some(banking) = document.banking() {
        for wrapper in banking.statement_responses() {
            if let Some(statement) = wrapper.response() {
                found_statement = true;
                let currency = statement.currency_default().as_str();
                collect(statement.transaction_list(), currency, &mut records)?;
            }
        }
    }

    if let Some(credit_card) = document.credit_card() {
        for wrapper in credit_card.statement_responses() {
            if let Some(statement) = wrapper.response() {
                found_statement = true;
                let currency = statement.currency_default().as_str();
                collect(statement.transaction_list(), currency, &mut records)?;
            }
        }
    }

    if !found_statement {
        return Err(ImportError::NoStatement);
    }

    Ok(records)
}

/// Extracts the transactions of one statement into [`ImportRecord`]s.
fn collect(
    list: Option<&TransactionList>,
    currency: &str,
    out: &mut Vec<ImportRecord>,
) -> Result<(), ImportError> {
    let Some(list) = list else {
        return Ok(());
    };

    for transaction in list.transactions() {
        let date = to_naive_date(transaction.date_posted())?;

        out.push(ImportRecord {
            date,
            amount: transaction.amount().as_decimal(),
            currency: Some(currency.to_string()),
            payee: transaction.name().map(str::to_string),
            memo: transaction.memo().map(str::to_string),
            transaction_type: Some(transaction.transaction_type().to_string()),
            check_number: transaction
                .check_number()
                .map(|value| value.as_str().to_string()),
            reference: transaction.reference_number().map(str::to_string),
            fitid: Some(transaction.fit_id().as_str().to_string()),
        });
    }

    Ok(())
}

/// Converts an OFX datetime to a [`NaiveDate`], discarding any time component.
fn to_naive_date(value: &OfxDateTime) -> Result<NaiveDate, ImportError> {
    let datetime = value.as_offset_date_time();
    let date = datetime.date();

    NaiveDate::from_ymd_opt(
        date.year(),
        u32::from(date.month() as u8),
        u32::from(date.day()),
    )
    .ok_or_else(|| ImportError::InvalidTransaction(format!("invalid date `{value}`")))
}
