//! Positioned, directive-by-directive syntax+lowering analysis of a `.pactole`
//! source text, for tooling (e.g. editors, a future language server) that
//! needs spans and per-directive diagnostics rather than an all-or-nothing
//! `Journal`.
//!
//! [`analyze_file`] never fails: it reuses `pactole-syntax`'s tolerant parsing
//! and the shared `AstBuilder` lowering logic, collecting one outcome per
//! top-level directive instead of stopping at the first error.
//!
//! `include` directives are detected here with their raw path and span, then
//! resolved by [`analyze_file_with_loader`] while preserving per-file
//! provenance.

use std::path::{Path, PathBuf};

use pactole_config::{ValidationLevel, ValidationOptions};
use pactole_core::{
    DiagnosticCode, Entry, Journal, MissingData, ValidationError, ValidationIssue,
    unused_declarations_with, validate_journal_all_with,
};
use pactole_syntax::{ParsedDocument, Span, SyntaxDiagnostic, parse_document};
use tree_sitter::Node;

use crate::loader::{SourceLoadError, SourceLoader};
use crate::parser::{AstBuilder, DirectiveOutcome};
use crate::uncertainty;

/// A single top-level entry successfully lowered from a directive, together
/// with the span of the `directive` node it was built from.
#[derive(Debug, Clone)]
pub struct ParsedEntry {
    /// The span of the `directive` node this entry was built from.
    pub span: Span,
    /// The lowered domain entry.
    pub entry: Entry,
    /// Spans of the significant source elements inside the directive.
    pub spans: EntrySpans,
}

/// Source spans for the significant elements of a lowered entry.
///
/// The domain model deliberately contains no source information. These spans
/// keep that information at the storage boundary so diagnostics can point to
/// the offending account, commodity, or payee instead of the whole directive.
#[derive(Debug, Clone, Default)]
pub struct EntrySpans {
    /// The date node, when the entry has one.
    pub date: Option<Span>,
    /// The main account node of the directive, when applicable.
    pub account: Option<Span>,
    /// Commodity nodes belonging directly to the directive.
    pub commodities: Vec<Span>,
    /// The transaction payee string, without its surrounding `payee` node.
    pub payee: Option<Span>,
    /// Source spans for transaction postings, in posting order.
    pub postings: Vec<PostingSpans>,
}

/// Source spans for one transaction posting.
#[derive(Debug, Clone)]
pub struct PostingSpans {
    /// The posting account node.
    pub account: Span,
    /// The posting commodity node, when an amount is present.
    pub commodity: Option<Span>,
    /// The full transaction-cost annotation, when present.
    pub transaction_cost: Option<Span>,
    /// The transaction-cost quote commodity, when an annotation is present.
    pub transaction_cost_commodity: Option<Span>,
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

/// Severity shared by all diagnostics produced by project analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// The diagnostic blocks the default CLI/LSP operation.
    Error,
    /// The diagnostic is reported but does not fail `check` unless `--strict` is used.
    Warning,
}

/// Origin of a project diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    /// Tree-sitter syntax analysis.
    Syntax,
    /// CST-to-domain lowering.
    Lowering,
    /// Include graph resolution.
    Include,
    /// Core journal validation.
    Business,
    /// Completeness of the project analysis.
    Analysis,
}

/// A source-positioned diagnostic ready for consumption by a CLI or editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// File in which the diagnostic was produced.
    pub path: PathBuf,
    /// Half-open source span of the diagnostic.
    pub span: Span,
    /// Whether the diagnostic blocks the operation by default.
    pub severity: DiagnosticSeverity,
    /// Layer that produced the diagnostic.
    pub source: DiagnosticSource,
    /// Stable machine-readable code for this diagnostic.
    pub code: DiagnosticCode,
    /// Human-readable explanation of the problem.
    pub message: String,
}

/// A business validation error positioned in its originating source file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BusinessDiagnostic {
    path: PathBuf,
    span: Span,
    code: DiagnosticCode,
    message: String,
}

impl BusinessDiagnostic {
    fn from_error(path: PathBuf, span: Span, error: &ValidationError) -> Self {
        Self {
            path,
            span,
            code: error.code(),
            message: error.to_string(),
        }
    }
}

/// Severity of a business diagnostic under the given validation options.
///
/// Two business checks are configurable through the `[validation]` section of
/// `pactole.toml`: `PayeeNotDeclared` (the `payee` key, an error by default)
/// and the unused-declaration kinds (the `unused` key, a warning by default),
/// each reported as a warning under [`ValidationLevel::Warning`] and dropped
/// entirely under [`ValidationLevel::Off`]. Every other kind stays an error
/// whatever the configuration says.
fn business_severity(
    code: DiagnosticCode,
    options: &ValidationOptions,
) -> Option<DiagnosticSeverity> {
    let level = match code {
        DiagnosticCode::PayeeNotDeclared => options.payee,
        DiagnosticCode::AccountUnused
        | DiagnosticCode::CommodityUnused
        | DiagnosticCode::PayeeUnused => options.unused,
        _ => return Some(DiagnosticSeverity::Error),
    };

    match level {
        ValidationLevel::Off => None,
        ValidationLevel::Warning => Some(DiagnosticSeverity::Warning),
        ValidationLevel::Error => Some(DiagnosticSeverity::Error),
    }
}

/// The result of a positioned, directive-by-directive analysis of a
/// `.pactole` source text.
///
/// This never fails: a syntactically invalid source yields empty
/// [`ParsedFile::entries`]/[`ParsedFile::includes`]
/// and non-empty [`ParsedFile::syntax_diagnostics`]; a syntactically valid
/// source with directives that fail to lower yields the entries that *did*
/// lower successfully plus [`ParsedFile::lowering_diagnostics`] for the ones
/// that didn't.
pub struct ParsedFile {
    document: ParsedDocument,
    entries: Vec<ParsedEntry>,
    includes: Vec<ParsedInclude>,
    lowering_diagnostics: Vec<LoweringDiagnostic>,
    missing: MissingData,
    lost_directives: usize,
}

impl ParsedFile {
    /// The underlying tolerant syntax analysis (source, concrete syntax
    /// tree, and syntax diagnostics), as produced by `pactole-syntax`.
    pub fn document(&self) -> &ParsedDocument {
        &self.document
    }

    /// The entries that were successfully lowered from top-level directives,
    /// in document order, each carrying directive and element-level source
    /// spans.
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

    /// Builds the unified syntax and lowering diagnostics for this file.
    ///
    /// Business and include diagnostics are intentionally absent: a standalone
    /// file has no project context in which those validations would be sound.
    pub fn diagnostics(&self, path: &Path) -> Vec<Diagnostic> {
        file_diagnostics(path, self)
    }

    /// The names this file's analysis could neither confirm nor rule out.
    ///
    /// A directive that could not be lowered, or that contains a syntax
    /// error, may have declared or used any of the names it mentions: the
    /// business checks that depend on those facts are uncertain for this
    /// file.
    pub(crate) fn missing(&self) -> &MissingData {
        &self.missing
    }

