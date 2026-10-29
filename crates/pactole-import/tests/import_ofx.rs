use std::collections::{BTreeMap, HashSet};

use chrono::NaiveDate;
use pactole_core::{Transaction, TransactionStatus};
use pactole_import::{Direction, ImportConfig, ImportError, ImportOptions, ImportWarning, Rule};
use rust_decimal::Decimal;

const XML: &str = include_str!("fixtures/statement_xml.ofx");
const TWO_ACCOUNTS: &str = include_str!("fixtures/statement_two_accounts.ofx");
const SGML: &str = include_str!("fixtures/statement_sgml.ofx");
const SGML_ACCTKEY: &str = include_str!("fixtures/statement_sgml_acctkey.ofx");
const SGML_CP1252: &[u8] = include_bytes!("fixtures/statement_cp1252.ofx");
const INVESTMENT: &str = include_str!("fixtures/statement_investment.ofx");

fn base_config() -> ImportConfig {
    ImportConfig {
        bank_account: "Actifs:Banque".to_string(),
        accounts: BTreeMap::new(),
        currency: None,
        default_counter_account: "Depenses:A-Classer".to_string(),
        payee_metadata_key: None,
        no_imported_payee: false,
        omit_negative_amount: false,
        rules: Vec::new(),
    }
}

fn dec(value: &str) -> Decimal {
    value.parse().expect("valid decimal literal")
}

fn fitid(transaction: &Transaction) -> Option<&str> {
    transaction.meta.get("fitid").map(String::as_str)
}

fn posting_for_account<'a>(
    transaction: &'a Transaction,
    account: &str,
) -> &'a pactole_core::Posting {
    transaction
        .postings
        .iter()
        .find(|posting| posting.account.as_str() == account)
        .expect("posting for account")
}

fn no_fitids() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn imports_an_xml_bank_statement() {
    let outcome = pactole_import::import_ofx(XML, &base_config(), &no_fitids()).unwrap();

    assert_eq!(outcome.skipped, 0);
    assert!(outcome.warnings.is_empty());
    assert_eq!(outcome.transactions.len(), 2);

    let debit = &outcome.transactions[0];
    assert_eq!(debit.date, NaiveDate::from_ymd_opt(2026, 9, 3).unwrap());
    assert_eq!(debit.status, TransactionStatus::Cleared);
    assert_eq!(debit.payee.as_deref(), Some("CARREFOUR"));
    assert_eq!(debit.narration.as_deref(), Some("Courses"));
    assert_eq!(fitid(debit), Some("2026090300001"));

    // The positive posting comes first, then the negative imported account.
    assert_eq!(debit.postings[0].account.as_str(), "Depenses:A-Classer");
    let counter = debit.postings[0].amount.as_ref().unwrap();
    assert_eq!(counter.number, dec("45.30"));
    assert_eq!(counter.commodity.as_str(), "EUR");

    assert_eq!(debit.postings[1].account.as_str(), "Actifs:Banque");
    let bank = debit.postings[1].amount.as_ref().unwrap();
    assert_eq!(bank.number, dec("-45.30"));
    assert_eq!(bank.commodity.as_str(), "EUR");

    let credit = &outcome.transactions[1];
    assert_eq!(credit.payee.as_deref(), Some("ACME CORP"));
    assert_eq!(
        credit.postings[0].amount.as_ref().unwrap().number,
        dec("2000.00")
    );
}

#[test]
fn filters_from_date_and_sorts_oldest_first() {
    let outcome = pactole_import::import_ofx_with_options(
        XML,
        &base_config(),
        &no_fitids(),
        ImportOptions {
            from: Some(NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()),
            no_imported_payee: None,
            omit_negative_amount: None,
        },
    )
    .unwrap();

    assert_eq!(outcome.transactions.len(), 1);
    assert_eq!(
        outcome.transactions[0].date,
        NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()
    );
}

#[test]
fn option_can_disable_configured_imported_payee_metadata() {
    let config = ImportConfig {
        payee_metadata_key: Some("imported-payee".to_string()),
        ..base_config()
    };

    let outcome = pactole_import::import_ofx_with_options(
        XML,
        &config,
        &no_fitids(),
        ImportOptions {
            from: None,
            no_imported_payee: Some(true),
            omit_negative_amount: None,
        },
    )
    .unwrap();

    assert!(!outcome.transactions[0].meta.contains_key("imported-payee"));
    assert!(outcome.transactions[0].meta.contains_key("fitid"));
}

