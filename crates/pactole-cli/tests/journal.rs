//! End-to-end tests for the `pactole` binary against the example journals
//! shipped at the root of the repository, under `fixtures/`.
//!
//! These complement the unit and crate-level tests: they check the commands
//! a user actually runs, on a realistic multi-file project, with the real
//! configuration layering in place.
//!
//! When the repository's fixtures are not available — for instance when the
//! crate is tested from a published tarball — the tests skip instead of
//! failing, so `cargo test` still passes outside a checkout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn pactole_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pactole"))
}

/// Returns the `fixtures` directory, or `None` when it is not available.
fn fixtures() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent()?.parent()?.to_path_buf();
    let fixtures = root.join("fixtures");
    if fixtures.join("journal/ledger.pactole").is_file() {
        Some(fixtures)
    } else {
        None
    }
}

fn run(args: &[&str]) -> Output {
    pactole_cmd().args(args).output().expect("run pactole")
}

/// Every `.pactole` file under `directory`, sorted, recursively.
fn pactole_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];

    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "pactole")
            {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

#[test]
fn check_accepts_the_example_journal_silently() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&["check", ledger.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "OK");
    assert!(
        output.stderr.is_empty(),
        "the example journal reports diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_accepts_the_example_journal_in_strict_mode() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&["check", ledger.to_str().unwrap(), "--strict"]);

    assert!(
        output.status.success(),
        "strict check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "OK");
}

#[test]
fn check_rejects_the_partial_example_and_reports_both_families() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal-partial/ledger.pactole");

    let output = run(&["check", ledger.to_str().unwrap()]);

    assert!(!output.status.success(), "the partial fixture must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in ["[syntax-error]", "[lowering-error]", "[payee-not-declared]"] {
        assert!(
            stderr.contains(expected),
            "missing {expected} in:\n{stderr}"
        );
    }
    // The lost `open` may have opened `Actifs:Carte`, so the check that
    // depends on it is suppressed.
    assert!(
        !stderr.contains("[account-not-open]"),
        "a check depending on the lost content was not suppressed:\n{stderr}"
    );
}

#[test]
fn fmt_is_a_no_op_on_the_example_journal() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    for file in pactole_files(&fixtures.join("journal")) {
        let output = run(&["fmt", file.to_str().unwrap()]);

        assert!(
            output.status.success(),
            "fmt failed on {}: {}",
            file.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            fs::read_to_string(&file).expect("the fixture is readable"),
            "{} is not in canonical form",
            file.display()
        );
    }
}

#[test]
fn fmt_write_keeps_a_canonical_file_unchanged() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let source = fixtures.join("journal/ledger.pactole");
    // Work on a copy: a buggy formatter must not corrupt the repository's
    // fixture before the assertion gets a chance to fail.
    let copy =
        std::env::temp_dir().join(format!("pactole-fmt-write-{}.pactole", std::process::id()));
    fs::copy(&source, &copy).expect("copy the fixture");

    let output = run(&["fmt", copy.to_str().unwrap(), "--write"]);
    let written = fs::read_to_string(&copy).ok();
    fs::remove_file(&copy).ok();

    assert!(
        output.status.success(),
        "fmt --write failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        written,
        fs::read_to_string(&source).ok(),
        "fmt --write rewrote an already canonical file"
    );
}

#[test]
fn register_reports_the_running_balance_of_the_current_account() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&[
        "register",
        ledger.to_str().unwrap(),
        "--account",
        "Actifs:Compte-Joint",
    ]);

    assert!(
        output.status.success(),
        "register failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // 15 transactions from the checking statement, plus the three monthly
    // transfers into the savings account.
    assert_eq!(lines.len(), 18, "unexpected register output:\n{stdout}");
    assert!(
        lines[0].contains("3200.00 EUR"),
        "unexpected first line: {:?}",
        lines[0]
    );
    assert!(
        lines
            .last()
            .expect("at least one line")
            .contains("4538.55 EUR"),
        "unexpected last line: {:?}",
        lines.last()
    );
}

#[test]
fn register_reports_the_balance_of_a_foreign_currency_account() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&[
        "register",
        ledger.to_str().unwrap(),
        "--account",
        "Actifs:Epargne-USD",
    ]);

    assert!(
        output.status.success(),
        "register failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last = stdout.lines().last().expect("at least one line");

    assert!(
        last.contains("723.50 USD"),
        "unexpected last line: {last:?}"
    );
}