    /// The number of top-level chunks of this file's source that could not be
    /// turned into a reliable domain entry: either the directive did not
    /// lower at all, or it holds a syntax error.
    pub(crate) fn lost_directives(&self) -> usize {
        self.lost_directives
    }
}

/// Analyzes `source` directive-by-directive, producing a positioned
/// [`ParsedFile`] that never fails, even for syntactically invalid or
/// partially invalid input.
///
/// This does not resolve `include` directives: they are reported as
/// [`ParsedInclude`] entries with their raw path, and their target files are
/// neither read nor parsed. Use [`analyze_file_with_loader`] for recursive
/// include resolution with source provenance.
pub fn analyze_file(source: &str) -> ParsedFile {
    let document = parse_document(source);

    let mut entries = Vec::new();
    let mut includes = Vec::new();
    let mut lowering_diagnostics = Vec::new();
    // What this file's analysis could neither confirm nor rule out: the
    // names the lost content may have declared or used.
    let mut missing = MissingData::default();
    let mut lost_directives = 0;

    // Even on a syntactically invalid document, tree-sitter still produces a
    // best-effort tree: attempt lowering whatever directives are well-formed
    // enough for `AstBuilder`, rather than giving up on the whole file.
    let builder = AstBuilder::new(document.source());

    {
        let mut cursor = document.root_node().walk();
        for child in document.root_node().named_children(&mut cursor) {
            // A top-level comment is not a directive: it names no account,
            // commodity nor payee, so nothing is lost when it is not lowered.
            // Counting it as a lost directive would report a spurious
            // `analysis-incomplete` warning on every commented file.
            if child.kind() == "comment" {
                continue;
            }
            if child.kind() != "directive" {
                // The parser could not make sense of this part of the file:
                // its content is unknown, so the names it mentions may have
                // been declared or used there.
                lost_directives += 1;
                uncertainty::record_unknown(&mut missing, child, document.source());
                continue;
            }

            let span = Span::from_node(&child);
            // A directive holding a syntax error is not reliable, even when
            // it lowers: the error may have swallowed part of a name.
            let degraded = uncertainty::is_degraded(child);

            match builder.build_directive_outcome(child) {
                Ok(DirectiveOutcome::Entry(entry)) => {
                    if degraded {
                        lost_directives += 1;
                        uncertainty::record_unknown(&mut missing, child, document.source());
                    }
                    entries.push(ParsedEntry {
                        span,
                        entry,
                        spans: entry_spans(child),
                    });
                }
                Ok(DirectiveOutcome::Include(path)) => {
                    if degraded {
                        lost_directives += 1;
                        uncertainty::record_unknown(&mut missing, child, document.source());
                    }
                    includes.push(ParsedInclude { span, path });
                }
                Err(err) => {
                    lost_directives += 1;
                    if degraded {
                        uncertainty::record_unknown(&mut missing, child, document.source());
                    } else {
                        uncertainty::record_lost_directive(&mut missing, child, document.source());
                    }
                    lowering_diagnostics.push(LoweringDiagnostic {
                        span,
                        message: err.to_string(),
                    });
                }
            }
        }
    }

    ParsedFile {
        document,
        entries,
        includes,
        lowering_diagnostics,
        missing,
        lost_directives,
    }
}

fn entry_spans(directive: Node<'_>) -> EntrySpans {
    let Some(header) = directive.named_child(0) else {
        return EntrySpans::default();
    };

    let mut spans = EntrySpans {
        date: named_child(header, "date").map(|node| Span::from_node(&node)),
        ..EntrySpans::default()
    };

    match header.kind() {
        "open" | "close" | "balance" => {
            spans.account = named_child(header, "account").map(|node| Span::from_node(&node));
            spans.commodities = named_children(header, "commodity_name")
                .into_iter()
                .map(|node| Span::from_node(&node))
                .collect();
        }
        "commodity" | "price" => {
            spans.commodities = named_children(header, "commodity_name")
                .into_iter()
                .map(|node| Span::from_node(&node))
                .collect();
        }
        "payee_declaration" => {
            spans.payee = named_child(header, "string").map(|node| Span::from_node(&node));
        }
        "transaction" => {
            spans.payee = named_child(header, "payee")
                .and_then(|payee| named_child(payee, "string"))
                .map(|node| Span::from_node(&node));
        }
        _ => {}
    }

    let mut cursor = directive.walk();
    for child in directive.named_children(&mut cursor) {
        if child.kind() != "posting" {
            continue;
        }

        let Some(account) = named_child(child, "account") else {
            continue;
        };
        let commodity = named_child(child, "amount")
            .and_then(|amount| named_child(amount, "commodity_name"))
            .map(|node| Span::from_node(&node));
        let transaction_cost = named_child(child, "transaction_cost");
        let transaction_cost_commodity = transaction_cost
            .and_then(|cost| named_child(cost, "amount"))
            .and_then(|amount| named_child(amount, "commodity_name"))
            .map(|node| Span::from_node(&node));
        spans.postings.push(PostingSpans {
            account: Span::from_node(&account),
            commodity,
            transaction_cost: transaction_cost.map(|node| Span::from_node(&node)),
            transaction_cost_commodity,
        });
    }

    spans
}

fn named_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn named_children<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .collect()
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
struct IncludeIssue {
    /// The path of the file containing the `include` directive.
    path: PathBuf,
    /// The `include` directive itself (raw path and span within `path`).
    include: ParsedInclude,
    /// The stable code identifying why this include could not be resolved.
    code: DiagnosticCode,
    /// A human-readable description of why this include could not be
    /// resolved.
    message: String,
}

/// Returns whether an include issue means part of the project is missing, so
/// that business validation would be unsound.
///
/// An include cycle is the only non-dropping issue: every reachable file was
/// still analyzed, so validation can continue.
fn include_drops_content(code: DiagnosticCode) -> bool {
    !matches!(code, DiagnosticCode::IncludeCycle)
}

/// Severity of an include issue: dropping content is an error, a cycle is a
/// warning.
fn include_severity(code: DiagnosticCode) -> DiagnosticSeverity {
    if include_drops_content(code) {
        DiagnosticSeverity::Error
    } else {
        DiagnosticSeverity::Warning
    }
}

/// What a project analysis could not recover.
///
/// A project analysis is *partial* when part of its content could not be
/// turned into a domain entry: a file with a syntax or lowering error, or an
/// include whose target could not be loaded.
///
/// A partial analysis is not a failed one: the business checks that do not
/// depend on the missing content are still reported. The checks that do
/// depend on it are suppressed, and the names they were about are listed
/// here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartialAnalysis {
    /// The aggregate uncertainty, as handed to the validator.
    pub missing: MissingData,
    /// The per-file breakdown of the losses.
    pub files: Vec<FileLoss>,
}

