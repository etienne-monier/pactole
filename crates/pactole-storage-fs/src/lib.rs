//! Filesystem-backed parsing, analysis and formatting for Pactole journals.
//!
//! Use [`analyze_file`] for tolerant single-file tooling, or
//! [`analyze_file_with_loader`] for include-aware project analysis. Use
//! [`PactoleFileStorage`] when implementing [`pactole_core::ReadableStorage`]
//! for a local journal, and [`format()`] for canonical source formatting.

use pactole_core::{Journal, ReadableStorage};
use pactole_syntax::SyntaxDiagnostic;
use std::fs;
use std::path::{Path, PathBuf};

use crate::analysis::normalize_lexically;
pub mod analysis;
pub mod errors;
pub mod loader;
mod parser;
mod printer;

pub use crate::analysis::{
    AnalyzedFile, AnalyzedProject, Diagnostic, DiagnosticSeverity, DiagnosticSource, EntrySpans,
    LoweringDiagnostic, ParsedEntry, ParsedFile, ParsedInclude, PostingSpans, analyze_file,
    analyze_file_with_loader,
};
pub use crate::errors::PactoleFsStorageError;
pub use crate::loader::{FsSourceLoader, InMemorySourceLoader, SourceLoadError, SourceLoader};
pub use crate::printer::{
    FormatOptions, format, format_transactions, format_transactions_with_options,
    format_with_options,
};

/// Formats a list of syntax diagnostics into a single human-readable string,
/// prefixed with the position of the first diagnostic (line:column) so callers
/// can locate the issue without inspecting every message.
///
/// Shared by the formatter, which refuses to proceed on a syntactically
/// invalid document.
fn describe_syntax_diagnostics(diagnostics: &[SyntaxDiagnostic]) -> String {
    let position = diagnostics
        .first()
        .map(|d| format!("at {}: ", d.span.start))
        .unwrap_or_default();
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    format!("{}{}", position, messages.join("; "))
}

/// A filesystem-backed journal storage that keeps the root file content in
/// memory while it is parsed.
///
/// Updating [`Self::content`] before calling [`Self::parse`] lets callers
/// analyze unsaved root-file content; included files are loaded through the
/// filesystem loader.
pub struct PactoleFileStorage {
    /// Path of the root journal, when the storage was created from a path.
    pub filepath: Option<PathBuf>,
    /// Current root-file content to parse.
    pub content: String,
    /// Validated journal populated by [`Self::parse`].
    pub journal: Option<Journal>,
}

struct RootContentLoader {
    root: PathBuf,
    content: String,
    fs: FsSourceLoader,
}

impl SourceLoader for RootContentLoader {
    fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
        if normalize_lexically(path) == self.root {
            Ok(self.content.clone())
        } else {
            self.fs.load(path)
        }
    }
}

impl PactoleFileStorage {
    /// Parses the current root content and stores its structurally valid
    /// journal. Include, syntax and lowering errors are returned instead of
    /// producing a partial storage journal.
    pub fn parse(&mut self) -> Result<(), errors::PactoleFsStorageError> {
        let entry_path = self.filepath.clone().unwrap_or_default();
        let loader = RootContentLoader {
            root: normalize_lexically(&entry_path),
            content: self.content.clone(),
            fs: FsSourceLoader,
        };
        let project = analyze_file_with_loader(&entry_path, &loader)
            .map_err(|error| errors::PactoleFsStorageError::ParseError(error.to_string()))?;
        self.journal = Some(project.journal().map_err(describe_project_diagnostics)?);
        Ok(())
    }
}

fn describe_project_diagnostics(diagnostics: Vec<Diagnostic>) -> errors::PactoleFsStorageError {
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.is_error())
        .or_else(|| diagnostics.first());
    match diagnostic {
        Some(diagnostic) => errors::PactoleFsStorageError::ParseError(format!(
            "{}:{}:{}: {}",
            diagnostic.path.display(),
            diagnostic.span.start.row + 1,
            diagnostic.span.start.column + 1,
            diagnostic.message
        )),
        None => errors::PactoleFsStorageError::ParseError("analysis failed".to_string()),
    }
}

impl TryFrom<PathBuf> for PactoleFileStorage {
    type Error = errors::PactoleFsStorageError;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let contents = fs::read_to_string(&value)?;
        Ok(Self {
            filepath: Some(value),
            content: contents,
            journal: None,
        })
    }
}

impl ReadableStorage for PactoleFileStorage {
    type Error = PactoleFsStorageError;

    fn journal(&self) -> Result<Journal, Self::Error> {
        self.journal.clone().ok_or(PactoleFsStorageError::NotParsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn storage_parse_uses_the_current_root_content() {
        let path = std::env::temp_dir().join(format!(
            "pactole-storage-content-{}.pactole",
            std::process::id()
        ));
        fs::write(&path, "2026-01-01 open Assets:Old\n").unwrap();

        let mut storage = PactoleFileStorage::try_from(path.clone()).unwrap();
        storage.content = "2026-01-01 open Assets:New\n".to_string();
        storage.parse().unwrap();

        let journal = storage.journal().unwrap();
        assert!(matches!(
            &journal.entries[0],
            pactole_core::Entry::Open(open) if open.account.to_string() == "Assets:New"
        ));
        fs::remove_file(path).ok();
    }

    #[test]
    fn storage_parse_tolerates_include_cycles() {
        let dir =
            std::env::temp_dir().join(format!("pactole-storage-cycle-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.pactole");
        let other = dir.join("other.pactole");
        fs::write(&main, "include \"other.pactole\"\n").unwrap();
        fs::write(&other, "include \"main.pactole\"\n").unwrap();

        let mut storage = PactoleFileStorage::try_from(main.clone()).unwrap();
        assert!(storage.parse().is_ok());
        assert!(storage.journal().unwrap().entries.is_empty());

        fs::remove_dir_all(dir).ok();
    }
}