/// The `[reports]` classification of the example journal comes from the
/// project `pactole.toml`, so a French-rooted ledger classifies cleanly.
#[test]
fn balance_reports_assets_and_equity_of_the_example_journal() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&["balance", ledger.to_str().unwrap(), "--date", "2026-03-31"]);

    assert!(
        output.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Balance sheet (as of 2026-03-31)"),
        "unexpected header:\n{stdout}"
    );
    for expected in [
        "Assets",
        "Total Assets",
        "6051.18 EUR, 723.50 USD",
        "Equity",
        "Total Equity",
        "Unclosed result",
        "6051.18 EUR, -276.50 USD",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("Unclassified"),
        "the French roots were not classified:\n{stdout}"
    );
}

#[test]
fn balance_describes_the_sections_in_json() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&[
        "balance",
        ledger.to_str().unwrap(),
        "--date",
        "2026-03-31",
        "--format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the output is valid JSON");

    assert_eq!(value["title"], "Balance sheet");
    assert_eq!(value["scope"]["kind"], "as_of");
    assert_eq!(value["scope"]["date"], "2026-03-31");

    let titles: Vec<&str> = value["sections"]
        .as_array()
        .expect("sections is an array")
        .iter()
        .map(|section| section["title"].as_str().expect("a section title"))
        .collect();
    assert_eq!(titles, ["Assets", "Liabilities", "Equity"]);

    let assets = subtotals(&value["sections"][0]["subtotal"]);
    let equity = subtotals(&value["sections"][2]["subtotal"]);
    assert_eq!(assets["EUR"], "6051.18");
    assert_eq!(assets["USD"], "723.50");
    assert_eq!(equity["EUR"], assets["EUR"]);
    assert_eq!(equity["USD"], assets["USD"]);
    assert!(
        value["totals"]
            .as_array()
            .expect("totals is an array")
            .is_empty(),
        "a balanced, fully classified journal has no report total"
    );
}

#[test]
fn balance_emits_a_section_column_in_csv() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&[
        "balance",
        ledger.to_str().unwrap(),
        "--date",
        "2026-03-31",
        "--format",
        "csv",
    ]);

    assert!(
        output.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.starts_with(
            "report,scope_kind,scope_start,scope_end,section,kind,label,depth,commodity,number\n"
        ),
        "unexpected header:\n{stdout}"
    );
    assert!(
        stdout.contains("Balance sheet,as_of,,2026-03-31,Assets,subtotal,,,EUR,6051.18"),
        "missing the assets subtotal:\n{stdout}"
    );
    assert!(
        stdout
            .contains("Balance sheet,as_of,,2026-03-31,Equity,line,Unclosed result,0,USD,-276.50"),
        "missing the unclosed result:\n{stdout}"
    );
}

#[test]
fn balance_folds_the_accounts_with_depth() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    let output = run(&[
        "balance",
        ledger.to_str().unwrap(),
        "--date",
        "2026-03-31",
        "--depth",
        "1",
    ]);

    assert!(
        output.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Actifs"),
        "missing the folded root:\n{stdout}"
    );
    assert!(
        !stdout.contains("Actifs:Compte-Joint"),
        "the depth was ignored:\n{stdout}"
    );
}

#[test]
fn balance_reports_the_roots_without_project_configuration_as_unclassified() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let ledger = fixtures.join("journal/ledger.pactole");

    // An explicit, empty configuration replaces the project layer, so the
    // default English roots no longer match the French journal.
    let config = std::env::temp_dir().join(format!(
        "pactole-balance-{}-{}.toml",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    fs::write(&config, "").expect("write the temporary configuration");

    let output = run(&[
        "--config",
        config.to_str().unwrap(),
        "balance",
        ledger.to_str().unwrap(),
        "--date",
        "2026-03-31",
    ]);
    fs::remove_file(&config).ok();

    assert!(
        output.status.success(),
        "balance failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Unclassified"),
        "the unknown roots were dropped:\n{stdout}"
    );
    assert!(
        stdout.contains("Actifs:Compte-Joint"),
        "the unclassified section lost its accounts:\n{stdout}"
    );
}

/// Maps the commodity of every amount of a JSON `amounts` array to its
/// decimal string.
fn subtotals(value: &serde_json::Value) -> std::collections::BTreeMap<String, String> {
    value
        .as_array()
        .expect("an amounts array")
        .iter()
        .map(|amount| {
            (
                amount["commodity"]
                    .as_str()
                    .expect("a commodity")
                    .to_string(),
                amount["number"].as_str().expect("a number").to_string(),
            )
        })
        .collect()
}
