use chrono::NaiveDate;
use ofx_rs::aggregates::TransactionList;
use ofx_rs::parse;
use ofx_rs::types::OfxDateTime;

use crate::error::ImportError;
use crate::record::ImportRecord;

/// Parses an OFX document (1.x SGML or 2.x XML) into format-neutral
/// [`ImportRecord`]s.
///
/// `BANKMSGSRSV1` (bank statements), `CREDITCARDMSGSRSV1` (credit card
/// statements) and `INVSTMTMSGSRSV1` (investment statements, banking
/// transactions only) are supported; every statement response present in the
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
                let account_id = statement.bank_account().account_id().as_str();
                collect(
                    statement.transaction_list(),
                    currency,
                    account_id,
                    &mut records,
                )?;
            }
        }
    }

    if let Some(credit_card) = document.credit_card() {
        for wrapper in credit_card.statement_responses() {
            if let Some(statement) = wrapper.response() {
                found_statement = true;
                let currency = statement.currency_default().as_str();
                let account_id = statement.credit_card_account().account_id().as_str();
                collect(
                    statement.transaction_list(),
                    currency,
                    account_id,
                    &mut records,
                )?;
            }
        }
    }

    // Investment statements carry their banking transactions (deposits,
    // withdrawals) in an INVTRANLIST; ofx-rs already lowers those into plain
    // transactions and skips the investment-specific ones.
    if let Some(investment) = document.investment() {
        for wrapper in investment.statement_responses() {
            if let Some(statement) = wrapper.response() {
                found_statement = true;
                let currency = statement.currency_default().as_str();
                let account_id = statement.investment_account().account_id().as_str();
                collect(
                    statement.transaction_list(),
                    currency,
                    account_id,
                    &mut records,
                )?;
            }
        }
    }

    if !found_statement {
        return Err(ImportError::NoStatement);
    }

    // A document that visibly contains transactions but yielded none means
    // the parser dropped them (an OFX dialect it mishandles), not that the
    // statement is empty. Failing loudly beats importing nothing silently.
    if records.is_empty() && contains_stmttrn_tag(source) {
        return Err(ImportError::TransactionsDropped);
    }

    Ok(records)
}

/// Returns whether the source text contains an `<STMTTRN>` element outside of
/// a comment.
///
/// Comments are skipped so a commented-out transaction is not mistaken for
/// one the parser dropped, and the check looks for the `<STMTTRN>` element
/// itself so that the `<STMTTRNRS>` statement wrapper does not count.
fn contains_stmttrn_tag(source: &str) -> bool {
    let mut remaining = source;

    while let Some(start) = remaining.find("<!--") {
        // Start of comment found in source
        if remaining.get(..start).is_some_and(has_stmttrn_element) {
            // STMTTRN found before the comemnt
            return true;
        }
        match remaining
            .get(start + "<!--".len()..)
            .and_then(|after| after.find("-->"))
        {
            Some(end) => {
                // Foud end of comment. Update remaining and seek in next loop
                remaining = remaining
                    .get(start + "<!--".len() + end + "-->".len()..)
                    .unwrap_or_default();
            }
            // Unterminated comment: everything after it is commented out.
            None => return false,
        }
    }

    // The source did not has comments
    // or all comments read without STMTTRN element.
    has_stmttrn_element(remaining)
}

/// Case-insensitive test for the `<STMTTRN>` opening tag.
fn has_stmttrn_element(text: &str) -> bool {
    text.to_ascii_uppercase().contains("<STMTTRN>")
}

/// Extracts the transactions of one statement into [`ImportRecord`]s.
///
/// `account_id` is the identifier of the account the statement belongs to
/// (OFX `ACCTID`), recorded on every produced record so the rules engine can
/// map it onto the configured Pactole account.
fn collect(
    list: Option<&TransactionList>,
    currency: &str,
    account_id: &str,
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
            account_id: Some(account_id.to_string()),
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

#[cfg(test)]
mod tests {
    use super::contains_stmttrn_tag;

    #[test]
    fn detects_a_statement_transaction_element() {
        assert!(contains_stmttrn_tag("<STMTTRN><TRNTYPE>DEBIT"));
        assert!(contains_stmttrn_tag("<stmttrn>\n<TRNTYPE>DEBIT"));
    }

    #[test]
    fn ignores_the_statement_wrapper() {
        // `<STMTTRNRS>` wraps a statement and is present even when the
        // statement itself carries no transaction.
        assert!(!contains_stmttrn_tag("<STMTTRNRS><TRNUID>1</TRNUID>"));
    }

    #[test]
    fn ignores_commented_out_transactions() {
        assert!(!contains_stmttrn_tag(
            "<BANKTRANLIST><!-- <STMTTRN> --></BANKTRANLIST>"
        ));
        // A real transaction after a comment is still detected.
        assert!(contains_stmttrn_tag(
            "<!-- <STMTTRN> --><STMTTRN><TRNTYPE>DEBIT"
        ));
        // An unterminated comment hides everything that follows.
        assert!(!contains_stmttrn_tag("<!-- <STMTTRN>"));
    }
}
