//! Positioned, directive-by-directive syntax+lowering analysis of a `.pactole`
//! source text, for tooling (e.g. editors, a future language server) that
//! needs spans and per-directive diagnostics rather than an all-or-nothing
//! `Journal`.
//!
//! Unlike [`crate::parser::parse`], which fails as soon as either the syntax
//! is invalid or a single directive cannot be lowered, [`analyze_file`] never
//! fails: it reuses `pactole-syntax`'s tolerant parsing and the same
//! `AstBuilder` lowering logic as `parser.rs`, but collects one outcome per
//! top-level directive instead of stopping at the first error.
//!
//! `include` directives are *detected* here, with their raw (unresolved)
//! path and span, but are not recursively read/parsed: resolving includes
//! remains the sole responsibility of [`crate::parser::parse`] and
//! [`crate::PactoleFileStorage`], whose existing strict behavior is
//! unchanged by this module.

use std::path::{Path, PathBuf};

use pactole_core::Entry;
use pactole_syntax::{ParsedDocument, Span, SyntaxDiagnostic, parse_document};

use crate::loader::{SourceLoadError, SourceLoader};
use crate::parser::{AstBuilder, DirectiveOutcome};

/// A single top-level entry successfully lowered from a directive, together
/// with the span of the `directive` node it was built from.
#[derive(Debug, Clone)]
pub struct ParsedEntry {
    /// The span of the `directive` node this entry was built from.
    pub span: Span,
    /// The lowered domain entry.
    pub entry: Entry,
}

/// A single `include` directive detected in the source, with its raw
/// (unresolved) path and span. The included file is neither read nor parsed
/// by this module.
#[derive(Debug, Clone)]
pub struct ParsedInclude {
    /// The span of the `directive` node the include was found in.
    pub span: Span,
    /// The raw path string as written in the source, not yet resolved
    /// against any base directory.
    pub path: String,
}

/// A diagnostic produced while lowering a syntactically valid directive into
/// a domain entry (e.g. an invalid date, an unknown account name, an
/// unbalanced transaction, ...).
///
/// This is distinct from [`pactole_syntax::SyntaxDiagnostic`], which reports
/// purely syntactic issues (`ERROR`/`MISSING` nodes) found before any
/// lowering is attempted.
#[derive(Debug, Clone)]
pub struct LoweringDiagnostic {
    /// The span of the `directive` node that failed to lower.
    pub span: Span,
    /// A human-readable description of the failure.
    pub message: String,
}

/// The result of a positioned, directive-by-directive analysis of a
/// `.pactole` source text.
///
/// Unlike [`crate::parser::parse`], this never fails: a syntactically
/// invalid source yields empty [`ParsedFile::entries`]/[`ParsedFile::includes`]
/// and non-empty [`ParsedFile::syntax_diagnostics`]; a syntactically valid
/// source with directives that fail to lower yields the entries that *did*
/// lower successfully plus [`ParsedFile::lowering_diagnostics`] for the ones
/// that didn't.
pub struct ParsedFile {
    document: ParsedDocument,
    entries: Vec<ParsedEntry>,
    includes: Vec<ParsedInclude>,
    lowering_diagnostics: Vec<LoweringDiagnostic>,
}

impl ParsedFile {
    /// The underlying tolerant syntax analysis (source, concrete syntax
    /// tree, and syntax diagnostics), as produced by `pactole-syntax`.
    pub fn document(&self) -> &ParsedDocument {
        &self.document
    }

    /// The entries that were successfully lowered from top-level directives,
    /// in document order, each carrying the span of its source directive.
    pub fn entries(&self) -> &[ParsedEntry] {
        &self.entries
    }

    /// The `include` directives detected in the source, in document order,
    /// with their raw (unresolved) path and span. Included files are not
    /// read or parsed.
    pub fn includes(&self) -> &[ParsedInclude] {
        &self.includes
    }

    /// The syntax diagnostics collected while parsing (`ERROR`/`MISSING`
    /// nodes), in document order.
    pub fn syntax_diagnostics(&self) -> &[SyntaxDiagnostic] {
        self.document.diagnostics()
    }

    /// The diagnostics produced while lowering syntactically valid
    /// directives into domain entries, in document order.
    pub fn lowering_diagnostics(&self) -> &[LoweringDiagnostic] {
        &self.lowering_diagnostics
    }
}

