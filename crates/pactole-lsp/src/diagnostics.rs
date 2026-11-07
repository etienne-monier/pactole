//! Builds LSP [`lsp_types::Diagnostic`]s from `pactole-storage-fs`'s
//! positioned analysis results (syntax, lowering, include, and business
//! diagnostics), converting spans to LSP ranges via
//! [`crate::conversion`].

use std::collections::HashMap;
use std::ops::Range as ByteRange;
use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use pactole_config::{ConfigKeyPath, ConfigPathSegment};
use pactole_storage_fs::{
    AnalyzedProject, Diagnostic as ProjectDiagnostic,
    DiagnosticSeverity as ProjectDiagnosticSeverity, DiagnosticSource, ParsedFile,
    check_config_references,
};
use toml_edit::{Document, Item, Table, Value};

use crate::conversion::{byte_range_to_range, span_to_range};

/// Builds the diagnostics for a single, standalone file (as produced by
/// [`pactole_storage_fs::analyze_file`]): its syntax diagnostics followed by
/// its lowering diagnostics, in that order.
///
/// This deliberately does not include include or business diagnostics: a
/// standalone [`ParsedFile`] has no project context in which those checks
/// would be sound.
pub fn standalone_diagnostics(file: &ParsedFile, path: &std::path::Path) -> Vec<Diagnostic> {
    let source = file.document().source();
    file.diagnostics(path)
        .iter()
        .map(|diagnostic| to_lsp_diagnostic(diagnostic, source))
        .collect()
}

/// Converts a positioned project diagnostic into an LSP diagnostic.
fn to_lsp_diagnostic(diagnostic: &ProjectDiagnostic, source: &str) -> Diagnostic {
    Diagnostic {
        range: span_to_range(source, &diagnostic.span),
        severity: Some(match diagnostic.severity {
            ProjectDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            ProjectDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code.as_str().to_string(),
        )),
        source: Some(
            match diagnostic.source {
                DiagnosticSource::Syntax => "pactole-syntax",
                DiagnosticSource::Lowering => "pactole-lowering",
                DiagnosticSource::Include => "pactole-include",
                DiagnosticSource::Business => "pactole-business",
                DiagnosticSource::Analysis => "pactole-analysis",
            }
            .to_string(),
        ),
        message: diagnostic.message.clone(),
        ..Default::default()
    }
}

/// Builds the diagnostics for every file reached by a multi-file
/// [`AnalyzedProject`] (as produced by
/// [`pactole_storage_fs::analyze_file_with_loader`]), keyed by file path.
///
/// Each file's syntax, lowering, include, and business diagnostics are
/// converted through the same LSP conversion function.
///
/// Files with no diagnostics at all are still present in the returned map
/// (mapped to an empty `Vec`), so callers can clear stale diagnostics for
/// them by publishing an empty list.
pub fn project_diagnostics(project: &AnalyzedProject) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = project
        .files()
        .iter()
        .map(|f| (f.path.clone(), Vec::new()))
        .collect();

    let sources: HashMap<&std::path::Path, &str> = project
        .files()
        .iter()
        .map(|file| (file.path.as_path(), file.file.document().source()))
        .collect();

    for diagnostic in project.diagnostics() {
        let Some(source) = sources.get(diagnostic.path.as_path()) else {
            debug_assert!(false, "project diagnostic has no analyzed source file");
            continue;
        };

        by_path
            .entry(diagnostic.path.clone())
            .or_default()
            .push(to_lsp_diagnostic(&diagnostic, source));
    }

    by_path
}

/// Builds the LSP diagnostics for the journal references made by the
/// resolved `pactole.toml`, keyed by configuration file path.
///
/// The references are cross-checked against the journal the project analysis
/// recovered (including a partial one, whose uncertain references are
/// suppressed). `sources` holds the configuration file contents the
/// configuration was resolved from, so each diagnostic can point at the exact
/// TOML value rather than at the whole file. A value that cannot be located
/// (for example because the buffer is being edited and no longer parses) is
/// reported at the start of the file instead of being dropped.
pub fn config_diagnostics(
    config: &pactole_config::Config,
    project: &AnalyzedProject,
    sources: &HashMap<PathBuf, String>,
) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let missing = project
        .partial()
        .map(|partial| partial.missing.clone())
        .unwrap_or_default();
    let journal = project.source_order_journal();

    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for diagnostic in check_config_references(config, &journal, &missing) {
        let range = sources
            .get(&diagnostic.path)
            .and_then(|source| config_key_range(source, &diagnostic.key_path))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        by_path
            .entry(diagnostic.path)
            .or_default()
            .push(Diagnostic {
                range,
                severity: Some(match diagnostic.severity {
                    ProjectDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    ProjectDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                }),
                code: Some(NumberOrString::String(diagnostic.code.as_str().to_string())),
                source: Some("pactole-config".to_string()),
                message: diagnostic.message,
                ..Default::default()
            });
    }
    by_path
}

