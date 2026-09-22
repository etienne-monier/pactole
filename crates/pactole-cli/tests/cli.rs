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