/// Analyzes `source` directive-by-directive, producing a positioned
/// [`ParsedFile`] that never fails, even for syntactically invalid or
/// partially invalid input.
///
/// This does not resolve `include` directives: they are reported as
/// [`ParsedInclude`] entries with their raw path, and their target files are
/// neither read nor parsed. Use [`crate::parser::parse`] (via
/// [`crate::PactoleFileStorage`]) for the existing strict behavior that
/// recursively inlines includes and fails on the first error.
pub fn analyze_file(source: &str) -> ParsedFile {
    let document = parse_document(source);

    let mut entries = Vec::new();
    let mut includes = Vec::new();
    let mut lowering_diagnostics = Vec::new();

    // Even on a syntactically invalid document, tree-sitter still produces a
    // best-effort tree: attempt lowering whatever directives are well-formed
    // enough for `AstBuilder`, rather than giving up on the whole file.
    let builder = AstBuilder::new(document.source());

    {
        let mut cursor = document.root_node().walk();
        for child in document.root_node().named_children(&mut cursor) {
            if child.kind() != "directive" {
                continue;
            }

            let span = Span::from_node(&child);

            match builder.build_directive_outcome(child) {
                Ok(DirectiveOutcome::Entry(entry)) => entries.push(ParsedEntry { span, entry }),
                Ok(DirectiveOutcome::Include(path)) => includes.push(ParsedInclude { span, path }),
                Err(err) => lowering_diagnostics.push(LoweringDiagnostic {
                    span,
                    message: err.to_string(),
                }),
            }
        }
    }

    ParsedFile {
        document,
        entries,
        includes,
        lowering_diagnostics,
    }
}

/// A [`ParsedFile`] together with the path it was loaded from, as produced
/// by [`analyze_file_with_loader`].
pub struct AnalyzedFile {
    /// The path this file was loaded from (as resolved against its
    /// including file's directory, or the entry path itself).
    pub path: PathBuf,
    /// The positioned, directive-by-directive analysis of this file.
    pub file: ParsedFile,
}

/// A problem encountered while resolving an `include` directive during
/// multi-file analysis: the target could not be loaded, resolved without a
/// base directory, or resolving it would form a cycle.
///
/// Unlike [`LoweringDiagnostic`], this is not about a single file's content
/// but about the *graph* of files reachable from an entry point.
pub struct IncludeIssue {
    /// The path of the file containing the `include` directive.
    pub path: PathBuf,
    /// The `include` directive itself (raw path and span within `path`).
    pub include: ParsedInclude,
    /// A human-readable description of why this include could not be
    /// resolved.
    pub message: String,
}

/// The result of a multi-file, directive-by-directive analysis rooted at an
/// entry path, following `include` directives via a [`SourceLoader`].
///
/// Like [`ParsedFile`]/[`analyze_file`], this never fails on file content:
/// syntax and lowering issues are reported per-file via each
/// [`AnalyzedFile::file`]. It can, however, fail to even load the entry
/// path itself (see [`analyze_file_with_loader`]'s return type); everything
/// reachable from includes is instead reported as [`IncludeIssue`]s so a
/// single unreadable or cyclic include does not abort the whole analysis.
pub struct AnalyzedProject {
    files: Vec<AnalyzedFile>,
    include_issues: Vec<IncludeIssue>,
}

impl AnalyzedProject {
    /// The files reached from the entry path, in the order they were first
    /// encountered (entry path first, then each include depth-first). A
    /// file that is `include`d from more than one place (a "diamond"
    /// include) appears only once.
    pub fn files(&self) -> &[AnalyzedFile] {
        &self.files
    }

    /// The `include` directives that could not be resolved: missing files,
    /// loader errors, relative includes with no base directory, or include
    /// cycles.
    pub fn include_issues(&self) -> &[IncludeIssue] {
        &self.include_issues
    }
}

