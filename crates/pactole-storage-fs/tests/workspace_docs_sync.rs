//! Guards that keep the workspace documentation in sync with the code.
//!
//! Three things drift silently as the code evolves:
//!
//! - a new directive added to the grammar, and not documented;
//! - a new diagnostic code, and not documented;
//! - a new public item in a library crate, and not mentioned in `AGENTS.md`.
//!
//! The tests below compare the documentation with the sources of truth
//! (the generated grammar, the diagnostic code mappings and the crates
//! themselves). When the repository's markdown files are not available — for
//! instance when the crate is tested from a published tarball — the tests
//! skip instead of failing, so `cargo test` still passes outside a checkout.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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

/// Returns the string literals of a Rust source file.
fn string_literals(source: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        literals.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }

    literals
}

/// Returns the body of the block starting at `start` (the index of its
/// opening brace).
fn block_at(source: &str, start: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;

    for (index, byte) in bytes.iter().enumerate().skip(start) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[start..=index]);
                }
            }
            _ => {}
        }
    }

    None
}

/// Returns the stable diagnostic codes produced by a source file.
///
/// The codes are the string literals of the `code()` mapping functions and
/// of the `code: Some("...")` assignments.
fn diagnostic_codes(source: &str) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();

    let mut rest = source;
    while let Some(index) = rest.find("fn code(self) -> &'static str") {
        let after = &rest[index..];
        let Some(open) = after.find('{') else {
            break;
        };
        if let Some(body) = block_at(after, open) {
            for literal in string_literals(body) {
                codes.insert(literal);
            }
        }
        rest = &rest[index + 1..];
    }

    let mut rest = source;
    while let Some(index) = rest.find("code: Some(\"") {
        let after = &rest[index + "code: Some(\"".len()..];
        let Some(end) = after.find('"') else {
            break;
        };
        codes.insert(after[..end].to_string());
        rest = &rest[index + 1..];
    }

    codes
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
    let Some(root) = repo_root() else {
        return;
    };

    let Ok(source) =
        std::fs::read_to_string(root.join("crates/pactole-storage-fs/src/analysis.rs"))
    else {
        return;
    };

    let codes = diagnostic_codes(&source);
    assert!(!codes.is_empty(), "no diagnostic code found in the source");

    for code in codes {
        assert!(
            reference.contains(&format!("`{code}`")),
            "the diagnostic code `{code}` is not documented in {DIAGNOSTICS_REFERENCE}"
        );
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