#[test]
fn omits_the_negative_posting_amount_when_requested() {
    let outcome = pactole_import::import_ofx_with_options(
        XML,
        &base_config(),
        &no_fitids(),
        ImportOptions {
            from: None,
            no_imported_payee: None,
            omit_negative_amount: Some(true),
        },
    )
    .unwrap();

    let debit = &outcome.transactions[0];
    assert!(debit.postings[0].amount.is_some());
    assert!(debit.postings[1].amount.is_none());

    let credit = &outcome.transactions[1];
    assert!(credit.postings[0].amount.is_some());
    assert!(credit.postings[1].amount.is_none());
}

#[test]
fn keeps_the_statement_currency_for_a_transaction_with_original_currency() {
    let source = XML.replacen(
        "<TRNAMT>-45.30</TRNAMT>",
        "<TRNAMT>-45.30</TRNAMT><ORIGCURRENCY><CURRATE>1.0</CURRATE><CURSYM>USD</CURSYM></ORIGCURRENCY>",
        1,
    );

    let outcome = pactole_import::import_ofx(&source, &base_config(), &no_fitids()).unwrap();
    let transaction = &outcome.transactions[0];

    assert_eq!(
        transaction.postings[0]
            .amount
            .as_ref()
            .unwrap()
            .commodity
            .as_str(),
        "EUR"
    );
    assert_eq!(
        transaction.postings[1]
            .amount
            .as_ref()
            .unwrap()
            .commodity
            .as_str(),
        "EUR"
    );
}

#[test]
fn uses_the_transaction_currency_when_ofx_provides_currency() {
    let source = XML.replacen(
        "<TRNAMT>-45.30</TRNAMT>",
        "<TRNAMT>-45.30</TRNAMT><CURRENCY><CURRATE>1.0</CURRATE><CURSYM>USD</CURSYM></CURRENCY>",
        1,
    );

    let outcome = pactole_import::import_ofx(&source, &base_config(), &no_fitids()).unwrap();
    let transaction = &outcome.transactions[0];

    assert_eq!(
        transaction.postings[0]
            .amount
            .as_ref()
            .unwrap()
            .commodity
            .as_str(),
        "USD"
    );
}

#[test]
fn maps_each_statement_account_to_its_configured_pactole_account() {
    let config = ImportConfig {
        accounts: BTreeMap::from([
            ("00011111111".to_string(), "Actifs:Courant".to_string()),
            ("00022222222".to_string(), "Actifs:Epargne".to_string()),
        ]),
        ..base_config()
    };

    let outcome = pactole_import::import_ofx(TWO_ACCOUNTS, &config, &no_fitids()).unwrap();

    // No transaction fell back: no unmapped-account warning.
    assert!(outcome.warnings.is_empty());
    assert_eq!(outcome.transactions.len(), 3);

    // Both statements are collected in document order, each transaction
    // posted to the account mapped from its statement's ACCTID.
    assert_eq!(
        posting_for_account(&outcome.transactions[0], "Actifs:Courant")
            .account
            .as_str(),
        "Actifs:Courant"
    );
    assert_eq!(
        posting_for_account(&outcome.transactions[1], "Actifs:Courant")
            .account
            .as_str(),
        "Actifs:Courant"
    );
    assert_eq!(
        posting_for_account(&outcome.transactions[2], "Actifs:Epargne")
            .account
            .as_str(),
        "Actifs:Epargne"
    );
}

#[test]
fn falls_back_to_the_bank_account_once_per_unmapped_statement_account() {
    // Only the savings account is mapped: the two checking transactions
    // fall back to `bank_account`, warned about once (not per transaction).
    let config = ImportConfig {
        accounts: BTreeMap::from([("00022222222".to_string(), "Actifs:Epargne".to_string())]),
        ..base_config()
    };

    let outcome = pactole_import::import_ofx(TWO_ACCOUNTS, &config, &no_fitids()).unwrap();

    assert_eq!(
        outcome.warnings,
        vec![ImportWarning::UnmappedAccount {
            account_id: "00011111111".to_string(),
        }]
    );

    assert_eq!(
        posting_for_account(&outcome.transactions[0], "Actifs:Banque")
            .account
            .as_str(),
        "Actifs:Banque"
    );
    assert_eq!(
        posting_for_account(&outcome.transactions[1], "Actifs:Banque")
            .account
            .as_str(),
        "Actifs:Banque"
    );
    assert_eq!(
        posting_for_account(&outcome.transactions[2], "Actifs:Epargne")
            .account
            .as_str(),
        "Actifs:Epargne"
    );
}