/// Analyzes `entry_path` and, recursively, every file it (transitively)
/// `include`s, resolving each include's path via `loader` rather than
/// reading real files directly.
///
/// This is the multi-file counterpart to [`analyze_file`]: where
/// `analyze_file` reports `include` directives without following them,
/// `analyze_file_with_loader` follows them, but — unlike
/// [`crate::parser::parse`] — keeps each file's entries and diagnostics
/// separate (see [`AnalyzedFile`]) rather than flattening everything into a
/// single [`pactole_core::Journal`]. This keeps per-file provenance, which
/// matters for tooling that needs to report a diagnostic against the file
/// it actually came from.
///
/// A relative include is resolved against the directory of the file it
/// appears in; an include with no reachable base directory (e.g. the entry
/// path itself has no parent), a target the loader cannot provide, or an
/// include cycle are all reported as [`IncludeIssue`]s rather than causing
/// the whole analysis to fail. Only a failure to load `entry_path` itself
/// is returned as an `Err`, since there is then nothing to analyze at all.
pub fn analyze_file_with_loader(
    entry_path: &Path,
    loader: &dyn SourceLoader,
) -> Result<AnalyzedProject, SourceLoadError> {
    let mut files = Vec::new();
    let mut include_issues = Vec::new();
    // Paths currently being visited, i.e. on the path from the entry point
    // to the file currently being processed: used to detect include cycles.
    let mut in_progress = Vec::new();

    visit(
        entry_path,
        loader,
        &mut files,
        &mut include_issues,
        &mut in_progress,
        0,
    )?;

    Ok(AnalyzedProject {
        files,
        include_issues,
    })
}

/// A hard cap on include recursion depth, as a safety net against pathological
/// include graphs that lexical normalization and cycle detection alone might
/// not catch (e.g. a very long, strictly non-repeating chain of includes).
/// This is not expected to be hit in practice.
const MAX_INCLUDE_DEPTH: usize = 256;

/// Lexically normalizes `path` by collapsing `.` and `..` components without
/// touching the filesystem (no symlink resolution, no existence check).
///
/// This ensures that equivalent-but-textually-different includes (e.g.
/// `a/./b.pactole`, `a/../a/b.pactole`, and `a/b.pactole`) are recognized as
/// the same file for cycle detection and diamond-include deduplication,
/// instead of being treated as distinct paths and recursing without bound.
/// Absolute paths are preserved as absolute; this only collapses components,
/// it never resolves a relative path against a base directory.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }

    ret
}

