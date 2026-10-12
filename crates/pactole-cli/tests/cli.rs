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

    // The shipped fixture is already in canonical form, so formatting it
    // must be a no-op.
    let expected = std::fs::read_to_string(SAMPLE_FILE).unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
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

const IMPORT_RULES_FIXTURE: &str = "bank_account = \"Actifs:Banque\"\n\
currency = \"EUR\"\n\
default_counter_account = \"Depenses:A-Classer\"\n\
\n\
[[rules]]\n\
payee_contains = \"carrefour\"\n\
counter_account = \"Depenses:Alimentation\"\n\
payee = \"Carrefour\"\n";

/// Write a uniquely named temporary file and return its path.
fn write_import_temp(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "pactole-import-{}-{}-{name}",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn import_ofx_prints_transactions_to_stdout() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);

    let output = pactole_cmd()
        .args([
            "import-ofx",
            ofx.to_str().unwrap(),
            "--config",
            rules.to_str().unwrap(),
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
    assert!(stdout.contains("Depenses:Alimentation 45.30 EUR"));
    // The second transaction falls back to the default counter account.
    assert!(stdout.contains("2026-09-05 * \"ACME CORP\" \"Salary\""));
    assert!(stdout.contains("Depenses:A-Classer -2000.00 EUR"));
    // The imported account carries the signed source amount...
    assert!(stdout.contains("Actifs:Banque -45.30 EUR"));
    assert!(stdout.contains("Actifs:Banque 2000.00 EUR"));
    // ... and each transaction keeps its FITID for idempotent re-imports.
    assert!(stdout.contains("fitid: 2026090300001"));
    assert!(stdout.contains("fitid: 2026090500002"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("imported 2 transaction(s), skipped 0"));
}

#[test]
fn import_ofx_skips_transactions_already_in_the_target_file() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);
    let into = write_import_temp(
        "journal.pactole",
        "2026-09-03 * \"Carrefour\" \"Courses\"\n\
         \u{20}\u{20}fitid: 2026090300001\n\
         \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
         \u{20}\u{20}Actifs:Banque -45.30 EUR\n",
    );

    let output = pactole_cmd()
        .args([
            "import-ofx",
            ofx.to_str().unwrap(),
            "--config",
            rules.to_str().unwrap(),
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
fn import_ofx_write_appends_to_the_target_file() {
    let ofx = write_import_temp("statement.ofx", IMPORT_OFX_FIXTURE);
    let rules = write_import_temp("rules.toml", IMPORT_RULES_FIXTURE);
    let into = write_import_temp(
        "journal.pactole",
        "2026-09-03 * \"Carrefour\" \"Courses\"\n\
         \u{20}\u{20}fitid: 2026090300001\n\
         \u{20}\u{20}Depenses:Alimentation 45.30 EUR\n\
         \u{20}\u{20}Actifs:Banque -45.30 EUR\n",
    );

    let output = pactole_cmd()
        .args([
            "import-ofx",
            ofx.to_str().unwrap(),
            "--config",
            rules.to_str().unwrap(),
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
    assert!(written.contains("fitid: 2026090300001"));
    assert!(written.contains("fitid: 2026090500002"));
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
            "import-ofx",
            ofx.to_str().unwrap(),
            "--config",
            missing.to_str().unwrap(),
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

#[test]
fn init_creates_a_valid_ledger_and_import_configuration() {
    let dir = std::env::temp_dir();
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    );
    let ledger = dir.join(format!("pactole-init-{suffix}.pactole"));
    let config = dir.join(format!("pactole-init-{suffix}.toml"));
    std::fs::remove_file(&ledger).ok();
    std::fs::remove_file(&config).ok();

    let output = pactole_cmd()
        .args([
            "init",
            ledger.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
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

    let config_source = std::fs::read_to_string(&config).unwrap();
    assert!(config_source.contains("bank_account = \"Assets:Bank\""));
    assert!(config_source.contains("currency = \"USD\""));
    assert!(config_source.contains("default_counter_account = \"Expenses:Review\""));

    std::fs::remove_file(&ledger).ok();
    std::fs::remove_file(&config).ok();
}

#[test]
fn init_does_not_overwrite_existing_files() {
    let dir = std::env::temp_dir();
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    );
    let ledger = dir.join(format!("pactole-init-existing-{suffix}.pactole"));
    let config = dir.join(format!("pactole-init-existing-{suffix}.toml"));
    std::fs::write(&ledger, "; user ledger\n").unwrap();
    std::fs::write(&config, "# user configuration\n").unwrap();

    let output = pactole_cmd()
        .args([
            "init",
            ledger.to_str().unwrap(),
            "--config",
            config.to_str().unwrap(),
        ])
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

    std::fs::remove_file(&ledger).ok();
    std::fs::remove_file(&config).ok();
}
