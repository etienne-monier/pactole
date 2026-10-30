use std::path::Path;
use std::process::Command;

/// Path to the sample `.pactole` file shipped with the tree-sitter grammar,
/// used here as an end-to-end fixture for the CLI/core/storage stack.
const SAMPLE_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../tree-sitter-pactole/test/test.pactole"
);

fn pactole_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pactole"))
}

#[test]
fn parse_prints_journal_entries() {
    let output = pactole_cmd()
        .args(["parse", SAMPLE_FILE])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Spot check a few entries coming from each part of the pipeline
    // (declarations, assertions and events) to make sure the CLI, the
    // storage crate and the tree-sitter grammar work together. Entries
    // pulled in through `include` directives (e.g. `Depenses:Alimentation`,
    // opened from `comptes/ouvertures.pactole`) must be inlined directly
    // into the journal, with no `Include` entry left behind.
    assert!(!stdout.contains("Include("));
    assert!(stdout.contains("\"Depenses:Alimentation\""));
    assert!(stdout.contains("Open("));
    assert!(stdout.contains("\"Actifs:Compte-Joint\""));
    assert!(stdout.contains("Balance("));
    assert!(stdout.contains("Transaction("));
    assert!(stdout.contains("\"Carrefour\""));
}

#[test]
fn parse_reports_error_on_missing_file() {
    let output = pactole_cmd()
        .args(["parse", "this-file-does-not-exist.pactole"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

#[test]
fn report_commands_keep_positioned_diagnostics_for_an_invalid_ledger() {
    let directory =
        std::env::temp_dir().join(format!("pactole-report-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    std::fs::write(&ledger, "not a pactole directive\n").unwrap();

    let output = pactole_cmd()
        .args(["balance", ledger.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("syntax-error"), "stderr: {stderr}");
    assert!(
        stderr.contains("not a pactole directive"),
        "stderr: {stderr}"
    );

    std::fs::remove_dir_all(directory).ok();
}

#[test]
fn expenses_supports_optional_sections_and_calendar_month_comparisons() {
    let directory = std::env::temp_dir().join(format!("pactole-expenses-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    let config = directory.join("pactole.toml");
    std::fs::write(
        &ledger,
        "2026-01-01 open Assets:Bank\n\
         2026-01-01 open Expenses:Review\n\
         2026-01-01 open Expenses:Inactive\n\
         2026-01-01 commodity EUR\n\
         payee \"Shop\"\n\
         2026-01-05 * \"Shop\"\n\
         \u{20}\u{20}Assets:Bank -10.00 EUR\n\
         \u{20}\u{20}Expenses:Review 10.00 EUR\n\
         2026-02-05 * \"Shop\"\n\
         \u{20}\u{20}Assets:Bank -15.00 EUR\n\
         \u{20}\u{20}Expenses:Review 15.00 EUR\n",
    )
    .unwrap();
    std::fs::write(
        &config,
        "[reports]\nexpenses = [\"Expenses\"]\nuncategorized = [\"Expenses:Review\"]\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args([
            "expenses",
            ledger.to_str().unwrap(),
            "--from",
            "2026-01-01",
            "--to",
            "2026-02-28",
            "--by",
            "payee",
            "--uncategorized",
            "--inactive",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["sections"][1]["title"], "Uncategorized");
    assert_eq!(report["sections"][1]["lines"].as_array().unwrap().len(), 2);
    assert_eq!(report["sections"][2]["title"], "Inactive accounts");
    assert_eq!(
        report["sections"][2]["lines"][0]["label"],
        "Expenses:Inactive"
    );

    let comparison = pactole_cmd()
        .args([
            "expenses",
            ledger.to_str().unwrap(),
            "--from",
            "2026-02-01",
            "--to",
            "2026-02-28",
            "--compare",
            "previous",
            "--reverse-diff",
        ])
        .output()
        .unwrap();
    assert!(
        comparison.status.success(),
        "{}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison_text = String::from_utf8_lossy(&comparison.stdout);
    assert!(comparison_text.contains("Difference (previous − current)"));
    assert!(comparison_text.contains("-5.00 EUR"));

    let invalid = pactole_cmd()
        .args(["expenses", ledger.to_str().unwrap(), "--reverse-diff"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("require a comparison"));

    std::fs::remove_dir_all(directory).ok();
}

#[test]
fn report_commands_convert_with_dated_prices_and_retain_missing_commodities() {
    let directory = std::env::temp_dir().join(format!("pactole-valuation-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    std::fs::write(
        &ledger,
        concat!(
            "2026-01-01 commodity EUR\n",
            "2026-01-01 commodity USD\n",
            "2026-01-01 commodity GBP\n",
            "2026-01-01 open Assets:Bank\n",
            "2026-01-01 open Expenses:Food\n",
            "payee \"Store\"\n",
            "2026-01-01 price USD 0.9 EUR\n",
            "2026-01-05 * \"Store\"\n",
            "  Assets:Bank -10 USD\n",
            "  Expenses:Food 10 USD\n",
            "2026-01-06 * \"Store\"\n",
            "  Assets:Bank -2 GBP\n",
            "  Expenses:Food 2 GBP\n",
        ),
    )
    .unwrap();

    let expenses = pactole_cmd()
        .args([
            "expenses",
            ledger.to_str().unwrap(),
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
            "--convert-to",
            "EUR",
            "--at",
            "2026-01-06",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        expenses.status.success(),
        "{}",
        String::from_utf8_lossy(&expenses.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&expenses.stdout).unwrap();
    assert_eq!(report["valuation"]["currency"], "EUR");
    assert_eq!(report["valuation"]["date"], "2026-01-06");
    assert_eq!(report["valuation"]["prices"][0]["base"], "USD");
    assert_eq!(report["valuation"]["prices"][0]["date"], "2026-01-01");
    assert_eq!(report["valuation"]["missing"][0], "GBP");
    let amounts = report["sections"][0]["lines"][0]["amounts"]
        .as_array()
        .unwrap();
    assert!(
        amounts
            .iter()
            .any(|amount| { amount["commodity"] == "EUR" && amount["number"] == "9.0" })
    );
    assert!(
        amounts
            .iter()
            .any(|amount| { amount["commodity"] == "GBP" && amount["number"] == "2" })
    );

    let balance = pactole_cmd()
        .args([
            "balance",
            ledger.to_str().unwrap(),
            "--date",
            "2026-01-06",
            "--convert-to",
            "EUR",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        balance.status.success(),
        "{}",
        String::from_utf8_lossy(&balance.stderr)
    );
    let balance: serde_json::Value = serde_json::from_slice(&balance.stdout).unwrap();
    assert_eq!(balance["valuation"]["date"], "2026-01-06");

    let income = pactole_cmd()
        .args([
            "income",
            ledger.to_str().unwrap(),
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
            "--convert-to",
            "EUR",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        income.status.success(),
        "{}",
        String::from_utf8_lossy(&income.stderr)
    );
    let income: serde_json::Value = serde_json::from_slice(&income.stdout).unwrap();
    assert_eq!(income["valuation"]["currency"], "EUR");
    assert_eq!(income["valuation"]["date"], "2026-01-31");

    let comparison = pactole_cmd()
        .args([
            "expenses",
            ledger.to_str().unwrap(),
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
            "--compare",
            "previous",
            "--convert-to",
            "EUR",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        comparison.status.success(),
        "{}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison: serde_json::Value = serde_json::from_slice(&comparison.stdout).unwrap();
    assert_eq!(comparison["valuation"]["current"]["date"], "2026-01-31");
    assert_eq!(comparison["valuation"]["current"]["missing"][0], "GBP");

    let register = pactole_cmd()
        .args([
            "register",
            ledger.to_str().unwrap(),
            "--account",
            "Expenses:Food",
            "--convert-to",
            "EUR",
            "--at",
            "2026-01-06",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        register.status.success(),
        "{}",
        String::from_utf8_lossy(&register.stderr)
    );
    let register: serde_json::Value = serde_json::from_slice(&register.stdout).unwrap();
    assert!(register["entries"].is_array());
    assert_eq!(register["valuation"]["missing"][0], "GBP");
    assert_eq!(register["entries"][0]["amount"]["commodity"], "EUR");
    assert_eq!(register["entries"][0]["amount"]["number"], "9.0");
    assert_eq!(register["entries"][0]["balance"][0]["number"], "9.0");

    let invalid = pactole_cmd()
        .args(["balance", ledger.to_str().unwrap(), "--at", "2026-01-06"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("--convert-to <COMMODITY>"));

    let undeclared_target = pactole_cmd()
        .args(["balance", ledger.to_str().unwrap(), "--convert-to", "JPY"])
        .output()
        .unwrap();
    assert!(!undeclared_target.status.success());
    assert!(
        String::from_utf8_lossy(&undeclared_target.stderr)
            .contains("commodity JPY is not declared in the journal")
    );

    std::fs::remove_dir_all(directory).ok();
}

#[test]
fn imported_foreign_currency_transactions_are_valued_by_reports() {
    let directory =
        std::env::temp_dir().join(format!("pactole-import-valuation-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    let ofx = directory.join("statement.ofx");
    std::fs::write(
        &ledger,
        concat!(
            "2026-01-01 commodity EUR\n",
            "2026-01-01 commodity USD\n",
            "2026-01-01 open Assets:Bank\n",
            "2026-01-01 open Expenses:ToReview\n",
            "2026-01-01 price USD 0.90 EUR\n",
            "payee \"CARREFOUR\"\n",
            "payee \"ACME CORP\"\n",
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("pactole.toml"),
        "[import]\nbank_account = \"Assets:Bank\"\ncurrency = \"EUR\"\ndefault_counter_account = \"Expenses:ToReview\"\n",
    )
    .unwrap();
    let source = include_str!("../../pactole-import/tests/fixtures/statement_xml.ofx").replacen(
        "<TRNAMT>-45.30</TRNAMT>",
        "<TRNAMT>-45.30</TRNAMT><CURRENCY><CURRATE>1.0</CURRATE><CURSYM>USD</CURSYM></CURRENCY>",
        1,
    );
    std::fs::write(&ofx, source).unwrap();

    let import = pactole_cmd()
        .args([
            "import-ofx",
            ofx.to_str().unwrap(),
            "--into",
            ledger.to_str().unwrap(),
            "--write",
        ])
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );

    let balance = pactole_cmd()
        .args([
            "balance",
            ledger.to_str().unwrap(),
            "--date",
            "2026-09-30",
            "--convert-to",
            "EUR",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        balance.status.success(),
        "{}",
        String::from_utf8_lossy(&balance.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&balance.stdout).unwrap();
    assert_eq!(report["valuation"]["currency"], "EUR");
    assert_eq!(report["valuation"]["prices"][0]["base"], "USD");
    assert_eq!(report["valuation"]["prices"][0]["rate"], "0.90");

    std::fs::remove_dir_all(directory).ok();
}

#[test]
fn check_reports_success_on_business_valid_file() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("pactole-check-ok-{}.pactole", std::process::id()));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte\n\
         2024-01-01 open Depenses:Divers\n\
         2024-01-01 commodity EUR\n\
         payee \"Test\"\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Actifs:Compte -100.00 EUR\n\
         \u{20}\u{20}Depenses:Divers 100.00 EUR\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK"));
}

#[test]
fn check_accepts_market_prices_and_reports_conflicting_quotes() {
    let directory =
        std::env::temp_dir().join(format!("pactole-check-price-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    std::fs::write(
        &ledger,
        "2024-01-01 commodity EUR\n\
         2024-01-01 commodity USD\n\
         2024-01-02 price USD 0.92 EUR\n",
    )
    .unwrap();

    let valid = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );

    std::fs::write(
        &ledger,
        "2024-01-01 commodity EUR\n\
         2024-01-01 commodity USD\n\
         2024-01-02 price USD 0.92 EUR\n\
         2024-01-02 price USD 0.93 EUR\n",
    )
    .unwrap();
    let conflicting = pactole_cmd()
        .args(["check", ledger.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");
    std::fs::remove_dir_all(&directory).ok();

    assert!(!conflicting.status.success());
    let json: serde_json::Value = serde_json::from_slice(&conflicting.stdout).unwrap();
    assert_eq!(json["diagnostics"][0]["code"], "price-conflict");
}

#[test]
fn check_reports_undeclared_payee() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-payee-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte\n\
         2024-01-01 open Depenses:Divers\n\
         2024-01-01 commodity EUR\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Actifs:Compte -100.00 EUR\n\
         \u{20}\u{20}Depenses:Divers 100.00 EUR\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("not declared"));
}

#[test]
fn check_reports_the_source_location_of_a_business_error() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-location-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Assets:Checking\n\
         2024-02-01 open Assets:Checking\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(":2:17: error:"), "stderr: {stderr}");
    assert!(stderr.contains("already open"), "stderr: {stderr}");
}

#[test]
fn check_reports_all_independent_business_errors() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-all-errors-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Assets:Checking\n\
         2024-02-01 open Assets:Checking\n\
         2024-01-01 commodity EUR\n\
         2024-02-01 commodity EUR\n\
         payee \"Carrefour\"\n\
         payee \"Carrefour\"\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches(" error: ").count(), 3, "stderr: {stderr}");
    assert!(stderr.contains("already open"), "stderr: {stderr}");
    assert!(stderr.contains("already declared"), "stderr: {stderr}");
}

#[test]
fn check_reports_the_business_errors_of_the_valid_files_of_a_broken_project() {
    // A broken file must not hide the business errors of the other files:
    // the checks that do not depend on the missing content still run.
    let dir = std::env::temp_dir().join(format!(
        "pactole-check-partial-{}-{}",
        std::process::id(),
        "project"
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.pactole");
    let broken = dir.join("broken.pactole");
    let valid = dir.join("valid.pactole");

    std::fs::write(
        &main,
        "include \"broken.pactole\"\ninclude \"valid.pactole\"\n",
    )
    .unwrap();
    // The date is not a calendar date: the directive cannot be lowered.
    std::fs::write(&broken, "2024-13-45 open Assets:Checking\n").unwrap();
    std::fs::write(
        &valid,
        "2024-01-01 commodity EUR\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Assets:Checking -100.00 EUR\n\
         \u{20}\u{20}Expenses:Divers 100.00 EUR\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", main.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&dir).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The broken file is reported as such...
    assert!(stderr.contains("lowering-error"), "stderr: {stderr}");
    assert!(stderr.contains("analysis-incomplete"), "stderr: {stderr}");
    // ...and the business error of the valid file is still reported.
    assert!(stderr.contains("not declared"), "stderr: {stderr}");
}

#[test]
fn payee_declared_after_its_first_use_is_still_accepted() {
    // A `payee` declaration carries no date, so it is not checked
    // chronologically: declaring it anywhere in the file, even after the
    // transaction using it, must be accepted.
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-payee-order-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte\n\
         2024-01-01 open Depenses:Divers\n\
         2024-01-01 commodity EUR\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Actifs:Compte -100.00 EUR\n\
         \u{20}\u{20}Depenses:Divers 100.00 EUR\n\
         payee \"Test\"\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_business_validation_error_that_parse_does_not_catch() {
    // The shipped fixture parses fine grammatically (that's what
    // `parse_prints_journal_entries` checks above), but it is not valid
    // from a business point of view: `comptes/2026.pactole` asserts a
    // balance for `Actifs:Compte-Joint` on 2026-01-01, before that account
    // is opened on 2026-09-03 in `test.pactole`.
    let output = pactole_cmd()
        .args(["check", SAMPLE_FILE])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("not open"));
}

#[test]
fn check_cycle_is_a_warning_unless_strict() {
    let dir = std::env::temp_dir().join(format!("pactole-check-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.pactole");
    let other = dir.join("other.pactole");
    std::fs::write(&main, "include \"other.pactole\"\n").unwrap();
    std::fs::write(&other, "include \"main.pactole\"\n").unwrap();

    let normal = pactole_cmd()
        .args(["check", main.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(normal.status.success());
    assert!(String::from_utf8_lossy(&normal.stdout).contains("OK"));
    assert!(String::from_utf8_lossy(&normal.stderr).contains("warning:"));
    assert!(String::from_utf8_lossy(&normal.stderr).contains("include-cycle"));

    let strict = pactole_cmd()
        .args(["check", "--strict", main.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(!strict.status.success());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_inactive_poptag_is_a_warning_unless_strict() {
    let path = std::env::temp_dir().join(format!(
        "pactole-check-inactive-poptag-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2026-01-01 commodity EUR\n\
2026-01-01 open Assets:Checking\n\
2026-01-01 open Expenses:Food\n\
payee \"Shop\"\n\
poptag #missing\n\
2026-01-02 * \"Shop\"\n\
\u{20}\u{20}Expenses:Food 10 EUR\n\
\u{20}\u{20}Assets:Checking -10 EUR\n",
    )
    .unwrap();

    let normal = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(normal.status.success());
    assert!(String::from_utf8_lossy(&normal.stderr).contains("tag-not-active"));

    let strict = pactole_cmd()
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(!strict.status.success());

    std::fs::remove_file(&path).ok();
}

#[test]
fn check_reports_error_on_missing_file() {
    let output = pactole_cmd()
        .args(["check", "this-file-does-not-exist.pactole"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

#[test]
fn check_reports_balance_assertion_mismatch() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-balance-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte\n\
         2024-01-01 open Depenses:Divers\n\
         2024-01-01 commodity EUR\n\
         payee \"Test\"\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Actifs:Compte -100.00 EUR\n\
         \u{20}\u{20}Depenses:Divers 100.00 EUR\n\
         2024-01-10 balance Actifs:Compte -50.00 EUR\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("balance assertion failed"));
}

#[test]
fn check_reports_commodity_not_allowed_on_restricted_account() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-check-restricted-{}.pactole",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte USD\n\
         2024-01-01 open Depenses:Divers\n\
         2024-01-01 commodity EUR\n\
         payee \"Test\"\n\
         2024-01-05 * \"Test\"\n\
         \u{20}\u{20}Actifs:Compte -100.00 EUR\n\
         \u{20}\u{20}Depenses:Divers 100.00 EUR\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("not allowed"));
}

#[test]
fn fmt_prints_canonical_form_to_stdout() {
    let output = pactole_cmd()
        .args(["fmt", SAMPLE_FILE])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Depenses:Alimentation                  45.30 EUR\n"));
    assert!(stdout.contains("Actifs:Compte-Joint                   -45.30 EUR\n"));
}

#[test]
fn fmt_reads_from_stdin_when_file_is_dash() {
    use std::io::Write;

    let mut child = pactole_cmd()
        .args(["fmt", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run pactole-cli");

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"2026-01-01   commodity   EUR\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "2026-01-01 commodity EUR\n"
    );
}

#[test]
fn completions_generates_a_bash_script() {
    let output = pactole_cmd()
        .args(["completions", "bash"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_pactole"));
    assert!(stdout.contains("completions"));
}

#[test]
fn fmt_reports_error_on_syntax_error() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("pactole-fmt-test-{}.pactole", std::process::id()));
    std::fs::write(&path, "not a valid pactole file\n").unwrap();

    let output = pactole_cmd()
        .args(["fmt", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

fn write_register_fixture() -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-register-test-{}-{}.pactole",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    std::fs::write(
        &path,
        "2024-01-01 open Actifs:Compte\n\
         2024-01-01 open Depenses:Alimentation\n\
         2024-01-01 open Depenses:Loisirs\n\
         2024-01-01 commodity EUR\n\
         payee \"Carrefour\"\n\
         payee \"Cinema\"\n\
         2024-01-05 * \"Carrefour\" \"Courses\"\n\
         \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
         \u{20}\u{20}Actifs:Compte -45.30 EUR\n\
         2024-01-10 ! \"Cinema\"\n\
         \u{20}\u{20}Depenses:Loisirs 12.00 EUR\n\
         \u{20}\u{20}Actifs:Compte -12.00 EUR\n",
    )
    .unwrap();
    path
}

#[test]
fn register_lists_every_posting_by_default() {
    let path = write_register_fixture();

    let output = pactole_cmd()
        .args(["register", path.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Carrefour"));
    assert!(stdout.contains("Cinema"));
    assert!(stdout.contains("Depenses:Alimentation"));
    assert!(stdout.contains("Actifs:Compte"));
}

#[test]
fn register_supports_json_with_exact_decimal_amounts() {
    let path = write_register_fixture();

    let output = pactole_cmd()
        .args(["register", path.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json[0]["date"], "2024-01-05");
    assert_eq!(json[0]["status"], "cleared");
    assert_eq!(json[0]["amount"]["number"], "45.30");
    assert_eq!(json[0]["amount"]["commodity"], "EUR");
    assert_eq!(json[0]["balance"][0]["number"], "45.30");
}

#[test]
fn check_supports_json_diagnostics() {
    let directory = std::env::temp_dir().join(format!("pactole-check-json-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let ledger = directory.join("ledger.pactole");
    std::fs::write(&ledger, "not a valid directive\n").unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&directory).ok();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert!(!json["diagnostics"].as_array().unwrap().is_empty());
    assert!(json["diagnostics"][0]["code"].is_string());
    assert!(json["diagnostics"][0]["range"]["start"]["line"].is_number());
}

#[test]
fn check_json_success_has_no_diagnostics() {
    let path = write_register_fixture();

    let output = pactole_cmd()
        .args(["check", path.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["diagnostics"].as_array().unwrap().len(), 0);
}

#[test]
fn graph_reports_repeated_includes_cycles_and_unreachable_files() {
    let directory = std::env::temp_dir().join(format!("pactole-graph-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.join("root.pactole");
    std::fs::write(
        &root,
        "include \"accounts.pactole\"\ninclude \"accounts.pactole\"\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("accounts.pactole"),
        "include \"root.pactole\"\n",
    )
    .unwrap();
    std::fs::write(directory.join("orphan.pactole"), "\n").unwrap();

    let output = pactole_cmd()
        .args(["graph", root.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&directory).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["cycles"].as_array().unwrap().len(), 1);
    assert!(json["edges"].as_array().unwrap().iter().any(|edge| {
        edge["repeated_target"] == true
            && edge["to"].as_str().unwrap().ends_with("accounts.pactole")
    }));
    assert!(
        json["unreachable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| { path.as_str().unwrap().ends_with("orphan.pactole") })
    );
}

#[test]
fn graph_reports_missing_includes_as_errors_in_json() {
    let directory =
        std::env::temp_dir().join(format!("pactole-graph-missing-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.join("root.pactole");
    std::fs::write(&root, "include \"missing.pactole\"\n").unwrap();

    let output = pactole_cmd()
        .args(["graph", root.to_str().unwrap(), "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&directory).ok();

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["diagnostics"][0]["code"], "include-not-found");
}

#[test]
fn graph_resolves_mixed_and_parent_relative_paths() {
    let directory =
        std::env::temp_dir().join(format!("pactole-graph-paths-{}", std::process::id()));
    let working = directory.join("working");
    std::fs::create_dir_all(&working).unwrap();
    let root = directory.join("root.pactole");
    std::fs::write(&root, "include \"included.pactole\"\n").unwrap();
    std::fs::write(directory.join("included.pactole"), "\n").unwrap();
    std::fs::write(directory.join("orphan.pactole"), "\n").unwrap();

    let absolute_root_relative_scan = pactole_cmd()
        .current_dir(&working)
        .args([
            "graph",
            root.to_str().unwrap(),
            "--scan",
            "..",
            "--format",
            "json",
        ])
        .output()
        .expect("failed to run pactole-cli");
    let parent_relative_root_absolute_scan = pactole_cmd()
        .current_dir(&working)
        .args([
            "graph",
            "../root.pactole",
            "--scan",
            directory.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&directory).ok();

    for output in [
        absolute_root_relative_scan,
        parent_relative_root_absolute_scan,
    ] {
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["ok"], true);
        assert!(!Path::new(json["root"].as_str().unwrap()).is_absolute());
        assert_eq!(json["unreachable"].as_array().unwrap().len(), 1);
        assert!(
            json["unreachable"][0]
                .as_str()
                .unwrap()
                .ends_with("orphan.pactole")
        );
        assert!(json["files"].as_array().unwrap().iter().any(|file| {
            file["state"] == "reachable"
                && file["path"].as_str().unwrap().ends_with("included.pactole")
        }));
        assert!(
            json["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|file| { !Path::new(file["path"].as_str().unwrap()).is_absolute() })
        );
    }
}

#[test]
fn graph_accepts_a_filename_without_a_parent_directory() {
    let directory =
        std::env::temp_dir().join(format!("pactole-graph-bare-file-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("ledger.pactole"), "\n").unwrap();

    let output = pactole_cmd()
        .current_dir(&directory)
        .args(["graph", "ledger.pactole", "--format", "json"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_dir_all(&directory).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn register_filters_by_account() {
    let path = write_register_fixture();

    let output = pactole_cmd()
        .args([
            "register",
            path.to_str().unwrap(),
            "--account",
            "Depenses:Alimentation",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Carrefour"));
    assert!(!stdout.contains("Cinema"));
}

#[test]
fn register_filters_by_status_and_payee() {
    let path = write_register_fixture();

    let output = pactole_cmd()
        .args([
            "register",
            path.to_str().unwrap(),
            "--status",
            "pending",
            "--payee",
            "cinema",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&path).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Cinema"));
    assert!(!stdout.contains("Carrefour"));
}

#[test]
fn register_reports_error_on_missing_file() {
    let output = pactole_cmd()
        .args(["register", "this-file-does-not-exist.pactole"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

/// A minimal OFX 2.x (XML) bank statement with two transactions.
const IMPORT_OFX_FIXTURE: &str = "<?OFX OFXHEADER=\"200\" VERSION=\"220\" SECURITY=\"NONE\" OLDFILEUID=\"NONE\" NEWFILEUID=\"NONE\"?>\n\
<OFX>\n\
<SIGNONMSGSRSV1>\n\
<SONRS>\n\
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>\n\
<DTSERVER>20260903120000</DTSERVER>\n\
<LANGUAGE>ENG</LANGUAGE>\n\
</SONRS>\n\
</SIGNONMSGSRSV1>\n\
<BANKMSGSRSV1>\n\
<STMTTRNRS>\n\
<TRNUID>1</TRNUID>\n\
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>\n\
<STMTRS>\n\
<CURDEF>EUR</CURDEF>\n\
<BANKACCTFROM><BANKID>123456789</BANKID><ACCTID>00012345678</ACCTID><ACCTTYPE>CHECKING</ACCTTYPE></BANKACCTFROM>\n\
<BANKTRANLIST>\n\
<DTSTART>20260901</DTSTART>\n\
<DTEND>20260930</DTEND>\n\
<STMTTRN>\n\
<TRNTYPE>DEBIT</TRNTYPE>\n\
<DTPOSTED>20260903</DTPOSTED>\n\
<TRNAMT>-45.30</TRNAMT>\n\
<FITID>2026090300001</FITID>\n\
<NAME>CARREFOUR</NAME>\n\
<MEMO>Courses</MEMO>\n\
</STMTTRN>\n\
<STMTTRN>\n\
<TRNTYPE>CREDIT</TRNTYPE>\n\
<DTPOSTED>20260905</DTPOSTED>\n\
<TRNAMT>2000.00</TRNAMT>\n\
<FITID>2026090500002</FITID>\n\
<NAME>ACME CORP</NAME>\n\
<MEMO>Salary</MEMO>\n\
</STMTTRN>\n\
</BANKTRANLIST>\n\
</STMTRS>\n\
</STMTTRNRS>\n\
</BANKMSGSRSV1>\n\
</OFX>\n";

const IMPORT_RULES_FIXTURE: &str = "[import]\n\
bank_account = \"Actifs:Banque\"\n\
currency = \"EUR\"\n\
default_counter_account = \"Depenses:A-Classer\"\n\
\n\
[[import.rules]]\n\
payee_contains = \"carrefour\"\n\
counter_account = \"Depenses:Alimentation\"\n\
payee = \"Carrefour\"\n";

/// Import configuration whose `[import.accounts]` table maps the fixture's
/// statement account (`ACCTID 00012345678`) to a dedicated Pactole account.
const IMPORT_ACCOUNTS_FIXTURE: &str = "[import]\n\
bank_account = \"Actifs:Banque\"\n\
currency = \"EUR\"\n\
default_counter_account = \"Depenses:A-Classer\"\n\
\n\
[import.accounts]\n\
\"00012345678\" = \"Actifs:Courant\"\n";

/// Import configuration whose `[import.accounts]` table only maps an account the
/// fixture's statement does not use, leaving it unmapped.
const IMPORT_UNMAPPED_FIXTURE: &str = "[import]\n\
bank_account = \"Actifs:Banque\"\n\
currency = \"EUR\"\n\
default_counter_account = \"Depenses:A-Classer\"\n\
\n\
[import.accounts]\n\
\"99999999999\" = \"Actifs:Epargne\"\n";

/// Legacy SGML statement whose payee and narration carry raw windows-1252
/// bytes (`\xC9` = É, `\xE0` = à), as declared by its `CHARSET:1252` header.
const IMPORT_CP1252_FIXTURE: &[u8] = b"OFXHEADER:100\n\
DATA:OFXSGML\n\
VERSION:102\n\
SECURITY:NONE\n\
ENCODING:USASCII\n\
CHARSET:1252\n\
COMPRESSION:NONE\n\
OLDFILEUID:NONE\n\
NEWFILEUID:NONE\n\
\n\
<OFX>\n\
<SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO<DTSERVER>20261016120000<LANGUAGE>FRA</SONRS></SIGNONMSGSRSV1>\n\
<BANKMSGSRSV1><STMTTRNRS><TRNUID>1<STATUS><CODE>0<SEVERITY>INFO<STMTRS><CURDEF>EUR<BANKACCTFROM><BANKID>123456789<ACCTID>00012345678<ACCTTYPE>CHECKING<BANKTRANLIST><DTSTART>20261001<DTEND>20261016<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20261010<TRNAMT>-12.50<FITID>CP1252-CLI<NAME>CAF\xC9 L\xC9ON<MEMO>Croissants \xE0 la boulangerie</STMTTRN></BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1>\n\
</OFX>\n";

/// Write a uniquely named temporary file and return its path.
fn write_import_temp(name: &str, content: impl AsRef<[u8]>) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-import-{}-{}-{name}",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    std::fs::write(&path, content).unwrap();
    path
}

/// Asserts that `stdout` contains a posting line for `account` whose amount is
/// `amount`, regardless of the alignment padding in front of the amount.
fn assert_posting(stdout: &str, account: &str, amount: &str) {
    let mut expected = vec![account];
    expected.extend(amount.split_whitespace());
    let found = stdout.lines().any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        fields == expected
    });
    assert!(
        found,
        "no posting `{account} {amount}` in stdout:\n{stdout}"
    );
}

#[test]
fn import_ofx_prints_transactions_to_stdout() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The matched rule overrides the payee and picks the counter account.
    assert!(stdout.contains("2026-09-03 * \"Carrefour\" \"Courses\""));
    assert_posting(&stdout, "Depenses:Alimentation", "45.30 EUR");
    // The second transaction falls back to the default counter account.
    assert!(stdout.contains("2026-09-05 * \"ACME CORP\" \"Salary\""));
    assert_posting(&stdout, "Depenses:A-Classer", "-2000.00 EUR");
    // The imported account carries the signed source amount...
    assert_posting(&stdout, "Actifs:Banque", "-45.30 EUR");
    assert_posting(&stdout, "Actifs:Banque", "2000.00 EUR");
    // ... and each transaction keeps its qualified FITID for idempotent re-imports.
    assert!(stdout.contains("fitid: \"ofx:00012345678:2026090300001\""));
    assert!(stdout.contains("fitid: \"ofx:00012345678:2026090500002\""));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("imported 2 transaction(s), skipped 0"));
}

#[test]
fn import_ofx_filters_by_date_and_can_omit_the_imported_payee() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let config = write_import_temp(
        "import-options.toml",
        "[import]\n\
         bank_account = \"Actifs:Banque\"\n\
         currency = \"EUR\"\n\
         default_counter_account = \"Depenses:A-Classer\"\n\
         payee_metadata_key = \"imported_payee\"\n",
    );

    let output = pactole_cmd()
        .args([
            "--config",
            config.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--from",
            "2026-09-05",
            "--no-imported-payee",
            "--omit-negative-amount",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&config).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("2026-09-03 *"));
    assert!(stdout.contains("2026-09-05 *"));
    assert!(!stdout.contains("imported_payee:"));
    assert!(stdout.contains("fitid: \"ofx:00012345678:2026090500002\""));
    assert!(!stdout.contains("Actifs:Banque -2000.00 EUR"));
}

#[test]
fn import_ofx_maps_the_statement_account_through_the_configuration() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let config = write_import_temp("accounts.toml", IMPORT_ACCOUNTS_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            config.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&config).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Both transactions are posted to the account mapped from the
    // statement's ACCTID rather than to `bank_account`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_posting(&stdout, "Actifs:Courant", "-45.30 EUR");
    assert_posting(&stdout, "Actifs:Courant", "2000.00 EUR");
    assert!(!stdout.contains("Actifs:Banque"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("warning:"));
}

#[test]
fn import_ofx_warns_and_falls_back_for_an_unmapped_statement_account() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let config = write_import_temp("unmapped.toml", IMPORT_UNMAPPED_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            config.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&config).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The import still succeeds: transactions fall back to `bank_account`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_posting(&stdout, "Actifs:Banque", "-45.30 EUR");
    assert_posting(&stdout, "Actifs:Banque", "2000.00 EUR");

    // A single warning names the unmapped statement account.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning:"));
    assert!(stderr.contains("00012345678"));
}

#[test]
fn import_ofx_account_flag_overrides_the_configuration() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    // The configuration maps the statement account to `Actifs:Courant`:
    // `--account` must win over both the mapping and `bank_account`.
    let config = write_import_temp("accounts.toml", IMPORT_ACCOUNTS_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            config.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--account",
            "Actifs:Epargne",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&config).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_posting(&stdout, "Actifs:Epargne", "-45.30 EUR");
    assert_posting(&stdout, "Actifs:Epargne", "2000.00 EUR");
    assert!(!stdout.contains("Actifs:Banque"));
    assert!(!stdout.contains("Actifs:Courant"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("warning:"));
}

#[test]
fn import_ofx_account_flag_reports_an_invalid_account() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--account",
            "not an account",
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: invalid account name"));
}

#[test]
fn import_ofx_skips_transactions_already_in_the_target_file() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);
    let into = write_import_temp(
        "journal.pactole",
        "2026-09-03 * \"Carrefour\" \"Courses\"\n\
          \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
          \u{20}\u{20}Actifs:Banque -45.30 EUR\n\
          \u{20}\u{20}\u{20}\u{20}fitid: \"ofx:00012345678:2026090300001\"\n",
    );

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();
    std::fs::remove_file(&into).ok();

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The already-present transaction is gone; the other one remains.
    assert!(!stdout.contains("2026-09-03 *"));
    assert!(stdout.contains("2026-09-05 *"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("imported 1 transaction(s), skipped 1"));
}

#[test]
fn import_ofx_skips_transactions_already_in_an_included_file() {
    // The target journal only holds an `include`: the FITID it must
    // recognize lives in the included file, so FITID collection has to
    // follow the include rather than look at the entry file alone.
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);
    let included = write_import_temp(
        "included.pactole",
        "2026-09-03 * \"Carrefour\" \"Courses\"\n\
          \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
          \u{20}\u{20}Actifs:Banque -45.30 EUR\n\
          \u{20}\u{20}\u{20}\u{20}fitid: \"ofx:00012345678:2026090300001\"\n",
    );
    let into = write_import_temp(
        "journal.pactole",
        format!(
            "include \"{}\"\n",
            included.file_name().unwrap().to_str().unwrap()
        ),
    );

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();
    std::fs::remove_file(&included).ok();
    std::fs::remove_file(&into).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The transaction recorded in the included file is gone; the other one remains.
    assert!(!stdout.contains("2026-09-03 *"));
    assert!(stdout.contains("2026-09-05 *"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("imported 1 transaction(s), skipped 1"));
}

#[test]
fn import_ofx_decodes_windows_1252_sources() {
    // The statement is not valid UTF-8; reading it as a string would fail
    // before the parser ever sees it.
    let ofx = write_import_temp("statement-cp1252.ofx", IMPORT_CP1252_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("2026-10-10 * \"CAFÉ LÉON\" \"Croissants à la boulangerie\""),
        "stdout: {stdout}"
    );
    assert_posting(&stdout, "Depenses:A-Classer", "12.50 EUR");
}

#[test]
fn import_ofx_write_appends_to_the_target_file() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);
    let into = write_import_temp(
        "journal.pactole",
        "2026-09-03 * \"Carrefour\" \"Courses\"\n\
          \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
          \u{20}\u{20}Actifs:Banque -45.30 EUR\n\
          \u{20}\u{20}\u{20}\u{20}fitid: \"ofx:00012345678:2026090300001\"\n",
    );

    let output = pactole_cmd()
        .args([
            "--config",
            rules.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
            "--write",
        ])
        .output()
        .expect("failed to run pactole-cli");

    let written = std::fs::read_to_string(&into).unwrap();

    std::fs::remove_file(&ofx).ok();
    std::fs::remove_file(&rules).ok();
    std::fs::remove_file(&into).ok();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Nothing new on stdout when writing...
    assert!(String::from_utf8_lossy(&output.stdout).is_empty());
    // ... and the file keeps its original transaction plus the new one.
    assert!(written.contains("fitid: \"ofx:00012345678:2026090300001\""));
    assert!(written.contains("fitid: \"ofx:00012345678:2026090500002\""));
    assert!(written.contains("2026-09-05 * \"ACME CORP\" \"Salary\""));
}

#[test]
fn import_ofx_reports_missing_configuration() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let missing = std::env::temp_dir().join(format!(
        "pactole-import-missing-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    std::fs::remove_file(&missing).ok();

    let output = pactole_cmd()
        .args([
            "--config",
            missing.to_str().unwrap(),
            "import-ofx",
            ofx.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: cannot read configuration"));
}

#[test]
fn import_ofx_write_requires_into() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);

    let output = pactole_cmd()
        .args(["import-ofx", ofx.to_str().unwrap(), "--write"])
        .output()
        .expect("failed to run pactole-cli");

    std::fs::remove_file(&ofx).ok();

    assert!(!output.status.success());
}

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pactole-{}-{}-{name}",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn init_creates_a_valid_ledger_and_project_configuration() {
    let dir = unique_temp_dir("init");
    let ledger = dir.join("ledger.pactole");
    let config = dir.join("pactole.toml");

    let output = pactole_cmd()
        .args([
            "init",
            ledger.to_str().unwrap(),
            "--account",
            "Assets:Bank",
            "--currency",
            "USD",
            "--counter-account",
            "Expenses:Review",
            "--date",
            "2026-01-01",
        ])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("created ledger:"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("created configuration:"));

    let ledger_source = std::fs::read_to_string(&ledger).unwrap();
    assert!(ledger_source.contains("2026-01-01 commodity USD"));
    assert!(ledger_source.contains("2026-01-01 open Assets:Bank"));
    assert!(ledger_source.contains("2026-01-01 open Expenses:Review"));

    // The configuration is created next to the ledger and satisfies its own
    // schema (unknown or missing keys would make `check` fail).
    let config_source = std::fs::read_to_string(&config).unwrap();
    assert!(config_source.contains("[import]"));
    assert!(config_source.contains("bank_account = \"Assets:Bank\""));
    assert!(config_source.contains("currency = \"USD\""));
    assert!(config_source.contains("default_counter_account = \"Expenses:Review\""));

    let check = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");
    assert!(
        check.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&check.stdout), "OK\n");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_global_writes_into_the_configuration_directory() {
    let dir = unique_temp_dir("init-global");
    let ledger = dir.join("ledger.pactole");
    let xdg = dir.join("xdg");

    let output = pactole_cmd()
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["init", ledger.to_str().unwrap(), "--global", "--no-ledger"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(xdg.join("pactole").join("pactole.toml").is_file());
    assert!(!dir.join("pactole.toml").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_does_not_overwrite_existing_files() {
    let dir = unique_temp_dir("init-existing");
    let ledger = dir.join("ledger.pactole");
    let config = dir.join("pactole.toml");
    std::fs::write(&ledger, "; user ledger\n").unwrap();
    std::fs::write(&config, "# user configuration\n").unwrap();

    let output = pactole_cmd()
        .args(["init", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ledger already exists:"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("configuration already exists:"));
    assert_eq!(std::fs::read_to_string(&ledger).unwrap(), "; user ledger\n");
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "# user configuration\n"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// A journal whose payee is not declared, valid only under `payee = "warn"`.
const UNDECLARED_PAYEE_JOURNAL: &str = "2024-01-01 open Assets:Checking\n\
2024-01-01 open Expenses:Food\n\
2024-01-01 commodity EUR\n\
2024-01-05 * \"Unknown\"\n\
\u{20}\u{20}Assets:Checking -10 EUR\n\
\u{20}\u{20}Expenses:Food 10 EUR\n";

#[test]
fn check_fails_on_an_undeclared_payee_by_default() {
    let dir = unique_temp_dir("check-payee-default");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNDECLARED_PAYEE_JOURNAL).unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error: [payee-not-declared]"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_accepts_an_undeclared_payee_when_warned_but_strict_rejects_it() {
    let dir = unique_temp_dir("check-payee-warn");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNDECLARED_PAYEE_JOURNAL).unwrap();
    std::fs::write(dir.join("pactole.toml"), "[validation]\npayee = \"warn\"\n").unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning: [payee-not-declared]"));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "OK\n");

    let strict = pactole_cmd()
        .args(["check", ledger.to_str().unwrap(), "--strict"])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!strict.status.success());

    std::fs::remove_dir_all(&dir).ok();
}

/// A journal where every declaration is dead: both accounts, the commodity
/// and the payee are declared but nothing uses them.
const UNUSED_DECLARATIONS_JOURNAL: &str = "2024-01-01 open Assets:Checking\n\
2024-01-01 open Expenses:Food\n\
2024-01-01 commodity EUR\n\
payee \"Never\"\n";

#[test]
fn check_warns_about_unused_declarations_by_default_and_strict_rejects_them() {
    let dir = unique_temp_dir("check-unused");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNUSED_DECLARATIONS_JOURNAL).unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    // Unused declarations are warnings: they never block by themselves.
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "OK\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("warning: [account-unused]").count(),
        2,
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("warning: [commodity-unused]"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("warning: [payee-unused]"),
        "stderr: {stderr}"
    );

    let strict = pactole_cmd()
        .args(["check", "--strict", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!strict.status.success());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_ignores_unused_declarations_when_they_are_turned_off() {
    let dir = unique_temp_dir("check-unused-off");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNUSED_DECLARATIONS_JOURNAL).unwrap();
    std::fs::write(dir.join("pactole.toml"), "[validation]\nunused = \"off\"\n").unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("-unused]"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_reads_the_project_configuration_next_to_the_journal() {
    let dir = unique_temp_dir("check-project-config");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNDECLARED_PAYEE_JOURNAL).unwrap();
    std::fs::write(dir.join("pactole.toml"), "[validation]\npayee = \"warn\"\n").unwrap();

    let output = pactole_cmd()
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning: [payee-not-declared]"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_project_configuration_overrides_the_global_one() {
    let dir = unique_temp_dir("check-layered-config");
    let ledger = dir.join("ledger.pactole");
    std::fs::write(&ledger, UNDECLARED_PAYEE_JOURNAL).unwrap();

    // Global says `warn`, project says `error`: the project wins.
    let xdg = dir.join("xdg");
    std::fs::create_dir_all(xdg.join("pactole")).unwrap();
    std::fs::write(
        xdg.join("pactole").join("pactole.toml"),
        "[validation]\npayee = \"warn\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("pactole.toml"),
        "[validation]\npayee = \"error\"\n",
    )
    .unwrap();

    let output = pactole_cmd()
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(!output.status.success());

    // Removing the project layer falls back to the global `warn`.
    std::fs::remove_file(dir.join("pactole.toml")).unwrap();

    let output = pactole_cmd()
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["check", ledger.to_str().unwrap()])
        .output()
        .expect("failed to run pactole-cli");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn serve_exposes_the_reports_api_over_http() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    struct ChildGuard(std::process::Child);

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let directory = std::env::temp_dir().join(format!("pactole-serve-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let config = directory.join("pactole.toml");
    std::fs::write(&config, "").unwrap();

    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = pactole_cmd()
        .args([
            "--config",
            config.to_str().unwrap(),
            "serve",
            SAMPLE_FILE,
            "--port",
            &port.to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to run pactole-cli");
    let _guard = ChildGuard(child);

    let deadline = Instant::now() + Duration::from_secs(10);
    let response = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(mut stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                write!(
                    stream,
                    "GET /api/v1/capabilities HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                break response;
            }
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("server did not start: {error}"),
        }
    };

    std::fs::remove_dir_all(&directory).ok();

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"read\":true"), "{response}");
}
