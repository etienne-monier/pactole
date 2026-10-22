//! Guards that keep the workspace documentation in sync with the code.
//!
//! Three things drift silently as the code evolves:
//!
//! - a new directive added to the grammar, and not documented;
//! - a new diagnostic code, and not documented;
//! - a new public item in a library crate, and not mentioned in `AGENTS.md`.
//!
//! The tests below compare the documentation with the sources of truth
//! (the generated grammar, the diagnostic code catalogue and the crates
//! themselves). When the repository's markdown files are not available — for
//! instance when the crate is tested from a published tarball — the tests
//! skip instead of failing, so `cargo test` still passes outside a checkout.

use std::path::{Path, PathBuf};

use pactole_storage_fs::DiagnosticCode;

/// Path of the directives reference, relative to the repository root.
const DIRECTIVES_REFERENCE: &str = "docs/src/content/docs/reference/directives.md";

/// Path of the diagnostics reference, relative to the repository root.
const DIAGNOSTICS_REFERENCE: &str = "docs/src/content/docs/reference/diagnostics.md";

/// Path of the architecture reference, relative to the repository root.
const AGENTS: &str = "AGENTS.md";

/// Path of the generated tree-sitter grammar, relative to the repository root.
const GRAMMAR_JSON: &str = "crates/tree-sitter-pactole/src/grammar.json";

/// The crates whose public items are part of the published API.
///
/// `pactole-cli` and `pactole-lsp` are binaries: their `pub` items are
/// internal to the crate and are not part of any public API.
const LIBRARY_CRATES: &[&str] = &[
    "pactole-core",
    "pactole-config",
    "pactole-syntax",
    "pactole-storage-fs",
    "pactole-import",
    "tree-sitter-pactole",
];

/// Returns the root of the repository, or `None` when the crate is not
/// checked out from a repository (published tarball, vendored copy, ...).
fn repo_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent()?.parent()?.to_path_buf();
    if root.join(AGENTS).is_file() && root.join(GRAMMAR_JSON).is_file() {
        Some(root)
    } else {
        None
    }
}

/// Reads a repository file, or returns `None` when the repository is not
/// available.
fn read_repo_file(relative: &str) -> Option<String> {
    let root = repo_root()?;
    std::fs::read_to_string(root.join(relative)).ok()
}

