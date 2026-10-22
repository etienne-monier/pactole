//! Guards that keep the command-line reference in sync with the actual
//! binary.
//!
//! The reference page (`docs/src/content/docs/reference/cli.md`) and the
//! root `README.md` are the two places where the command-line surface is
//! described in prose. Both drift silently when a subcommand or an option is
//! added, so the tests below compare them with the `--help` output of the
//! binary built for this test.
//!
//! When the repository's markdown files are not available — for instance when
//! the crate is tested from a published tarball — the tests skip instead of
//! failing, so `cargo test` still passes outside a checkout.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path of the CLI reference page, relative to the repository root.
const CLI_REFERENCE: &str = "docs/src/content/docs/reference/cli.md";

/// Path of the user-facing README, relative to the repository root.
const README: &str = "README.md";

/// Returns the root of the repository, or `None` when the crate is not
/// checked out from a repository (published tarball, vendored copy, ...).
fn repo_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent()?.parent()?.to_path_buf();
    if root.join(CLI_REFERENCE).is_file() && root.join(README).is_file() {
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

/// Runs the binary built for this test and returns its standard output.
fn pactole_help(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_pactole"))
        .args(args)
        .output()
        .expect("failed to run pactole");

    assert!(
        output.status.success(),
        "`pactole {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Returns the lines of a clap help section (`Commands:`, `Options:`, ...).
///
/// The section starts at the header line and ends at the first blank line or
/// the first line that is not indented.
fn help_section<'a>(help: &'a str, header: &str) -> Vec<&'a str> {
    let lines = help
        .lines()
        .skip_while(|line| line.trim() != header)
        .skip(1);
    let mut section = Vec::new();

    for line in lines {
        if !line.starts_with(' ') || line.trim().is_empty() {
            break;
        }
        section.push(line);
    }

    section
}

/// Returns the sub-commands listed in the `Commands:` section of a help
/// output.
fn help_commands(help: &str) -> Vec<String> {
    help_section(help, "Commands:")
        .into_iter()
        // A command name is indented with exactly two spaces; the
        // continuation lines of a wrapped description are indented deeper.
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// Returns the long options listed in the `Options:` section of a help
/// output.
fn help_long_options(help: &str) -> BTreeSet<String> {
    let mut options = BTreeSet::new();

    for line in help_section(help, "Options:") {
        // clap separates the option specification from its description with
        // at least two spaces. When the description is long enough, it is
        // moved to the next line instead.
        let spec = line.trim_start().split("  ").next().unwrap_or_default();
        options.extend(option_names(spec));
    }

    options
}

/// Returns the long option names mentioned in a text.
fn option_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-' || bytes[end] == b'_')
            {
                end += 1;
            }
            if end > start {
                names.insert(text[start..end].to_string());
            }
            index = end;
        } else {
            index += 1;
        }
    }

    names
}

/// Returns the long options a markdown section shows to the reader, either in
/// an option table or in a code example.
///
/// The prose of a section is deliberately ignored: an option that is only
/// mentioned in passing is not really documented.
fn documented_options(section: &str) -> BTreeSet<String> {
    let mut options = BTreeSet::new();
    let mut in_code_block = false;

    for line in section.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if !(in_code_block || trimmed.starts_with('|')) {
            continue;
        }
        options.extend(option_names(trimmed));
    }

    options
}

/// Returns the body of a markdown section, given the text of its `##`
/// heading.
fn markdown_section(document: &str, heading: &str) -> String {
    let mut body = String::new();
    let mut inside = false;

    for line in document.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            if inside {
                break;
            }
            inside = title.trim() == heading;
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }

    body
}

#[test]
fn cli_help_matches_the_documented_subcommands() {
    let Some(reference) = read_repo_file(CLI_REFERENCE) else {
        // Not a repository checkout: nothing to compare against.
        return;
    };
    let Some(readme) = read_repo_file(README) else {
        return;
    };

    let commands = help_commands(&pactole_help(&["--help"]));

    // `help` is clap's built-in sub-command, not one of ours.
    let commands: Vec<&String> = commands.iter().filter(|name| *name != "help").collect();

    assert!(
        !commands.is_empty(),
        "no sub-command found in the help output"
    );

    for command in commands {
        assert!(
            reference.contains(&format!("`pactole {command}`")),
            "the sub-command `pactole {command}` has no section in {CLI_REFERENCE}"
        );
        assert!(
            readme.contains(&format!("pactole {command}")),
            "the sub-command `pactole {command}` is not mentioned in {README}"
        );
    }
}

#[test]
fn cli_help_matches_the_documented_options() {
    let Some(reference) = read_repo_file(CLI_REFERENCE) else {
        return;
    };

    // The options of the root command live in their own section, before the
    // sub-commands are listed.
    let root_section = markdown_section(&reference, "Global options");
    for option in help_long_options(&pactole_help(&["--help"])) {
        // `help` and `version` are clap's built-ins.
        if option == "help" || option == "version" {
            continue;
        }
        assert!(
            documented_options(&root_section).contains(&option),
            "the global option `--{option}` is not documented in the option table or in a \
             code example of the \"Global options\" section of {CLI_REFERENCE}"
        );
    }

    for command in help_commands(&pactole_help(&["--help"])) {
        if command == "help" {
            continue;
        }

        let help = pactole_help(&[&command, "--help"]);
        let section = markdown_section(&reference, &format!("`pactole {command}`"));

        assert!(
            !section.is_empty(),
            "the sub-command `pactole {command}` has no section in {CLI_REFERENCE}"
        );

        let documented = documented_options(&section);
        for option in help_long_options(&help) {
            if option == "help" {
                continue;
            }
            assert!(
                documented.contains(&option),
                "the option `--{option}` of `pactole {command}` is not documented in the \
                 option table or in a code example of its section of {CLI_REFERENCE}"
            );
        }
    }
}
