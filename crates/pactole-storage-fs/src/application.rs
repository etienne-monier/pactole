//! Application-layer adapter for filesystem-backed ledgers.

use std::path::{Path, PathBuf};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
};

use pactole_app::{
    BackendCapabilities, BackendError, BackendOperation, Diagnostic, DiagnosticOrigin,
    DiagnosticSeverity, LedgerReader, LedgerSnapshot, Revision, SourceId, SourcePoint, SourceSpan,
};
use pactole_config::ValidationOptions;
use pactole_syntax::Span;

use crate::{FsSourceLoader, analyze_file_with_loader_with_options};

/// Reads a filesystem project through the shared application boundary.
#[derive(Debug, Clone)]
pub struct FsLedgerReader {
    /// Root journal path used for analysis.
    entry_path: PathBuf,
    /// Validation options applied to the analyzed project.
    validation: ValidationOptions,
}

impl FsLedgerReader {
    /// Creates a reader for the project rooted at `entry_path`.
    pub fn new(entry_path: impl Into<PathBuf>, validation: ValidationOptions) -> Self {
        Self {
            entry_path: entry_path.into(),
            validation,
        }
    }

    /// Returns the root path used by this reader.
    pub fn entry_path(&self) -> &Path {
        &self.entry_path
    }
}

impl LedgerReader for FsLedgerReader {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::read_only()
    }

    fn snapshot(&self) -> Result<LedgerSnapshot, BackendError> {
        let project = analyze_file_with_loader_with_options(
            &self.entry_path,
            &FsSourceLoader,
            &self.validation,
        )
        .map_err(|error| BackendError {
            operation: BackendOperation::LoadSnapshot,
            message: error.to_string(),
        })?;

        let diagnostics = project
            .diagnostics()
            .into_iter()
            .map(convert_diagnostic)
            .collect::<Vec<_>>();

        match project.validated_journal() {
            Ok(journal) => Ok(LedgerSnapshot::Ready {
                revision: revision(&self.entry_path, &journal, &diagnostics)?,
                journal,
                diagnostics,
            }),
            Err(_) => Ok(LedgerSnapshot::Unavailable { diagnostics }),
        }
    }
}

/// Computes a stable revision from the source, journal and diagnostics.
fn revision(
    entry_path: &Path,
    journal: &pactole_core::Journal,
    diagnostics: &[Diagnostic],
) -> Result<Revision, BackendError> {
    let source = fs::read(entry_path).map_err(|error| BackendError {
        operation: BackendOperation::LoadSnapshot,
        message: error.to_string(),
    })?;
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{journal:?}{diagnostics:?}").hash(&mut hasher);
    Ok(Revision(format!("{:016x}", hasher.finish())))
}

/// Converts a filesystem diagnostic into an application diagnostic.
fn convert_diagnostic(diagnostic: crate::Diagnostic) -> Diagnostic {
    Diagnostic {
        source: Some(SourceId(diagnostic.path.display().to_string())),
        span: Some(convert_span(diagnostic.span)),
        severity: match diagnostic.severity {
            crate::DiagnosticSeverity::Error => DiagnosticSeverity::Error,
            crate::DiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
        },
        origin: match diagnostic.source {
            crate::DiagnosticSource::Syntax => DiagnosticOrigin::Syntax,
            crate::DiagnosticSource::Lowering => DiagnosticOrigin::Lowering,
            crate::DiagnosticSource::Include => DiagnosticOrigin::Include,
            crate::DiagnosticSource::Analysis => DiagnosticOrigin::Analysis,
            crate::DiagnosticSource::Business => DiagnosticOrigin::Business,
        },
        code: diagnostic.code,
        message: diagnostic.message,
    }
}

/// Converts a syntax span into an application source span.
fn convert_span(span: Span) -> SourceSpan {
    SourceSpan {
        start: convert_point(span.start),
        end: convert_point(span.end),
    }
}

/// Converts a syntax point into an application source point.
fn convert_point(point: pactole_syntax::Point) -> SourcePoint {
    SourcePoint {
        byte: point.byte,
        line: point.row,
        column: point.column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_app::Capability;
    use std::fs;

    #[test]
    fn reads_a_valid_project_as_a_ready_snapshot() {
        let directory =
            std::env::temp_dir().join(format!("pactole-app-reader-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.pactole");
        fs::write(&path, "2026-01-01 open Assets:Checking\n").unwrap();

        let reader = FsLedgerReader::new(&path, ValidationOptions::default());
        assert!(reader.capabilities().supports(Capability::Read));
        assert!(matches!(
            reader.snapshot(),
            Ok(LedgerSnapshot::Ready { .. })
        ));

        fs::remove_dir_all(directory).ok();
    }

    #[test]
    fn keeps_content_diagnostics_in_an_unavailable_snapshot() {
        let directory =
            std::env::temp_dir().join(format!("pactole-app-reader-invalid-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.pactole");
        fs::write(&path, "not a pactole directive\n").unwrap();

        let snapshot = FsLedgerReader::new(&path, ValidationOptions::default())
            .snapshot()
            .unwrap();
        assert!(matches!(snapshot, LedgerSnapshot::Unavailable { .. }));

        fs::remove_dir_all(directory).ok();
    }
}