fn visit(
    path: &Path,
    loader: &dyn SourceLoader,
    files: &mut Vec<AnalyzedFile>,
    include_issues: &mut Vec<IncludeIssue>,
    in_progress: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), SourceLoadError> {
    let path = normalize_lexically(path);

    // A "diamond" include (the same file reached via two different
    // include paths, possibly written with different `.`/`..` components):
    // already fully analyzed, nothing more to do.
    if files.iter().any(|f| f.path == path) {
        return Ok(());
    }

    let source = loader.load(&path)?;
    let parsed = analyze_file(&source);
    let base_dir = path.parent().map(Path::to_path_buf);

    // Clone the includes out before `parsed` is moved into `files` below:
    // `ParsedFile` is not `Clone` (it owns a tree-sitter `Tree`), but
    // `ParsedInclude` is a small, plain value.
    let includes: Vec<ParsedInclude> = parsed.includes().to_vec();

    files.push(AnalyzedFile {
        path: path.clone(),
        file: parsed,
    });

    in_progress.push(path.clone());

    for include in includes {
        let raw = PathBuf::from(&include.path);
        let resolved = if raw.is_absolute() {
            raw
        } else {
            match &base_dir {
                Some(dir) => dir.join(&raw),
                None => {
                    include_issues.push(IncludeIssue {
                        path: path.clone(),
                        message: format!(
                            "cannot resolve relative include `{}` without a base directory",
                            include.path
                        ),
                        include,
                    });
                    continue;
                }
            }
        };

        let resolved = normalize_lexically(&resolved);

        if in_progress.contains(&resolved) {
            include_issues.push(IncludeIssue {
                path: path.clone(),
                message: format!(
                    "include cycle detected: `{}` is already being analyzed",
                    resolved.display()
                ),
                include,
            });
            continue;
        }

        if depth + 1 >= MAX_INCLUDE_DEPTH {
            include_issues.push(IncludeIssue {
                path: path.clone(),
                message: format!(
                    "include depth limit ({MAX_INCLUDE_DEPTH}) exceeded while resolving `{}`",
                    resolved.display()
                ),
                include,
            });
            continue;
        }

        match visit(
            &resolved,
            loader,
            files,
            include_issues,
            in_progress,
            depth + 1,
        ) {
            Ok(()) => {}
            Err(SourceLoadError::NotFound(missing)) => {
                include_issues.push(IncludeIssue {
                    path: path.clone(),
                    message: format!("included file not found: `{}`", missing.display()),
                    include,
                });
            }
            Err(err) => {
                include_issues.push(IncludeIssue {
                    path: path.clone(),
                    message: err.to_string(),
                    include,
                });
            }
        }
    }

    in_progress.pop();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::InMemorySourceLoader;
    use pactole_core::Entry;

    #[test]
    fn valid_source_yields_entries_with_spans() {
        let source = "2026-09-03 open Assets:Checking\n";
        let file = analyze_file(source);

        assert!(file.syntax_diagnostics().is_empty());
        assert!(file.lowering_diagnostics().is_empty());
        assert_eq!(file.entries().len(), 1);

        let entry = &file.entries()[0];
        assert!(matches!(entry.entry, Entry::Open(_)));
        assert_eq!(entry.span.start.row, 0);
        assert_eq!(entry.span.start.byte, 0);
        assert_eq!(entry.span.end.byte, source.len());
    }

    #[test]
    fn include_is_detected_with_its_raw_path_and_span_but_not_resolved() {
        let source = "include \"other.pactole\"\n";
        let file = analyze_file(source);

        assert!(file.entries().is_empty());
        assert_eq!(file.includes().len(), 1);

        let include = &file.includes()[0];
        assert_eq!(include.path, "other.pactole");
        assert_eq!(include.span.start.byte, 0);
        assert_eq!(include.span.end.byte, source.len());
    }

    #[test]
    fn invalid_source_does_not_panic_and_reports_syntax_diagnostics() {
        let source = "2026-09-03 open\n";
        let file = analyze_file(source);

        assert!(!file.syntax_diagnostics().is_empty());
        // The malformed `open` directive itself may or may not be lowerable
        // depending on how tree-sitter recovers; the important property is
        // that analysis completes without panicking and still reports the
        // syntax error positioned in the source.
        let diagnostic = &file.syntax_diagnostics()[0];
        assert!(diagnostic.span.start.byte <= source.len());
    }

    #[test]
    fn multiple_entries_and_an_include_are_all_positioned() {
        let source = "2026-09-03 open Assets:Checking\ninclude \"other.pactole\"\n2026-09-04 close Assets:Checking\n";
        let file = analyze_file(source);

        assert!(file.syntax_diagnostics().is_empty());
        assert!(file.lowering_diagnostics().is_empty());
        assert_eq!(file.entries().len(), 2);
        assert_eq!(file.includes().len(), 1);

        // Entries and includes are reported in source order relative to each
        // other's spans.
        assert!(file.entries()[0].span.start.byte < file.includes()[0].span.start.byte);
        assert!(file.includes()[0].span.start.byte < file.entries()[1].span.start.byte);
    }

    #[test]
    fn loader_based_analysis_follows_a_relative_include_and_keeps_per_file_provenance() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-09-03 open Assets:Checking\ninclude \"other.pactole\"\n",
        );
        loader.insert("/journal/other.pactole", "2026-09-04 open Assets:Savings\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(project.include_issues().is_empty());
        assert_eq!(project.files().len(), 2);

        assert_eq!(
            project.files()[0].path,
            PathBuf::from("/journal/main.pactole")
        );
        assert_eq!(project.files()[0].file.entries().len(), 1);

        assert_eq!(
            project.files()[1].path,
            PathBuf::from("/journal/other.pactole")
        );
        assert_eq!(project.files()[1].file.entries().len(), 1);
        match &project.files()[1].file.entries()[0].entry {
            Entry::Open(open) => assert_eq!(open.account.to_string(), "Assets:Savings"),
            other => panic!("expected an Open entry, got {other:?}"),
        }
    }

    #[test]
    fn loader_based_analysis_reports_a_missing_include_without_failing() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "include \"missing.pactole\"\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert_eq!(project.files().len(), 1);
        assert_eq!(project.include_issues().len(), 1);
        let issue = &project.include_issues()[0];
        assert_eq!(issue.path, PathBuf::from("/journal/main.pactole"));
        assert_eq!(issue.include.path, "missing.pactole");
        assert!(issue.message.contains("not found"));
    }

    #[test]
    fn loader_based_analysis_detects_an_include_cycle() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/a.pactole", "include \"b.pactole\"\n");
        loader.insert("/journal/b.pactole", "include \"a.pactole\"\n");

        let project = analyze_file_with_loader(Path::new("/journal/a.pactole"), &loader).unwrap();

        assert_eq!(project.files().len(), 2);
        assert_eq!(project.include_issues().len(), 1);
        let issue = &project.include_issues()[0];
        assert_eq!(issue.path, PathBuf::from("/journal/b.pactole"));
        assert_eq!(issue.include.path, "a.pactole");
        assert!(issue.message.contains("cycle"));
    }

    #[test]
    fn loader_based_analysis_deduplicates_a_diamond_include() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"a.pactole\"\ninclude \"b.pactole\"\n",
        );
        loader.insert("/journal/a.pactole", "include \"common.pactole\"\n");
        loader.insert("/journal/b.pactole", "include \"common.pactole\"\n");
        loader.insert(
            "/journal/common.pactole",
            "2026-09-03 open Assets:Checking\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(project.include_issues().is_empty());
        // main, a, common, b: `common` is visited once even though both `a`
        // and `b` include it.
        assert_eq!(project.files().len(), 4);
        let common_count = project
            .files()
            .iter()
            .filter(|f| f.path == Path::new("/journal/common.pactole"))
            .count();
        assert_eq!(common_count, 1);
    }

    #[test]
    fn loader_based_analysis_fails_when_the_entry_path_itself_cannot_be_loaded() {
        let loader = InMemorySourceLoader::new();

        let result = analyze_file_with_loader(Path::new("/journal/missing-entry.pactole"), &loader);

        assert!(matches!(result, Err(SourceLoadError::NotFound(_))));
    }

    #[test]
    fn loader_based_analysis_follows_an_absolute_include() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"/shared/other.pactole\"\n",
        );
        loader.insert("/shared/other.pactole", "2026-09-04 open Assets:Savings\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(project.include_issues().is_empty());
        assert_eq!(project.files().len(), 2);
        assert_eq!(
            project.files()[1].path,
            PathBuf::from("/shared/other.pactole")
        );
        assert_eq!(project.files()[1].file.entries().len(), 1);
    }

    /// A [`SourceLoader`] used only to exercise the non-`NotFound` error path
    /// of [`SourceLoadError`], which [`InMemorySourceLoader`] cannot produce.
    struct FailingLoader;

    impl SourceLoader for FailingLoader {
        fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
            Err(SourceLoadError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::other("simulated permission denied"),
            })
        }
    }

    #[test]
    fn loader_based_analysis_fails_when_the_entry_path_load_errors_with_something_other_than_not_found()
     {
        let loader = FailingLoader;

        let result = analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader);

        assert!(matches!(result, Err(SourceLoadError::Io { .. })));
    }

    /// A [`SourceLoader`] whose entry file includes a target that always
    /// fails to load with a non-`NotFound` error, to check that such errors
    /// are reported as [`IncludeIssue`]s instead of aborting the analysis.
    struct EntryOkThenFailingLoader;

    impl SourceLoader for EntryOkThenFailingLoader {
        fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
            if path == Path::new("/journal/main.pactole") {
                Ok("include \"broken.pactole\"\n".to_string())
            } else {
                Err(SourceLoadError::Io {
                    path: path.to_path_buf(),
                    source: std::io::Error::other("simulated permission denied"),
                })
            }
        }
    }

    #[test]
    fn loader_based_analysis_reports_a_non_not_found_include_error_as_an_include_issue() {
        let loader = EntryOkThenFailingLoader;

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert_eq!(project.files().len(), 1);
        assert_eq!(project.include_issues().len(), 1);
        let issue = &project.include_issues()[0];
        assert_eq!(issue.include.path, "broken.pactole");
        assert!(issue.message.contains("simulated permission denied"));
    }

    #[test]
    fn loader_based_analysis_detects_a_cycle_written_with_dot_and_dot_dot_components() {
        // `./a.pactole` and `sub/../a.pactole` are lexically equivalent to
        // `a.pactole`; without normalization these would be treated as
        // distinct paths and the mutual include would recurse without bound.
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/a.pactole", "include \"./b.pactole\"\n");
        loader.insert("/journal/b.pactole", "include \"sub/../a.pactole\"\n");

        let project = analyze_file_with_loader(Path::new("/journal/a.pactole"), &loader).unwrap();

        assert_eq!(project.files().len(), 2);
        assert_eq!(project.include_issues().len(), 1);
        let issue = &project.include_issues()[0];
        assert!(issue.message.contains("cycle"));
    }

    #[test]
    fn loader_based_analysis_reports_a_relative_include_without_a_base_directory() {
        // A path with no parent directory at all cannot resolve a relative
        // include against anything.
        let mut loader = InMemorySourceLoader::new();
        loader.insert("", "include \"other.pactole\"\n");

        let project = analyze_file_with_loader(Path::new(""), &loader).unwrap();

        assert_eq!(project.files().len(), 1);
        assert_eq!(project.include_issues().len(), 1);
        assert!(
            project.include_issues()[0]
                .message
                .contains("base directory")
        );
    }
}