#[test]
fn stays_silent_without_any_account_mapping() {
    // With no `[import.accounts]` table at all, `bank_account` is the primary
    // account rather than a fallback: every transaction goes there and no
    // unmapped-account warning is emitted.
    let outcome = pactole_import::import_ofx(TWO_ACCOUNTS, &base_config(), &no_fitids()).unwrap();

    assert!(outcome.warnings.is_empty());
    assert_eq!(outcome.transactions.len(), 3);
    for transaction in &outcome.transactions {
        assert_eq!(
            posting_for_account(transaction, "Actifs:Banque")
                .account
                .as_str(),
            "Actifs:Banque"
        );
    }
}

#[test]
fn reports_an_error_on_an_invalid_account_in_the_mapping() {
    let config = ImportConfig {
        accounts: BTreeMap::from([("00011111111".to_string(), "not-an-account".to_string())]),
        ..base_config()
    };

    let error = pactole_import::import_ofx(TWO_ACCOUNTS, &config, &no_fitids()).unwrap_err();

    assert!(matches!(error, ImportError::InvalidAccount { .. }));
}

#[test]
fn imports_a_sgml_bank_statement() {
    let outcome = pactole_import::import_ofx(SGML, &base_config(), &no_fitids()).unwrap();

    assert_eq!(outcome.transactions.len(), 1);

    let transaction = &outcome.transactions[0];
    assert_eq!(transaction.payee.as_deref(), Some("BOULANGERIE"));
    assert_eq!(fitid(transaction), Some("SGML-1"));
    assert_eq!(
        transaction.postings[0].amount.as_ref().unwrap().number,
        dec("12.00")
    );
}

#[test]
fn imports_a_sgml_bank_statement_carrying_acctkey() {
    // Regression: `ACCTKEY` inside `BANKACCTFROM` used to make the SGML
    // normalizer close the account aggregate early and drop the whole
    // `BANKTRANLIST`, so the statement parsed with no transaction at all.
    let outcome = pactole_import::import_ofx(SGML_ACCTKEY, &base_config(), &no_fitids()).unwrap();

    assert_eq!(outcome.transactions.len(), 1);

    let transaction = &outcome.transactions[0];
    assert_eq!(
        transaction.payee.as_deref(),
        Some("VIREMENT PERMANENT POUR")
    );
    assert_eq!(fitid(transaction), Some("1234sen"));
    assert_eq!(
        transaction.postings[0].amount.as_ref().unwrap().number,
        dec("10.00")
    );
}

#[test]
fn decodes_and_imports_a_windows_1252_statement() {
    // The fixture is not valid UTF-8: its payee and narration carry raw
    // windows-1252 bytes, as declared by the header (`CHARSET:1252`).
    let source = pactole_import::decode_ofx_source(SGML_CP1252);
    let outcome = pactole_import::import_ofx(&source, &base_config(), &no_fitids()).unwrap();

    assert_eq!(outcome.transactions.len(), 1);

    let transaction = &outcome.transactions[0];
    assert_eq!(
        transaction.date,
        NaiveDate::from_ymd_opt(2026, 10, 10).expect("date")
    );
    assert_eq!(transaction.payee.as_deref(), Some("CAFÉ LÉON"));
    assert_eq!(
        transaction.narration.as_deref(),
        Some("Croissants à la boulangerie")
    );
    assert_eq!(fitid(transaction), Some("CP1252-1"));
    assert_eq!(
        transaction.postings[0].amount.as_ref().unwrap().number,
        dec("12.50")
    );
}

#[test]
fn imports_the_banking_transactions_of_an_investment_statement() {
    let outcome = pactole_import::import_ofx(INVESTMENT, &base_config(), &no_fitids()).unwrap();

    // Only the INVBANKTRAN banking transaction is imported; the BUYSTOCK
    // trade is an investment operation and is left out.
    assert_eq!(outcome.transactions.len(), 1);
    let transaction = &outcome.transactions[0];
    assert_eq!(fitid(transaction), Some("INV-1"));
    assert_eq!(
        transaction.payee.as_deref(),
        Some("VIREMENT PERMANENT POUR")
    );
}

