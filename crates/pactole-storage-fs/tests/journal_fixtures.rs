//! End-to-end tests on the example journals that live at the root of the
//! repository, under `fixtures/`.
//!
//! `fixtures/journal` is a realistic, multi-file project: it must stay
//! completely clean — no diagnostic, no partial analysis, canonical
//! formatting — so that it also serves as a reference for users.
//!
//! `fixtures/journal-partial` is its deliberately broken sibling: one file
//! cannot be analyzed, and the tests check that the business diagnostics of
//! the other files survive that loss while the checks depending on the lost
//! content are suppressed.
//!
//! When the repository's fixtures are not available — for instance when the
//! crate is tested from a published tarball — the tests skip instead of
//! failing, so `cargo test` still passes outside a checkout.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pactole_core::Entry;
use pactole_storage_fs::{
    AnalyzedProject, Diagnostic, DiagnosticCode, DiagnosticSource, FsSourceLoader,
    analyze_file_with_loader, format,
};
use rust_decimal::Decimal;

/// Returns the root of the repository, or `None` when the crate is not
/// checked out from a repository (published tarball, vendored copy, ...).
fn repo_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent()?.parent()?.to_path_buf();
    if root.join("fixtures/journal/ledger.pactole").is_file() {
        Some(root)
    } else {
        None
    }
}

/// Returns the `fixtures` directory, or `None` when it is not available.
fn fixtures() -> Option<PathBuf> {
    repo_root().map(|root| root.join("fixtures"))
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

fn analyze(entry: &Path) -> AnalyzedProject {
    analyze_file_with_loader(entry, &FsSourceLoader)
        .unwrap_or_else(|error| panic!("{} could not be analyzed: {error}", entry.display()))
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn the_example_journal_has_no_diagnostic() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    let project = analyze(&fixtures.join("journal/ledger.pactole"));
    let diagnostics = project.diagnostics();

    assert!(
        diagnostics.is_empty(),
        "the example journal reports diagnostics: {diagnostics:#?}"
    );
    assert!(
        !project.is_partial(),
        "the example journal is partial: {:#?}",
        project.partial()
    );
    assert!(
        project.validated_journal().is_ok(),
        "the example journal does not validate"
    );
}

#[test]
fn the_example_journal_resolves_every_file_it_contains() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let directory = fixtures.join("journal");

    let project = analyze(&directory.join("ledger.pactole"));
    let files = pactole_files(&directory);

    assert_eq!(
        project.files().len(),
        files.len(),
        "a file of the fixture is not reachable through the include graph"
    );
    for file in &files {
        assert!(
            project
                .files()
                .iter()
                .any(|analyzed| analyzed.path == *file),
            "{} was not analyzed",
            file.display()
        );
    }
}

#[test]
fn the_example_journal_is_in_canonical_form() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    for file in pactole_files(&fixtures.join("journal")) {
        let source = fs::read_to_string(&file).expect("the fixture is readable");
        let formatted = format(&source)
            .unwrap_or_else(|error| panic!("{} does not format: {error}", file.display()));

        assert_eq!(
            formatted,
            source,
            "{} is not in canonical form; run `pactole fmt --write`",
            file.display()
        );
    }
}

#[test]
fn the_example_journal_balances_on_its_declared_accounts() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    let project = analyze(&fixtures.join("journal/ledger.pactole"));
    let journal = project.validated_journal().expect("the fixture validates");

    // Every transaction is balanced and therefore contributes exactly zero
    // per commodity once summed over all its postings.
    let mut totals: BTreeMap<String, Decimal> = BTreeMap::new();
    for entry in &journal.entries {
        let Entry::Transaction(transaction) = entry else {
            continue;
        };
        let mut transaction_total = Decimal::ZERO;
        for posting in &transaction.postings {
            let amount = posting
                .amount
                .as_ref()
                .expect("validation inferred every missing amount");
            transaction_total += amount.number;
            *totals
                .entry(amount.commodity.as_str().to_string())
                .or_default() += amount.number;
        }
        assert!(
            transaction_total.is_zero(),
            "a transaction of the fixture is not balanced: {transaction:?}"
        );
    }

    assert!(
        !totals.is_empty(),
        "the fixture should hold at least one amount"
    );
    assert!(
        totals.values().all(|total| total.is_zero()),
        "the fixture does not balance overall: {totals:?}"
    );
}

#[test]
fn the_partial_example_keeps_the_business_diagnostics_of_the_valid_files() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    let project = analyze(&fixtures.join("journal-partial/ledger.pactole"));
    let diagnostics = project.diagnostics();

    assert!(project.is_partial(), "the partial fixture must be partial");
    let partial = project.partial().expect("the project is partial");
    let loss = partial
        .files
        .iter()
        .find(|loss| loss.path.ends_with("broken.pactole"))
        .expect("the broken file is reported as a loss");
    assert_eq!(loss.lost_directives, 2);

    let codes = codes(&diagnostics);
    for expected in [
        DiagnosticCode::SyntaxError,
        DiagnosticCode::LoweringError,
        DiagnosticCode::AnalysisIncomplete,
        DiagnosticCode::PayeeNotDeclared,
    ] {
        assert!(
            codes.contains(&expected),
            "missing {expected:?} in {codes:?}"
        );
    }

    // The undeclared payee lives in the *valid* file: a broken file must not
    // hide the checks that do not depend on it.
    let payee = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == DiagnosticCode::PayeeNotDeclared)
        .expect("the valid file still reports its undeclared payee");
    assert!(payee.path.ends_with("valid.pactole"));
    assert_eq!(payee.source, DiagnosticSource::Business);

    // The lost `open` may have opened `Actifs:Carte`, so the pairing of that
    // account with a transaction must not be reported as unopened.
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::AccountNotOpen),
        "the checks depending on the lost content were not suppressed: {diagnostics:#?}"
    );
}

#[test]
fn the_partial_example_cannot_be_validated_but_can_be_analyzed() {
    let Some(fixtures) = fixtures() else {
        return;
    };

    let project = analyze(&fixtures.join("journal-partial/ledger.pactole"));

    assert!(
        project.validated_journal().is_err(),
        "a project with errors must not validate"
    );
    // The structural journal still contains the entries of the valid files.
    assert!(
        project.journal().is_err(),
        "a project with a syntax error has no structural journal"
    );
    assert!(
        !project.files().is_empty(),
        "the analysis still visited the files"
    );
}

#[test]
fn the_partial_example_is_canonical_except_for_the_broken_file() {
    let Some(fixtures) = fixtures() else {
        return;
    };
    let directory = fixtures.join("journal-partial");

    for file in pactole_files(&directory) {
        if file
            .file_name()
            .is_some_and(|name| name == "broken.pactole")
        {
            // Deliberately not canonical: it cannot be formatted at all.
            continue;
        }
        let source = fs::read_to_string(&file).expect("the fixture is readable");
        let formatted = format(&source)
            .unwrap_or_else(|error| panic!("{} does not format: {error}", file.display()));
        assert_eq!(formatted, source, "{} is not canonical", file.display());
    }
}