/// What a single file's analysis could not recover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLoss {
    /// The file the loss comes from.
    pub path: PathBuf,
    /// The number of top-level chunks of the file's source that could not be
    /// turned into a reliable domain entry: either the directive did not
    /// lower at all, or it holds a syntax error.
    pub lost_directives: usize,
    /// The names this file's loss leaves uncertain.
    pub missing: MissingData,
}

/// The result of a multi-file, directive-by-directive analysis rooted at an
/// entry path, following `include` directives via a [`SourceLoader`].
///
/// Like [`ParsedFile`]/[`analyze_file`], this never fails on file content:
/// syntax and lowering issues are reported per-file via each
/// [`AnalyzedFile::file`]. It can, however, fail to even load the entry
/// path itself (see [`analyze_file_with_loader`]'s return type); everything
/// reachable from includes is instead reported as include diagnostics so a
/// single unreadable or cyclic include does not abort the whole analysis.
pub struct AnalyzedProject {
    files: Vec<AnalyzedFile>,
    include_issues: Vec<IncludeIssue>,
    business_diagnostics: Vec<BusinessDiagnostic>,
    source_order: Vec<EntryRef>,
    validated_journal: Option<Journal>,
    validation: ValidationOptions,
    partial: Option<PartialAnalysis>,
}

impl AnalyzedProject {
    /// The files reached from the entry path, in the order they were first
    /// encountered (entry path first, then each include depth-first). A
    /// file that is `include`d from more than one place (a "diamond"
    /// include) appears only once.
    pub fn files(&self) -> &[AnalyzedFile] {
        &self.files
    }

    /// Returns whether at least one error diagnostic is present.
    pub fn has_errors(&self) -> bool {
        self.diagnostics().iter().any(Diagnostic::is_error)
    }

    /// Returns whether at least one warning diagnostic is present.
    pub fn has_warnings(&self) -> bool {
        self.diagnostics().iter().any(Diagnostic::is_warning)
    }

    /// Returns whether the analysis could recover the whole project.
    ///
    /// A partial analysis is one where part of the content could not be
    /// turned into a domain entry: the business checks that depend on the
    /// missing content were skipped. See [`AnalyzedProject::partial`] for the
    /// details.
    pub fn is_partial(&self) -> bool {
        self.partial.is_some()
    }

    /// Returns what the analysis could not recover, when it is partial.
    ///
    /// `None` means the whole project was analyzed: every business check ran.
    pub fn partial(&self) -> Option<&PartialAnalysis> {
        self.partial.as_ref()
    }

    /// Returns the structurally valid journal in source order.
    ///
    /// This does not run business validation. An incomplete or syntactically
    /// invalid project returns its structural diagnostics instead.
    pub fn journal(&self) -> Result<Journal, Vec<Diagnostic>> {
        let diagnostics = self.structural_diagnostics();
        if diagnostics.iter().any(Diagnostic::is_error) {
            return Err(diagnostics);
        }
        Ok(self.source_order_journal())
    }

    /// Returns the chronologically validated journal, including inferred
    /// transaction amounts, or every diagnostic preventing validation.
    ///
    /// Warnings (e.g. an undeclared payee when the `[validation] payee`
    /// level is `warn`) do not prevent validation; only error-severity
    /// diagnostics do.
    pub fn validated_journal(&self) -> Result<Journal, Vec<Diagnostic>> {
        let structural = self.structural_diagnostics();
        if structural.iter().any(Diagnostic::is_error) {
            return Err(structural);
        }
        if self.business_diagnostics.iter().any(|diagnostic| {
            business_severity(diagnostic.code, &self.validation) == Some(DiagnosticSeverity::Error)
        }) {
            return Err(self.diagnostics());
        }
        self.validated_journal
            .clone()
            .ok_or_else(|| self.diagnostics())
    }

    #[cfg(test)]
    fn include_issues(&self) -> &[IncludeIssue] {
        &self.include_issues
    }

    #[cfg(test)]
    fn business_diagnostics(&self) -> &[BusinessDiagnostic] {
        &self.business_diagnostics
    }

    /// Returns all diagnostics for the project in source-analysis order.
    ///
    /// This is the canonical API for consumers such as the CLI and the LSP:
    /// callers do not need to know how syntax, includes, lowering, and
    /// business validation are stored internally.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.structural_diagnostics();

        diagnostics.extend(self.business_diagnostics.iter().filter_map(|diagnostic| {
            Some(Diagnostic {
                path: diagnostic.path.clone(),
                span: diagnostic.span,
                severity: business_severity(diagnostic.code, &self.validation)?,
                source: DiagnosticSource::Business,
                code: diagnostic.code,
                message: diagnostic.message.clone(),
            })
        }));

        diagnostics
    }

    /// Returns syntax, lowering, include and completeness diagnostics,
    /// excluding business validation diagnostics.
    ///
    /// The completeness notices are warnings: they report that part of a
    /// file could not be analyzed, and therefore that some of the checks
    /// depending on it were skipped. They never make the project invalid.
    pub fn structural_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for file in &self.files {
            diagnostics.extend(file_diagnostics(&file.path, &file.file));
        }

        for issue in &self.include_issues {
            diagnostics.push(Diagnostic {
                path: issue.path.clone(),
                span: issue.include.span,
                severity: include_severity(issue.code),
                source: DiagnosticSource::Include,
                code: issue.code,
                message: issue.message.clone(),
            });
        }

        diagnostics
    }

    fn source_order_journal(&self) -> Journal {
        Journal {
            entries: self
                .source_order
                .iter()
                .map(|entry_ref| {
                    self.files[entry_ref.file_index].file.entries()[entry_ref.entry_index]
                        .entry
                        .clone()
                })
                .collect(),
        }
    }
}

impl Diagnostic {
    /// Returns whether this diagnostic is an error.
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Error)
    }

    /// Returns whether this diagnostic is a warning.
    pub const fn is_warning(&self) -> bool {
        matches!(self.severity, DiagnosticSeverity::Warning)
    }
}

fn file_diagnostics(path: &Path, file: &ParsedFile) -> Vec<Diagnostic> {
    let mut diagnostics = file
        .syntax_diagnostics()
        .iter()
        .map(|diagnostic| Diagnostic {
            path: path.to_path_buf(),
            span: diagnostic.span,
            severity: DiagnosticSeverity::Error,
            source: DiagnosticSource::Syntax,
            code: DiagnosticCode::SyntaxError,
            message: diagnostic.message.clone(),
        })
        .collect::<Vec<_>>();

    diagnostics.extend(
        file.lowering_diagnostics()
            .iter()
            .map(|diagnostic| Diagnostic {
                path: path.to_path_buf(),
                span: diagnostic.span,
                severity: DiagnosticSeverity::Error,
                source: DiagnosticSource::Lowering,
                code: DiagnosticCode::LoweringError,
                message: diagnostic.message.clone(),
            }),
    );

    // The completeness notice comes last: it qualifies the checks that are
    // missing because of what could not be analyzed.
    diagnostics.extend(analysis_incomplete_diagnostic(path, file));

    diagnostics
}