#[test]
fn reports_an_error_when_statements_contain_transactions_that_are_dropped() {
    // A field the SGML normalizer does not know inside `BANKACCTFROM` makes
    // it close the account aggregate early and unwind the stack, dropping
    // `BANKTRANLIST`: the parser then reports an empty statement even though
    // the source plainly contains a transaction.
    let source = "\
OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE

<OFX>
<SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS><DTSERVER>20261016120000<LANGUAGE>FRA</SONRS></SIGNONMSGSRSV1>
<BANKMSGSRSV1><STMTTRNRS><TRNUID>1234<STATUS><CODE>0<SEVERITY>INFO</STATUS><STMTRS><CURDEF>EUR<BANKACCTFROM><BANKID>0000<ACCTID>1231<ACCTTYPE>CHECKING<UNKNOWNLEAF>41</BANKACCTFROM><BANKTRANLIST><DTSTART>20260918<DTEND>20261016<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20261015<TRNAMT>-10.00<FITID>DROPPED-1<NAME>FOURNISSEUR</STMTTRN></BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1>
</OFX>
";

    let error = pactole_import::import_ofx(source, &base_config(), &no_fitids()).unwrap_err();

    assert!(matches!(error, ImportError::TransactionsDropped));
}

#[test]
fn skips_transactions_already_known() {
    let known: HashSet<String> = ["2026090300001".to_string()].into_iter().collect();

    let outcome = pactole_import::import_ofx(XML, &base_config(), &known).unwrap();

    assert_eq!(outcome.skipped, 1);
    assert_eq!(outcome.transactions.len(), 1);
    assert_eq!(fitid(&outcome.transactions[0]), Some("2026090500002"));
}

#[test]
fn applies_matching_rules_in_order() {
    let config = ImportConfig {
        rules: vec![
            Rule {
                payee_contains: Some("carrefour".to_string()),
                payee_in: None,
                payee_regex: None,
                memo_contains: None,
                memo_in: None,
                memo_regex: None,
                trntype: None,
                trntype_in: None,
                account: None,
                direction: None,
                counter_account: Some("Depenses:Alimentation".to_string()),
                payee: Some("Carrefour".to_string()),
            },
            Rule {
                payee_contains: None,
                payee_in: None,
                payee_regex: None,
                memo_contains: None,
                memo_in: None,
                memo_regex: None,
                trntype: None,
                trntype_in: None,
                account: None,
                direction: Some(Direction::Credit),
                counter_account: Some("Revenus:Salaire".to_string()),
                payee: None,
            },
        ],
        ..base_config()
    };

    let outcome = pactole_import::import_ofx(XML, &config, &no_fitids()).unwrap();

    let groceries = &outcome.transactions[0];
    assert_eq!(
        groceries.postings[0].account.as_str(),
        "Depenses:Alimentation"
    );
    assert_eq!(groceries.payee.as_deref(), Some("Carrefour"));

    let salary = &outcome.transactions[1];
    assert_eq!(
        posting_for_account(salary, "Revenus:Salaire")
            .account
            .as_str(),
        "Revenus:Salaire"
    );
    assert_eq!(salary.payee.as_deref(), Some("ACME CORP"));
}

#[test]
fn applies_matching_rules_in_order_to_the_transformed_payee() {
    let config = ImportConfig {
        rules: vec![
            Rule {
                payee_contains: Some("carrefour".to_string()),
                payee_in: None,
                payee_regex: None,
                memo_contains: None,
                memo_in: None,
                memo_regex: None,
                trntype: None,
                trntype_in: None,
                account: None,
                direction: None,
                counter_account: Some("Depenses:A-Classer".to_string()),
                payee: Some("boulangerie 1".to_string()),
            },
            Rule {
                payee_contains: Some("boulangerie".to_string()),
                payee_in: None,
                payee_regex: None,
                memo_contains: None,
                memo_in: None,
                memo_regex: None,
                trntype: None,
                trntype_in: None,
                account: None,
                direction: None,
                counter_account: Some("Depenses:Pain".to_string()),
                payee: None,
            },
        ],
        ..base_config()
    };

    let outcome = pactole_import::import_ofx(XML, &config, &no_fitids()).unwrap();
    let transaction = &outcome.transactions[0];

    assert_eq!(transaction.payee.as_deref(), Some("boulangerie 1"));
    assert_eq!(transaction.postings[0].account.as_str(), "Depenses:Pain");
}

