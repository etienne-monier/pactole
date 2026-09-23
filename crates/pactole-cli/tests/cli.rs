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