/// Builds the notice reporting that part of a file could not be analyzed.
///
/// It is a warning: it never prevents the journal from being validated, it
/// only explains why some of the checks that depend on the missing content
/// were skipped.
fn analysis_incomplete_diagnostic(path: &Path, file: &ParsedFile) -> Option<Diagnostic> {
    let count = file.lost_directives();
    if count == 0 {
        return None;
    }

    // Point at the first piece of source that could not be analyzed.
    let span = file
        .lowering_diagnostics()
        .first()
        .map(|diagnostic| diagnostic.span)
        .or_else(|| {
            file.syntax_diagnostics()
                .first()
                .map(|diagnostic| diagnostic.span)
        })
        .unwrap_or_default();

    let (noun, pronoun) = if count == 1 {
        ("directive", "it")
    } else {
        ("directives", "them")
    };

    Some(Diagnostic {
        path: path.to_path_buf(),
        span,
        severity: DiagnosticSeverity::Warning,
        source: DiagnosticSource::Analysis,
        code: DiagnosticCode::AnalysisIncomplete,
        message: format!(
            "{count} {noun} could not be analyzed: the checks that depend on {pronoun} were skipped"
        ),
    })
}

/// Analyzes `entry_path` and, recursively, every file it (transitively)
/// `include`s, resolving each include's path via `loader` rather than
/// reading real files directly.
///
/// This is the multi-file counterpart to [`analyze_file`]: where
/// `analyze_file` reports `include` directives without following them,
/// `analyze_file_with_loader` follows them while keeping each file's entries
/// and diagnostics separate (see [`AnalyzedFile`]). It also exposes a
/// source-order journal and keeps per-file provenance, which
/// matters for tooling that needs to report a diagnostic against the file
/// it actually came from.
///
/// A relative include is resolved against the directory of the file it
/// appears in; an include with no reachable base directory (e.g. the entry
/// path itself has no parent), a target the loader cannot provide, or an
/// include cycle are all reported as include diagnostics rather than causing
/// the whole analysis to fail. Only a failure to load `entry_path` itself
/// is returned as an `Err`, since there is then nothing to analyze at all.
pub fn analyze_file_with_loader(
    entry_path: &Path,
    loader: &dyn SourceLoader,
) -> Result<AnalyzedProject, SourceLoadError> {
    analyze_file_with_loader_with_options(entry_path, loader, &ValidationOptions::default())
}

/// Analyzes a project rooted at `entry_path` using explicit validation
/// options.
///
/// This is the same analysis as [`analyze_file_with_loader`], with the
/// severity of configurable business checks (the undeclared payee check and
/// the unused-declaration checks) driven by `validation`. The default
/// options reproduce [`analyze_file_with_loader`] exactly.
///
/// # Errors
///
/// Like [`analyze_file_with_loader`], only a failure to load `entry_path`
/// itself is returned as an error; problems in included files are reported
/// as include diagnostics.
pub fn analyze_file_with_loader_with_options(
    entry_path: &Path,
    loader: &dyn SourceLoader,
    validation: &ValidationOptions,
) -> Result<AnalyzedProject, SourceLoadError> {
    let mut files = Vec::new();
    let mut include_issues = Vec::new();
    let mut source_order = Vec::new();
    // Paths currently being visited, i.e. on the path from the entry point
    // to the file currently being processed: used to detect include cycles.
    let mut in_progress = Vec::new();

    visit(
        entry_path,
        loader,
        &mut files,
        &mut include_issues,
        &mut source_order,
        &mut in_progress,
        0,
    )?;

    let missing = aggregate_missing_data(&files, &include_issues);
    let partial = build_partial_analysis(&files, &missing);

    let (business_diagnostics, validated_journal) =
        validate_project(&files, &source_order, validation, &missing);

    Ok(AnalyzedProject {
        files,
        include_issues,
        business_diagnostics,
        source_order,
        validated_journal,
        validation: *validation,
        partial,
    })
}

struct LocatedEntry {
    path: PathBuf,
    span: Span,
    spans: EntrySpans,
    entry: Entry,
}