#[test]
fn applies_regex_list_and_account_rules_and_preserves_source_payee() {
    let config = ImportConfig {
        payee_metadata_key: Some("imported-payee".to_string()),
        rules: vec![Rule {
            payee_contains: None,
            payee_in: Some(vec!["CARREFOUR".to_string()]),
            payee_regex: Some("^carre".to_string()),
            memo_contains: None,
            memo_in: None,
            memo_regex: None,
            trntype: None,
            trntype_in: None,
            account: Some("00012345678".to_string()),
            direction: None,
            counter_account: Some("Depenses:Alimentation".to_string()),
            payee: Some("Carrefour".to_string()),
        }],
        ..base_config()
    };

    let outcome = pactole_import::import_ofx(XML, &config, &no_fitids()).unwrap();
    let groceries = &outcome.transactions[0];
    assert_eq!(groceries.payee.as_deref(), Some("Carrefour"));
    assert_eq!(
        groceries.meta.get("imported-payee").map(String::as_str),
        Some("CARREFOUR")
    );
    assert_eq!(
        groceries.postings[0].account.as_str(),
        "Depenses:Alimentation"
    );
    assert_eq!(
        posting_for_account(&outcome.transactions[1], "Depenses:A-Classer")
            .account
            .as_str(),
        "Depenses:A-Classer"
    );
}

#[test]
fn warns_and_falls_back_when_payee_is_missing() {
    let source = r#"<?OFX OFXHEADER="200" VERSION="220" SECURITY="NONE" OLDFILEUID="NONE" NEWFILEUID="NONE"?>
<OFX>
<SIGNONMSGSRSV1>
<SONRS>
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>
<DTSERVER>20260903120000</DTSERVER>
<LANGUAGE>ENG</LANGUAGE>
</SONRS>
</SIGNONMSGSRSV1>
<BANKMSGSRSV1>
<STMTTRNRS>
<TRNUID>1</TRNUID>
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>
<STMTRS>
<CURDEF>EUR</CURDEF>
<BANKACCTFROM><BANKID>1</BANKID><ACCTID>2</ACCTID><ACCTTYPE>CHECKING</ACCTTYPE></BANKACCTFROM>
<BANKTRANLIST>
<DTSTART>20260901</DTSTART>
<DTEND>20260930</DTEND>
<STMTTRN>
<TRNTYPE>DEBIT</TRNTYPE>
<DTPOSTED>20260903</DTPOSTED>
<TRNAMT>-1.00</TRNAMT>
<FITID>NOPAYEE</FITID>
</STMTTRN>
</BANKTRANLIST>
</STMTRS>
</STMTTRNRS>
</BANKMSGSRSV1>
</OFX>"#;

    let outcome = pactole_import::import_ofx(source, &base_config(), &no_fitids()).unwrap();

    assert_eq!(outcome.transactions.len(), 1);
    assert_eq!(outcome.transactions[0].payee.as_deref(), Some("Unknown"));
    assert_eq!(
        outcome.warnings,
        vec![ImportWarning::MissingPayee {
            fitid: Some("NOPAYEE".to_string()),
        }]
    );
}

#[test]
fn reports_an_error_when_no_statement_is_present() {
    let source = r#"<?OFX OFXHEADER="200" VERSION="220" SECURITY="NONE" OLDFILEUID="NONE" NEWFILEUID="NONE"?>
<OFX>
<SIGNONMSGSRSV1>
<SONRS>
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>
<DTSERVER>20260903120000</DTSERVER>
<LANGUAGE>ENG</LANGUAGE>
</SONRS>
</SIGNONMSGSRSV1>
</OFX>"#;

    let error = pactole_import::import_ofx(source, &base_config(), &no_fitids()).unwrap_err();

    assert!(matches!(error, ImportError::NoStatement));
}

#[test]
fn reports_an_error_on_unparsable_input() {
    let error =
        pactole_import::import_ofx("not an OFX file", &base_config(), &no_fitids()).unwrap_err();

    assert!(matches!(error, ImportError::Ofx(_)));
}

#[test]
fn reports_an_error_on_an_invalid_configured_account() {
    let config = ImportConfig {
        bank_account: "not-an-account".to_string(),
        ..base_config()
    };

    let error = pactole_import::import_ofx(XML, &config, &no_fitids()).unwrap_err();

    assert!(matches!(error, ImportError::InvalidAccount { .. }));
}
