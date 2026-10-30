use pactole_core::DiagnosticCode;

/// Backend-independent identifier for a source of diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceId(pub String);

/// A source position using byte, line and byte-column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePoint {
    /// Zero-based byte offset in the source.
    pub byte: usize,
    /// Zero-based line number.
    pub line: usize,
    /// Zero-based byte column within the line.
    pub column: usize,
}

/// A half-open source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    /// Start position, inclusive.
    pub start: SourcePoint,
    /// End position, exclusive.
    pub end: SourcePoint,
}

/// Severity of an application diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// A diagnostic that prevents successful processing.
    Error,
    /// A diagnostic that does not prevent successful processing.
    Warning,
}

/// Layer that produced an application diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticOrigin {
    /// Produced while parsing source syntax.
    Syntax,
    /// Produced while lowering syntax into domain data.
    Lowering,
    /// Produced while resolving included sources.
    Include,
    /// Produced while determining whether analysis is complete.
    Analysis,
    /// Produced by domain validation.
    Business,
}

/// A diagnostic that can be consumed by any frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Source file associated with the diagnostic, when known.
    pub source: Option<SourceId>,
    /// Source range associated with the diagnostic, when known.
    pub span: Option<SourceSpan>,
    /// Diagnostic severity.
    pub severity: DiagnosticSeverity,
    /// Layer that produced the diagnostic.
    pub origin: DiagnosticOrigin,
    /// Stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable diagnostic message.
    pub message: String,
}

impl Diagnostic {
    /// Returns whether this diagnostic has error severity.
    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }

    /// Returns whether this diagnostic has warning severity.
    pub fn is_warning(&self) -> bool {
        self.severity == DiagnosticSeverity::Warning
    }
}