fn diagnostic_span(entry: &LocatedEntry, error: &ValidationError) -> Span {
    match error {
        ValidationError::AccountAlreadyOpen { .. } | ValidationError::AccountNotOpen { .. } => {
            let account = match error {
                ValidationError::AccountAlreadyOpen { account, .. }
                | ValidationError::AccountNotOpen { account, .. } => account,
                _ => unreachable!("account validation error matched above"),
            };

            entry
                .spans
                .postings
                .iter()
                .zip(transaction_postings(&entry.entry).iter())
                .find_map(|(spans, posting)| (posting.account == *account).then_some(spans.account))
                .or(entry.spans.account)
                .unwrap_or(entry.span)
        }
        ValidationError::CommodityAlreadyDeclared { .. }
        | ValidationError::CommodityNotDeclared { .. } => {
            let commodity = match error {
                ValidationError::CommodityAlreadyDeclared { commodity, .. }
                | ValidationError::CommodityNotDeclared { commodity, .. } => commodity,
                _ => unreachable!("commodity declaration error matched above"),
            };

            entry
                .spans
                .postings
                .iter()
                .zip(transaction_postings(&entry.entry).iter())
                .find_map(|(spans, posting)| {
                    if posting
                        .amount
                        .as_ref()
                        .is_some_and(|amount| amount.commodity == *commodity)
                    {
                        spans.commodity
                    } else if posting
                        .transaction_cost
                        .as_ref()
                        .is_some_and(|cost| cost.amount.commodity == *commodity)
                    {
                        spans.transaction_cost_commodity
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    let Entry::Price(price) = &entry.entry else {
                        return None;
                    };
                    entry
                        .spans
                        .commodities
                        .iter()
                        .zip([&price.base, &price.quote.commodity])
                        .find_map(|(span, candidate)| (candidate == commodity).then_some(*span))
                })
                .or_else(|| entry.spans.commodities.first().copied())
                .unwrap_or(entry.span)
        }
        ValidationError::PayeeAlreadyDeclared { .. } | ValidationError::PayeeNotDeclared { .. } => {
            entry.spans.payee.unwrap_or(entry.span)
        }
        ValidationError::CommodityNotAllowed { commodity, .. } => entry
            .spans
            .postings
            .iter()
            .zip(transaction_postings(&entry.entry).iter())
            .find_map(|(spans, posting)| {
                posting
                    .amount
                    .as_ref()
                    .filter(|amount| amount.commodity == *commodity)
                    .and(spans.commodity)
            })
            .or(entry.spans.account)
            .unwrap_or(entry.span),
        ValidationError::AccountUnused { .. } | ValidationError::CommodityUnused { .. } => {
            // The declaration itself is the offending entry: point at the
            // declared name rather than at the whole directive.
            match error {
                ValidationError::AccountUnused { .. } => entry.spans.account.unwrap_or(entry.span),
                ValidationError::CommodityUnused { .. } => entry
                    .spans
                    .commodities
                    .first()
                    .copied()
                    .unwrap_or(entry.span),
                _ => unreachable!("unused declaration error matched above"),
            }
        }
        ValidationError::PayeeUnused { .. } => entry.spans.payee.unwrap_or(entry.span),
        ValidationError::BalanceAssertionFailed { .. } => entry.span,
        ValidationError::TransactionCostNotPositive { account, .. }
        | ValidationError::TransactionCostInconsistent { account, .. } => entry
            .spans
            .postings
            .iter()
            .zip(transaction_postings(&entry.entry).iter())
            .find_map(|(spans, posting)| {
                (posting.account == *account && posting.transaction_cost.is_some())
                    .then_some(spans.transaction_cost)
                    .flatten()
            })
            .unwrap_or(entry.span),
        ValidationError::PriceNotPositive { .. } | ValidationError::PriceConflict { .. } => {
            entry.span
        }
        ValidationError::TooManyPostingsWithoutAmount { .. }
        | ValidationError::AmbiguousAutoBalanceCommodity { .. }
        | ValidationError::TransactionNotBalanced { .. } => entry.span,
    }
}

fn transaction_postings(entry: &Entry) -> &[pactole_core::Posting] {
    match entry {
        Entry::Transaction(transaction) => &transaction.postings,
        _ => &[],
    }
}

#[derive(Debug, Clone, Copy)]
struct EntryRef {
    file_index: usize,
    entry_index: usize,
}

enum SourceItem {
    Entry(usize),
    Include(ParsedInclude),
}

fn validate_project(
    files: &[AnalyzedFile],
    source_order: &[EntryRef],
    validation: &ValidationOptions,
    missing: &MissingData,
) -> (Vec<BusinessDiagnostic>, Option<Journal>) {
    let located_entries: Vec<LocatedEntry> = source_order
        .iter()
        .map(|entry_ref| {
            let file = &files[entry_ref.file_index];
            let entry = &file.file.entries()[entry_ref.entry_index];
            LocatedEntry {
                path: file.path.clone(),
                span: entry.span,
                spans: entry.spans.clone(),
                entry: entry.entry.clone(),
            }
        })
        .collect();

    let journal = Journal {
        entries: located_entries
            .iter()
            .map(|entry| entry.entry.clone())
            .collect(),
    };
    // Unused-declaration issues carry positions in that same source-order
    // journal, so they must be collected before validation consumes it.
    let unused_issues = unused_declarations_with(&journal, missing);
    let report = validate_journal_all_with(journal, missing);

    let to_diagnostic = |issue: &ValidationIssue| {
        debug_assert!(issue.input_index < located_entries.len());
        let entry = &located_entries[issue.input_index];
        BusinessDiagnostic::from_error(
            entry.path.clone(),
            diagnostic_span(entry, &issue.error),
            &issue.error,
        )
    };

    // Validation issues first, then unused declarations: the latter are
    // cleanup hints, reported after the problems that break the journal.
    let mut diagnostics: Vec<BusinessDiagnostic> =
        report.issues.iter().map(to_diagnostic).collect();
    diagnostics.extend(unused_issues.iter().map(to_diagnostic));

    // Warnings do not prevent a validated journal: only error-severity
    // business diagnostics do.
    let validated_journal = if diagnostics.iter().any(|diagnostic| {
        business_severity(diagnostic.code, validation) == Some(DiagnosticSeverity::Error)
    }) {
        None
    } else {
        Some(report.journal)
    };

    (diagnostics, validated_journal)
}

/// Aggregates the uncertainty of every analyzed file into a single
/// [`MissingData`].
///
/// An include whose target could not be loaded drops an unknown amount of
/// content: every completeness-dependent check is then uncertain.
fn aggregate_missing_data(files: &[AnalyzedFile], include_issues: &[IncludeIssue]) -> MissingData {
    let mut missing = MissingData {
        everything: include_issues
            .iter()
            .any(|issue| include_drops_content(issue.code)),
        ..MissingData::default()
    };

    for file in files {
        let file_missing = file.file.missing();
        missing
            .accounts
            .extend(file_missing.accounts.iter().cloned());
        missing
            .commodities
            .extend(file_missing.commodities.iter().cloned());
        missing.payees.extend(file_missing.payees.iter().cloned());
        missing
            .used_accounts
            .extend(file_missing.used_accounts.iter().cloned());
        missing
            .used_commodities
            .extend(file_missing.used_commodities.iter().cloned());
        missing
            .used_payees
            .extend(file_missing.used_payees.iter().cloned());
    }

    missing
}

/// Builds the per-file breakdown of what the project analysis could not
/// recover, or `None` when the whole project was analyzed.
fn build_partial_analysis(
    files: &[AnalyzedFile],
    missing: &MissingData,
) -> Option<PartialAnalysis> {
    let losses: Vec<FileLoss> = files
        .iter()
        .filter(|file| file.file.lost_directives() > 0)
        .map(|file| FileLoss {
            path: file.path.clone(),
            lost_directives: file.file.lost_directives(),
            missing: file.file.missing().clone(),
        })
        .collect();

    // An include that dropped its content leaves nothing to point at, but
    // the project is still a partial analysis.
    let incomplete = !losses.is_empty() || missing.everything;
    if !incomplete {
        return None;
    }

    Some(PartialAnalysis {
        missing: missing.clone(),
        files: losses,
    })
}

/// A hard cap on include recursion depth, as a safety net against pathological
/// include graphs that lexical normalization and cycle detection alone might
/// not catch (e.g. a very long, strictly non-repeating chain of includes).
/// This is not expected to be hit in practice.
const MAX_INCLUDE_DEPTH: usize = 256;

fn source_items(file: &ParsedFile) -> Vec<SourceItem> {
    let mut entries = file.entries().iter().enumerate().peekable();
    let mut includes = file.includes().iter().cloned().peekable();
    let mut items = Vec::new();

    while entries.peek().is_some() || includes.peek().is_some() {
        let take_entry = match (entries.peek(), includes.peek()) {
            (Some((_, entry)), Some(include)) => entry.span.start.byte <= include.span.start.byte,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        if take_entry {
            let Some((entry_index, _)) = entries.next() else {
                break;
            };
            items.push(SourceItem::Entry(entry_index));
        } else {
            let Some(include) = includes.next() else {
                break;
            };
            items.push(SourceItem::Include(include));
        }
    }

    items
}

/// Lexically normalizes `path` by collapsing `.` and `..` components without
/// touching the filesystem (no symlink resolution, no existence check).
///
/// This ensures that equivalent-but-textually-different includes (e.g.
/// `a/./b.pactole`, `a/../a/b.pactole`, and `a/b.pactole`) are recognized as
/// the same file for cycle detection and diamond-include deduplication,
/// instead of being treated as distinct paths and recursing without bound.
/// Absolute paths are preserved as absolute; this only collapses components,
/// it never resolves a relative path against a base directory.
///
/// Exposed so callers comparing an externally-computed path (e.g. a
/// configured declaration file) against [`AnalyzedProject::files`] can use
/// the exact same normalization.
pub fn normalize_lexically(path: &Path) -> PathBuf {
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
                if ret.file_name().is_some() {
                    ret.pop();
                } else if !ret.has_root() {
                    ret.push(component.as_os_str());
                }
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
    source_order: &mut Vec<EntryRef>,
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

    let items = source_items(&parsed);
    let file_index = files.len();

    files.push(AnalyzedFile {
        path: path.clone(),
        file: parsed,
    });

    in_progress.push(path.clone());

    for item in items {
        let include = match item {
            SourceItem::Entry(entry_index) => {
                source_order.push(EntryRef {
                    file_index,
                    entry_index,
                });
                continue;
            }
            SourceItem::Include(include) => include,
        };

        let raw = PathBuf::from(&include.path);
        let resolved = if raw.is_absolute() {
            raw
        } else {
            match &base_dir {
                Some(dir) => dir.join(&raw),
                None => {
                    include_issues.push(IncludeIssue {
                        path: path.clone(),
                        code: DiagnosticCode::IncludeNoBaseDirectory,
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
                code: DiagnosticCode::IncludeCycle,
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
                code: DiagnosticCode::IncludeDepthExceeded,
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
            source_order,
            in_progress,
            depth + 1,
        ) {
            Ok(()) => {}
            Err(SourceLoadError::NotFound(missing)) => {
                include_issues.push(IncludeIssue {
                    path: path.clone(),
                    code: DiagnosticCode::IncludeNotFound,
                    message: format!("included file not found: `{}`", missing.display()),
                    include,
                });
            }
            Err(err) => {
                include_issues.push(IncludeIssue {
                    path: path.clone(),
                    code: DiagnosticCode::IncludeLoadFailed,
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

    /// Whether `code` is one of the configurable unused-declaration codes.
    fn is_unused(code: DiagnosticCode) -> bool {
        matches!(
            code,
            DiagnosticCode::AccountUnused
                | DiagnosticCode::CommodityUnused
                | DiagnosticCode::PayeeUnused
        )
    }

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
    fn business_diagnostic_keeps_the_originating_file_and_span() {
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
        let diagnostics = project.business_diagnostics();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].path,
            Path::new("/journal/transactions.pactole")
        );
        assert_eq!(diagnostics[0].code, DiagnosticCode::PayeeNotDeclared);
        assert_eq!(diagnostics[0].span.start.row, 0);
        assert_eq!(diagnostics[0].code.as_str(), "payee-not-declared");
    }

    fn undeclared_payee_project() -> (InMemorySourceLoader, &'static str) {
        let source = "2026-01-01 open Assets:Checking\n\
                      2026-01-01 open Expenses:Food\n\
                      2026-01-01 commodity EUR\n\
                      2026-01-02 * \"Unknown\"\n\
                      \u{20}\u{20}Assets:Checking -10 EUR\n\
                      \u{20}\u{20}Expenses:Food 10 EUR\n";
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);
        (loader, source)
    }

    #[test]
    fn error_level_makes_an_undeclared_payee_blocking() {
        let (loader, _) = undeclared_payee_project();

        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &ValidationOptions::default(),
        )
        .unwrap();

        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::PayeeNotDeclared)
            .expect("missing payee diagnostic");
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
        assert!(project.validated_journal().is_err());
    }

    #[test]
    fn warn_level_reports_an_undeclared_payee_as_a_warning_and_still_validates() {
        let (loader, _) = undeclared_payee_project();
        let validation = ValidationOptions {
            payee: ValidationLevel::Warning,
            unused: ValidationLevel::Warning,
        };

        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &validation,
        )
        .unwrap();

        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::PayeeNotDeclared)
            .expect("missing payee diagnostic");
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert!(project.has_warnings());
        assert!(!project.has_errors());
        assert!(project.validated_journal().is_ok());
    }

    #[test]
    fn off_level_suppresses_an_undeclared_payee_diagnostic() {
        let (loader, _) = undeclared_payee_project();
        let validation = ValidationOptions {
            payee: ValidationLevel::Off,
            unused: ValidationLevel::Warning,
        };

        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &validation,
        )
        .unwrap();

        assert!(
            !project
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::PayeeNotDeclared)
        );
        assert!(!project.has_warnings());
        assert!(!project.has_errors());
        assert!(project.validated_journal().is_ok());
    }

    /// A project where only some declarations are unused: `Expenses:Unused`,
    /// `USD` and the `Never` payee are dead, everything else is exercised.
    fn unused_declarations_source() -> &'static str {
        "2026-01-01 open Assets:Checking\n\
         2026-01-01 open Expenses:Food\n\
         2026-01-01 open Expenses:Unused\n\
         2026-01-01 commodity EUR\n\
         2026-01-01 commodity USD\n\
         payee \"Used\"\n\
         payee \"Never\"\n\
         2026-01-02 * \"Used\"\n\
         \u{20}\u{20}Assets:Checking -10 EUR\n\
         \u{20}\u{20}Expenses:Food 10 EUR\n"
    }

    fn unused_project(validation: &ValidationOptions) -> AnalyzedProject {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", unused_declarations_source());
        analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            validation,
        )
        .unwrap()
    }

    #[test]
    fn unused_declarations_are_warnings_by_default_and_point_at_the_declaration() {
        let source = unused_declarations_source();
        let project = unused_project(&ValidationOptions::default());
        let diagnostics = project.diagnostics();

        let unused: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| is_unused(diagnostic.code))
            .collect();
        assert_eq!(unused.len(), 3, "{diagnostics:?}");
        for diagnostic in &unused {
            assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
            assert_eq!(diagnostic.source, DiagnosticSource::Business);
            assert_eq!(diagnostic.path, Path::new("/journal/main.pactole"));
        }

        let codes: Vec<DiagnosticCode> = unused.iter().map(|diagnostic| diagnostic.code).collect();
        // Journal order: the unused `open`, then the unused commodity, then
        // the unused payee declaration.
        assert_eq!(
            codes,
            [
                DiagnosticCode::AccountUnused,
                DiagnosticCode::CommodityUnused,
                DiagnosticCode::PayeeUnused
            ]
        );

        let span_of = |code: DiagnosticCode| {
            unused
                .iter()
                .find(|diagnostic| diagnostic.code == code)
                .map(|diagnostic| source[diagnostic.span.byte_range()].to_string())
                .unwrap_or_else(|| panic!("missing {code}"))
        };
        assert_eq!(span_of(DiagnosticCode::AccountUnused), "Expenses:Unused");
        assert_eq!(span_of(DiagnosticCode::CommodityUnused), "USD");
        assert_eq!(span_of(DiagnosticCode::PayeeUnused), "\"Never\"");

        // Warnings never block: the journal still validates.
        assert!(!project.has_errors());
        assert!(project.has_warnings());
        assert!(project.validated_journal().is_ok());
    }

    #[test]
    fn off_level_suppresses_unused_declarations() {
        let project = unused_project(&ValidationOptions {
            payee: ValidationLevel::Error,
            unused: ValidationLevel::Off,
        });

        assert!(
            !project
                .diagnostics()
                .iter()
                .any(|diagnostic| is_unused(diagnostic.code))
        );
        assert!(!project.has_warnings());
        assert!(project.validated_journal().is_ok());
    }

    #[test]
    fn error_level_makes_an_unused_declaration_blocking() {
        let project = unused_project(&ValidationOptions {
            payee: ValidationLevel::Error,
            unused: ValidationLevel::Error,
        });

        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::AccountUnused)
            .expect("missing account diagnostic");
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
        assert!(project.validated_journal().is_err());
    }

    #[test]
    fn warn_level_does_not_downgrade_other_business_errors() {
        let source = "2026-01-01 commodity EUR\n\
                      2026-01-02 * \"Unknown\"\n\
                      \u{20}\u{20}Assets:Missing 10 EUR\n\
                      \u{20}\u{20}Assets:Checking -10 EUR\n";
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);

        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &ValidationOptions {
                payee: ValidationLevel::Warning,
                unused: ValidationLevel::Warning,
            },
        )
        .unwrap();

        let account = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::AccountNotOpen)
            .expect("missing account diagnostic");
        assert_eq!(account.severity, DiagnosticSeverity::Error);
        assert!(project.has_errors());
        assert!(project.validated_journal().is_err());
    }

    #[test]
    fn business_diagnostics_target_the_relevant_source_elements() {
        let source = "2026-01-01 open Assets:Checking\n\
                      2026-01-01 commodity EUR\n\
                      2026-01-02 * \"Unknown\"\n\
                      \u{20}\u{20}Assets:Missing 10 GBP\n\
                      \u{20}\u{20}Assets:Checking -10 GBP\n\
                      2026-01-03 balance Assets:Checking 1 EUR\n";
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let diagnostics = project.diagnostics();

        let diagnostic = |code: DiagnosticCode| {
            diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == code)
                .unwrap_or_else(|| panic!("missing diagnostic {code}: {diagnostics:?}"))
        };

        let payee = diagnostic(DiagnosticCode::PayeeNotDeclared);
        let payee_start = source.find("\"Unknown\"").unwrap();
        assert_eq!(
            payee.span.byte_range(),
            payee_start..payee_start + "\"Unknown\"".len()
        );

        let account = diagnostic(DiagnosticCode::AccountNotOpen);
        let account_start = source.find("Assets:Missing").unwrap();
        assert_eq!(
            account.span.byte_range(),
            account_start..account_start + "Assets:Missing".len()
        );

        let commodity = diagnostic(DiagnosticCode::CommodityNotDeclared);
        let commodity_start = source.find("GBP").unwrap();
        assert_eq!(
            commodity.span.byte_range(),
            commodity_start..commodity_start + "GBP".len()
        );

        let balance = diagnostic(DiagnosticCode::BalanceAssertionFailed);
        let balance_start = source.find("2026-01-03 balance").unwrap();
        let balance_end = source[balance_start..].find('\n').unwrap() + balance_start + 1;
        assert_eq!(balance.span.byte_range(), balance_start..balance_end);
    }

    #[test]
    fn a_price_with_an_undeclared_commodity_points_at_that_commodity() {
        let source = "2026-01-01 commodity EUR\n2026-01-02 price USD 0.92 EUR\n";
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::CommodityNotDeclared)
            .expect("undeclared price commodity diagnostic");
        let expected_start = source.find("USD").unwrap();

        assert_eq!(
            diagnostic.span.byte_range(),
            expected_start..expected_start + "USD".len()
        );
    }

    #[test]
    fn a_transaction_cost_with_an_undeclared_commodity_points_at_that_commodity() {
        let source = concat!(
            "2026-01-01 commodity USD\n",
            "2026-01-01 open Assets:USD\n",
            "2026-01-01 open Assets:Counter\n",
            "2026-01-02 * \"Exchange\"\n",
            "  Assets:USD -100 USD @ 0.93 EUR\n",
            "  Assets:Counter 100 USD\n",
        );
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::CommodityNotDeclared)
            .expect("undeclared transaction-cost commodity diagnostic");
        let expected_start = source.find("EUR").unwrap();

        assert_eq!(
            diagnostic.span.byte_range(),
            expected_start..expected_start + "EUR".len()
        );
    }

    #[test]
    fn transaction_cost_validation_points_at_cost_after_an_uncosted_same_account_posting() {
        let source = concat!(
            "2026-01-01 commodity USD\n",
            "2026-01-01 commodity EUR\n",
            "2026-01-01 open Assets:Checking\n",
            "2026-01-01 open Assets:Counter\n",
            "2026-01-02 * \"Exchange\"\n",
            "  Assets:Checking 100 USD\n",
            "  Assets:Checking 3 USD @@ -1 EUR\n",
            "  Assets:Counter -103 USD\n",
        );
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", source);

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::TransactionCostNotPositive)
            .expect("non-positive cost diagnostic");
        let expected_start = source.find("@@ -1 EUR").unwrap();

        assert_eq!(
            diagnostic.span.byte_range(),
            expected_start..expected_start + "@@ -1 EUR".len()
        );
    }

    #[test]
    fn conflicting_prices_across_included_files_are_reported_on_the_second_quote() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 commodity EUR\n\
             2026-01-01 commodity USD\n\
             2026-01-02 price USD 0.92 EUR\n\
             include \"rates.pactole\"\n",
        );
        let rates = "2026-01-02 price USD 0.93 EUR\n";
        loader.insert("/journal/rates.pactole", rates);

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let diagnostic = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::PriceConflict)
            .expect("conflicting price diagnostic");

        assert_eq!(diagnostic.path, Path::new("/journal/rates.pactole"));
        assert_eq!(diagnostic.span.byte_range(), 0..rates.len());
    }

    #[test]
    fn include_entries_are_validated_at_their_source_position() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"accounts.pactole\"\n\
             2026-01-01 commodity EUR\n\
             payee \"Shop\"\n\
             2026-01-01 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );
        loader.insert(
            "/journal/accounts.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(project.diagnostics().is_empty());
    }

    #[test]
    fn entries_after_an_include_keep_their_source_order_on_equal_dates() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 commodity EUR\n\
             payee \"Shop\"\n\
             2026-01-01 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n\
             include \"accounts.pactole\"\n",
        );
        loader.insert(
            "/journal/accounts.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(
            project
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::AccountNotOpen)
        );
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
    fn missing_include_suppresses_partial_business_validation() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"missing.pactole\"\n\
             2026-01-02 * \"Unknown\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let diagnostics = project.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::IncludeNotFound);
        assert!(project.business_diagnostics().is_empty());
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
    fn include_cycle_does_not_suppress_business_validation() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/a.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n\
             2026-01-01 commodity EUR\n\
             include \"b.pactole\"\n",
        );
        loader.insert(
            "/journal/b.pactole",
            "include \"a.pactole\"\n\
             2026-01-02 * \"Unknown\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project = analyze_file_with_loader(Path::new("/journal/a.pactole"), &loader).unwrap();
        let diagnostics = project.diagnostics();

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::IncludeCycle)
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::PayeeNotDeclared)
        );
    }

    #[test]
    fn only_content_dropping_include_issues_block_validation() {
        assert!(include_drops_content(
            DiagnosticCode::IncludeNoBaseDirectory
        ));
        assert!(include_drops_content(DiagnosticCode::IncludeNotFound));
        assert!(include_drops_content(DiagnosticCode::IncludeLoadFailed));
        assert!(include_drops_content(DiagnosticCode::IncludeDepthExceeded));
        assert!(!include_drops_content(DiagnosticCode::IncludeCycle));
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
    fn lexical_normalization_preserves_unmatched_parent_components() {
        assert_eq!(
            normalize_lexically(Path::new("../a")),
            PathBuf::from("../a")
        );
        assert_eq!(
            normalize_lexically(Path::new("a/../../b")),
            PathBuf::from("../b")
        );
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

    /// A project whose `broken.pactole` cannot be lowered (its date is not
    /// a calendar date) and whose `valid.pactole` holds a transaction.
    fn partial_project() -> AnalyzedProject {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"broken.pactole\"\n\
             include \"valid.pactole\"\n",
        );
        loader.insert(
            "/journal/broken.pactole",
            "2026-13-45 open Assets:Checking\n",
        );
        loader.insert(
            "/journal/valid.pactole",
            "2026-01-01 commodity EUR\n\
             2026-01-02 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap()
    }

    #[test]
    fn a_broken_file_does_not_hide_the_business_errors_of_the_other_files() {
        let project = partial_project();

        let codes: Vec<DiagnosticCode> = project
            .business_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();

        assert!(
            codes.contains(&DiagnosticCode::PayeeNotDeclared),
            "unexpected business diagnostics: {codes:?}"
        );
        assert!(
            codes.contains(&DiagnosticCode::AccountNotOpen),
            "unexpected business diagnostics: {codes:?}"
        );
    }

    #[test]
    fn a_partial_project_reports_its_completeness() {
        let project = partial_project();

        assert!(project.is_partial());
        let partial = project.partial().expect("the project is partial");
        assert_eq!(partial.files.len(), 1);
        let loss = &partial.files[0];
        assert_eq!(loss.path, PathBuf::from("/journal/broken.pactole"));
        assert_eq!(loss.lost_directives, 1);
        // The lost directive was an `open`: the account it opened may be
        // missing from the journal.
        assert!(
            loss.missing
                .accounts
                .contains(&pactole_core::AccountName::new("Assets:Checking").unwrap()),
            "unexpected missing accounts: {:?}",
            loss.missing.accounts
        );
    }

    #[test]
    fn a_complete_project_is_not_partial() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "2026-01-01 open Assets:Checking\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(!project.is_partial());
        assert!(project.partial().is_none());
    }

    #[test]
    fn a_top_level_comment_is_not_a_lost_directive() {
        // A comment is not a directive: it declares and uses nothing, so a
        // commented file must not be reported as incomplete.
        let file = analyze_file(
            "; an example journal\n\
             ; spanning several lines\n\
             2026-01-01 open Assets:Checking\n",
        );

        assert_eq!(file.lost_directives(), 0);
    }

    #[test]
    fn a_commented_project_is_not_partial() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "; included on purpose\n\
             include \"other.pactole\"\n",
        );
        loader.insert("/journal/other.pactole", "; just a comment\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(
            !project.is_partial(),
            "unexpected losses: {:?}",
            project.partial()
        );
    }

    #[test]
    fn the_completeness_notice_is_reported_as_a_warning() {
        let project = partial_project();

        let notice = project
            .diagnostics()
            .into_iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::AnalysisIncomplete)
            .expect("the project reports its incompleteness");

        assert_eq!(notice.severity, DiagnosticSeverity::Warning);
        assert_eq!(notice.source, DiagnosticSource::Analysis);
        assert_eq!(notice.path, PathBuf::from("/journal/broken.pactole"));
    }

    #[test]
    fn a_lost_open_suppresses_the_account_not_open_error_of_the_other_files() {
        // The `open Assets:Checking` directive of the broken file is lost:
        // the account may have been opened there, so the transaction of the
        // valid file must not be reported as using an unopened account.
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"broken.pactole\"\n\
             include \"valid.pactole\"\n",
        );
        loader.insert(
            "/journal/broken.pactole",
            "2026-13-45 open Assets:Checking\n",
        );
        loader.insert(
            "/journal/valid.pactole",
            "2026-01-02 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let not_open: Vec<&str> = project
            .business_diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::AccountNotOpen)
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            !not_open
                .iter()
                .any(|message| message.contains("Assets:Checking")),
            "unexpected diagnostics: {not_open:?}"
        );
        assert!(
            not_open
                .iter()
                .any(|message| message.contains("Expenses:Food")),
            "unexpected diagnostics: {not_open:?}"
        );
    }

    #[test]
    fn a_lost_transaction_suppresses_the_unused_declaration_warnings() {
        // The lost transaction may have used the account, the commodity and
        // the payee declared by the other file: none of them may be
        // reported as unused.
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"broken.pactole\"\n\
             include \"valid.pactole\"\n",
        );
        loader.insert(
            "/journal/broken.pactole",
            "2026-01-02 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n",
        );
        loader.insert(
            "/journal/valid.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 commodity EUR\n\
             payee \"Shop\"\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let unused: Vec<DiagnosticCode> = project
            .business_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .filter(|code| is_unused(*code))
            .collect();

        assert!(unused.is_empty(), "unexpected diagnostics: {unused:?}");
    }

    #[test]
    fn a_lost_declaration_suppresses_the_already_declared_error() {
        // The lost `open` directive may have opened the account a second
        // time: the duplicate must not be reported.
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"broken.pactole\"\n\
             include \"valid.pactole\"\n",
        );
        loader.insert(
            "/journal/broken.pactole",
            "2026-13-45 open Assets:Checking\n",
        );
        loader.insert(
            "/journal/valid.pactole",
            "2026-01-01 open Assets:Checking\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        let already_open: Vec<DiagnosticCode> = project
            .business_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .filter(|code| *code == DiagnosticCode::AccountAlreadyOpen)
            .collect();

        assert!(
            already_open.is_empty(),
            "unexpected diagnostics: {already_open:?}"
        );
    }

    #[test]
    fn an_include_that_drops_content_makes_the_whole_project_uncertain() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"missing.pactole\"\n\
             2026-01-02 * \"Shop\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();

        assert!(project.is_partial());
        let partial = project.partial().expect("the project is partial");
        assert!(partial.missing.everything);
        // Every check that depends on the completeness of the journal is
        // uncertain, but the transaction's own balancing is not.
        assert!(
            project
                .business_diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.code != DiagnosticCode::AccountNotOpen),
            "unexpected business diagnostics: {:?}",
            project
                .business_diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>()
        );
    }
}