/// Locates the TOML value a configuration key path points at, returning its
/// byte span within `source`.
fn config_key_range(source: &str, key_path: &ConfigKeyPath) -> Option<Range> {
    // `DocumentMut` intentionally drops spans when it parses; `ImDocument`
    // keeps them, which is exactly what locating a value needs.
    let document = Document::parse(source).ok()?;
    let mut node = Node::Table(document.as_table());
    for segment in key_path.segments() {
        node = match segment {
            ConfigPathSegment::Key(key) => node.key(key)?,
            ConfigPathSegment::Index(index) => node.index(*index)?,
        };
    }
    let span = node.span()?;
    Some(byte_range_to_range(source, span))
}

/// One value reached while walking a TOML document by key path.
enum Node<'a> {
    /// A table entry.
    Item(&'a Item),
    /// A bare value (an array element, or an inline table entry).
    Value(&'a Value),
    /// A table header, including an array-of-tables element.
    Table(&'a Table),
}

impl<'a> Node<'a> {
    /// Walks one table key.
    fn key(&self, key: &str) -> Option<Self> {
        match self {
            Self::Item(item) => item.as_table_like()?.get(key).map(Self::Item),
            Self::Value(value) => value.as_inline_table()?.get(key).map(Self::Value),
            Self::Table(table) => table.get(key).map(Self::Item),
        }
    }

    /// Walks one array index.
    fn index(&self, index: usize) -> Option<Self> {
        match self {
            Self::Item(item) => {
                if let Some(array) = item.as_array() {
                    array.get(index).map(Self::Value)
                } else {
                    item.as_array_of_tables()?.get(index).map(Self::Table)
                }
            }
            Self::Value(value) => value.as_array()?.get(index).map(Self::Value),
            Self::Table(_) => None,
        }
    }

    /// Returns the byte span of this value, when the parser recorded one.
    fn span(&self) -> Option<ByteRange<usize>> {
        match self {
            Self::Item(item) => item.span(),
            Self::Value(value) => value.span(),
            Self::Table(table) => table.span(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_config::{ValidationLevel, ValidationOptions};
    use pactole_storage_fs::{
        InMemorySourceLoader, analyze_file, analyze_file_with_loader,
        analyze_file_with_loader_with_options,
    };
    use std::path::Path;

    #[test]
    fn valid_source_yields_no_diagnostics() {
        let file = analyze_file("2026-09-03 open Assets:Checking\n");
        assert!(standalone_diagnostics(&file, Path::new("/journal.pactole")).is_empty());
    }

    #[test]
    fn invalid_syntax_yields_an_error_diagnostic() {
        let file = analyze_file("2026-09-03 open\n");
        let diagnostics = standalone_diagnostics(&file, Path::new("/journal.pactole"));
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "syntax-error".to_string()
            ))
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-syntax"));
    }

    #[test]
    fn standalone_and_project_diagnostics_have_the_same_lsp_shape() {
        let path = Path::new("/journal/main.pactole");
        let source = "2026-01-01 open\n";
        let standalone = standalone_diagnostics(&analyze_file(source), path);

        let mut loader = InMemorySourceLoader::new();
        loader.insert(path.to_path_buf(), source);
        let project = analyze_file_with_loader(path, &loader).unwrap();
        let project_diagnostics = project_diagnostics(&project);

        assert_eq!(standalone, project_diagnostics[path]);
    }

    #[test]
    fn missing_include_is_reported_as_an_error_on_the_including_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "include \"missing.pactole\"\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);

        let main_diags = &by_path[Path::new("/journal/main.pactole")];
        assert_eq!(main_diags.len(), 1);
        assert_eq!(main_diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            main_diags[0].code,
            Some(lsp_types::NumberOrString::String(
                "include-not-found".to_string()
            ))
        );
        assert_eq!(main_diags[0].source.as_deref(), Some("pactole-include"));
    }

    #[test]
    fn every_analyzed_file_is_present_in_the_map_even_without_diagnostics() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"other.pactole\"\n2026-09-03 open Assets:Checking\n",
        );
        loader.insert("/journal/other.pactole", "2026-09-04 open Assets:Savings\n");

        // Without this, both files would carry an `account-unused` warning
        // (nothing posts to those accounts); the point of this test is a
        // file with *no* diagnostic at all, so the check is turned off.
        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &ValidationOptions {
                unused: ValidationLevel::Off,
                ..ValidationOptions::default()
            },
        )
        .unwrap();
        let by_path = project_diagnostics(&project);

        assert!(by_path.contains_key(Path::new("/journal/main.pactole")));
        assert!(by_path.contains_key(Path::new("/journal/other.pactole")));
        assert!(by_path[Path::new("/journal/other.pactole")].is_empty());
    }

    #[test]
    fn unused_declarations_are_published_as_warnings() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "2026-01-01 open Assets:Checking\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);
        let diagnostics = &by_path[Path::new("/journal/main.pactole")];

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "account-unused".to_string()
            ))
        );
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-business"));
        assert_eq!(
            diagnostics[0].range,
            lsp_types::Range::new(
                lsp_types::Position::new(0, 16),
                lsp_types::Position::new(0, 31),
            )
        );
    }

    #[test]
    fn business_diagnostic_is_published_on_the_originating_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n\
             2026-01-01 commodity EUR\n\
             include \"transactions.pactole\"\n",
        );
        loader.insert(
            "/journal/transactions.pactole",
            "2026-01-02 * \"Unknown\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);
        let diagnostics = &by_path[Path::new("/journal/transactions.pactole")];

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "payee-not-declared".to_string()
            ))
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-business"));
        assert_eq!(diagnostics[0].range.start, lsp_types::Position::new(0, 13));
    }

    #[test]
    fn a_broken_file_does_not_hide_the_business_diagnostics_of_the_other_files() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"broken.pactole\"\ninclude \"valid.pactole\"\n",
        );
        // The date is not a calendar date: the directive cannot be lowered.
        loader.insert(
            "/journal/broken.pactole",
            "2026-13-45 open Assets:Checking\n",
        );
        loader.insert(
            "/journal/valid.pactole",
            "2026-01-01 commodity EUR\n\
             2026-01-02 * \"Unknown\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);

        // The broken file reports the lowering error and the completeness
        // notice...
        let broken = &by_path[Path::new("/journal/broken.pactole")];
        assert!(
            broken.iter().any(|diagnostic| diagnostic.code
                == Some(lsp_types::NumberOrString::String(
                    "lowering-error".to_string()
                ))),
            "{broken:?}"
        );
        assert!(
            broken.iter().any(|diagnostic| diagnostic.code
                == Some(lsp_types::NumberOrString::String(
                    "analysis-incomplete".to_string()
                ))),
            "{broken:?}"
        );

        // ...and the valid file still carries its business diagnostic.
        let valid = &by_path[Path::new("/journal/valid.pactole")];
        assert!(
            valid.iter().any(|diagnostic| diagnostic.code
                == Some(lsp_types::NumberOrString::String(
                    "payee-not-declared".to_string()
                ))),
            "{valid:?}"
        );
    }

    /// Builds a key path from table keys and indices.
    fn key_path(segments: &[ConfigPathSegment]) -> ConfigKeyPath {
        ConfigKeyPath::new(segments.to_vec())
    }

    fn key(name: &str) -> ConfigPathSegment {
        ConfigPathSegment::Key(name.to_string())
    }

    fn range(source: &str, value: &str) -> lsp_types::Range {
        let start = source.find(value).expect("value is present");
        byte_range_to_range(source, start..start + value.len())
    }

    #[test]
    fn a_key_path_locates_a_regular_table_value() {
        let source = "[import]\nbank_account = \"Assets:Savings\"\n";
        let path = key_path(&[key("import"), key("bank_account")]);

        assert_eq!(
            config_key_range(source, &path),
            Some(range(source, "\"Assets:Savings\""))
        );
    }

    #[test]
    fn a_key_path_locates_an_array_element() {
        let source = "[reports]\nassets = [\"Actifs\", \"Cash\"]\n";
        let path = key_path(&[key("reports"), key("assets"), ConfigPathSegment::Index(1)]);

        assert_eq!(
            config_key_range(source, &path),
            Some(range(source, "\"Cash\""))
        );
    }

    #[test]
    fn a_key_path_locates_a_field_of_an_array_of_tables() {
        let source = "[[import.rules]]\ncounter_account = \"Expenses:Food\"\n";
        let path = key_path(&[
            key("import"),
            key("rules"),
            ConfigPathSegment::Index(0),
            key("counter_account"),
        ]);

        assert_eq!(
            config_key_range(source, &path),
            Some(range(source, "\"Expenses:Food\""))
        );
    }

    #[test]
    fn a_key_path_locates_a_field_of_an_inline_table_in_an_array() {
        let source =
            "[import]\nrules = [{ counter_account = \"Expenses:Travel\", payee = \"Airline\" }]\n";
        let path = key_path(&[
            key("import"),
            key("rules"),
            ConfigPathSegment::Index(0),
            key("counter_account"),
        ]);

        assert_eq!(
            config_key_range(source, &path),
            Some(range(source, "\"Expenses:Travel\""))
        );
    }

    #[test]
    fn a_key_path_locates_a_field_of_a_top_level_inline_table() {
        let source = "import = { bank_account = \"Assets:Checking\", default_counter_account = \"Expenses:Food\" }\n";
        let path = key_path(&[key("import"), key("bank_account")]);

        assert_eq!(
            config_key_range(source, &path),
            Some(range(source, "\"Assets:Checking\""))
        );
    }

    #[test]
    fn a_key_path_that_does_not_resolve_has_no_range() {
        let source = "[import]\nbank_account = \"Assets:Checking\"\n";
        let path = key_path(&[key("import"), key("missing")]);

        assert_eq!(config_key_range(source, &path), None);
    }

    /// A configuration reference missing from the journal becomes an LSP
    /// diagnostic on the configuration file, pointing at the TOML value.
    #[test]
    fn a_missing_journal_declaration_is_reported_on_the_configuration_value() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n\
             2026-01-01 commodity EUR\n\
             payee \"Employer\"\n\
             2026-01-02 * \"Employer\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );
        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let contents = "[import]\n\
                        bank_account = \"Assets:Savings\"\n\
                        default_counter_account = \"Expenses:Food\"\n";
        let config_path = Path::new("/journal/pactole.toml");
        let config = pactole_config::Config::resolve(
            &[pactole_config::Source {
                path: config_path,
                contents,
            }],
            &pactole_config::Overrides::default(),
        )
        .unwrap();
        let sources = HashMap::from([(config_path.to_path_buf(), contents.to_string())]);

        let by_path = config_diagnostics(&config, &project, &sources);

        let diagnostics = &by_path[config_path];
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].code,
            Some(NumberOrString::String(
                "config-account-not-declared".to_string()
            ))
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-config"));
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostics[0].range, range(contents, "\"Assets:Savings\""));
    }

    #[test]
    fn a_reference_whose_value_cannot_be_located_falls_back_to_the_file_start() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n\
             2026-01-01 commodity EUR\n\
             payee \"Employer\"\n\
             2026-01-02 * \"Employer\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );
        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let contents = "[import]\n\
                        bank_account = \"Assets:Savings\"\n\
                        default_counter_account = \"Expenses:Food\"\n";
        let config_path = Path::new("/journal/pactole.toml");
        let config = pactole_config::Config::resolve(
            &[pactole_config::Source {
                path: config_path,
                contents,
            }],
            &pactole_config::Overrides::default(),
        )
        .unwrap();

        // The buffer the configuration was resolved from is no longer
        // available (e.g. it was closed): the diagnostic is not dropped.
        let by_path = config_diagnostics(&config, &project, &HashMap::new());

        assert_eq!(
            by_path[config_path][0].range,
            lsp_types::Range::new(
                lsp_types::Position::new(0, 0),
                lsp_types::Position::new(0, 0)
            )
        );
    }
}