/// Returns every `.rs` file of a crate, recursively.
fn rust_files(crate_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![crate_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}

/// Returns the name of the directive a grammar rule stands for.
///
/// The grammar names the payee declaration `payee_declaration`, while the
/// language calls it `payee`.
fn directive_name(rule: &str) -> String {
    rule.strip_suffix("_declaration")
        .unwrap_or(rule)
        .to_string()
}

/// Returns the directives of the grammar, in the order of the `directive`
/// rule.
fn grammar_directives(grammar: &serde_json::Value) -> Vec<String> {
    let Some(members) = grammar
        .get("rules")
        .and_then(|rules| rules.get("directive"))
        .and_then(|directive| directive.get("members"))
        .and_then(|members| members.as_array())
    else {
        return Vec::new();
    };

    members
        .iter()
        .filter_map(|member| {
            // Each member is a sequence starting with the directive rule.
            let name = member
                .get("members")
                .and_then(|members| members.as_array())
                .and_then(|members| members.first())
                .and_then(|first| first.get("name"))
                .and_then(|name| name.as_str())?;
            Some(directive_name(name))
        })
        .collect()
}

/// Returns the top-level public items of a Rust source file.
///
/// Only the items declared at the module level are considered: the methods of
/// a public type are documented with the type itself.
fn public_items(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("pub ")?;
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            let name = rest
                .strip_prefix("fn ")
                .or_else(|| rest.strip_prefix("struct "))
                .or_else(|| rest.strip_prefix("enum "))
                .or_else(|| rest.strip_prefix("trait "))
                .or_else(|| rest.strip_prefix("type "))
                .or_else(|| rest.strip_prefix("const "))
                .or_else(|| rest.strip_prefix("static "))?;
            let name = name
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .next()?;
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

/// Returns whether a word appears in a text.
fn mentions(text: &str, word: &str) -> bool {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|token| token == word)
}

#[test]
fn every_directive_is_documented() {
    let Some(reference) = read_repo_file(DIRECTIVES_REFERENCE) else {
        return;
    };
    let Some(root) = repo_root() else {
        return;
    };
    let Ok(grammar) = std::fs::read_to_string(root.join(GRAMMAR_JSON)) else {
        return;
    };
    let Ok(grammar) = serde_json::from_str::<serde_json::Value>(&grammar) else {
        return;
    };

    let directives = grammar_directives(&grammar);
    assert!(
        !directives.is_empty(),
        "no directive found in the generated grammar"
    );

    for directive in directives {
        // Every directive has its own section.
        let has_section = reference.lines().any(|line| {
            line.strip_prefix("## ").is_some_and(|title| {
                title
                    .trim()
                    .trim_matches('`')
                    .eq_ignore_ascii_case(&directive)
            })
        });
        assert!(
            has_section,
            "the directive `{directive}` has no section in {DIRECTIVES_REFERENCE}"
        );

        // Every directive is listed in the overview table.
        let has_row = reference.lines().any(|line| {
            let Some(rest) = line.strip_prefix('|') else {
                return false;
            };
            let cell = rest.split('|').next().unwrap_or_default();
            cell.trim()
                .trim_matches('`')
                .eq_ignore_ascii_case(&directive)
        });
        assert!(
            has_row,
            "the directive `{directive}` is not listed in the overview table of \
             {DIRECTIVES_REFERENCE}"
        );
    }
}

#[test]
fn every_diagnostic_code_is_documented() {
    let Some(reference) = read_repo_file(DIAGNOSTICS_REFERENCE) else {
        return;
    };

    assert!(
        !DiagnosticCode::ALL.is_empty(),
        "the diagnostic code catalogue is empty"
    );

    for code in DiagnosticCode::ALL {
        assert!(
            reference.contains(&format!("`{}`", code.as_str())),
            "the diagnostic code `{}` is not documented in {DIAGNOSTICS_REFERENCE}",
            code.as_str()
        );
    }
}

#[test]
fn no_diagnostic_code_is_a_free_form_string() {
    let Some(root) = repo_root() else {
        return;
    };

    for crate_name in LIBRARY_CRATES {
        let crate_dir = root.join("crates").join(crate_name).join("src");
        if !crate_dir.is_dir() {
            continue;
        }

        for file in rust_files(&crate_dir) {
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };

            assert!(
                !source.contains("code: Option<String>"),
                "{} still declares an optional string diagnostic code; use DiagnosticCode",
                file.display()
            );
            assert!(
                !source.contains("code: Some(\""),
                "{} still assigns a free-form diagnostic code; use a DiagnosticCode variant",
                file.display()
            );
        }
    }
}

#[test]
fn every_public_api_item_is_mentioned_in_agents_md() {
    let Some(agents) = read_repo_file(AGENTS) else {
        return;
    };
    let Some(root) = repo_root() else {
        return;
    };

    for crate_name in LIBRARY_CRATES {
        let crate_dir = root.join("crates").join(crate_name);
        if !crate_dir.is_dir() {
            continue;
        }

        for file in rust_files(&crate_dir) {
            // The integration tests are not part of the public API.
            if file
                .components()
                .any(|component| component.as_os_str() == "tests")
            {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&file) else {
                continue;
            };

            for item in public_items(&source) {
                assert!(
                    mentions(&agents, &item),
                    "the public item `{item}` ({}), defined in {crate_name}, is not mentioned \
                     in {AGENTS}",
                    file.display()
                );
            }
        }
    }
}
