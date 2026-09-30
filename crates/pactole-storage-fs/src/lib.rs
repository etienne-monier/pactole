use pactole_core::{Journal, ReadableStorage};
use pactole_syntax::SyntaxDiagnostic;
use std::fs;
use std::path::PathBuf;
pub mod analysis;
pub mod errors;
pub mod loader;
mod parser;
mod printer;

pub use crate::analysis::{
    AnalyzedFile, AnalyzedProject, IncludeIssue, LoweringDiagnostic, ParsedEntry, ParsedFile,
    ParsedInclude, analyze_file, analyze_file_with_loader,
};
pub use crate::errors::PactoleFsStorageError;
pub use crate::loader::{FsSourceLoader, InMemorySourceLoader, SourceLoadError, SourceLoader};
pub use crate::printer::format;

/// Formats a list of syntax diagnostics into a single human-readable string,
/// prefixed with the position of the first diagnostic (line:column) so callers
/// can locate the issue without inspecting every message.
///
/// Shared by `parser.rs` and `printer.rs`, which both refuse to proceed on a
/// syntactically invalid document.
fn describe_syntax_diagnostics(diagnostics: &[SyntaxDiagnostic]) -> String {
    let position = diagnostics
        .first()
        .map(|d| format!("at {}: ", d.span.start))
        .unwrap_or_default();
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    format!("{}{}", position, messages.join("; "))
}

pub struct PactoleFileStorage {
    pub filepath: Option<PathBuf>,
    pub content: String,
    pub journal: Option<Journal>,
}

impl PactoleFileStorage {
    pub fn parse(self: &mut Self) -> Result<(), errors::PactoleFsStorageError> {
        let base_dir = self.filepath.as_deref().and_then(std::path::Path::parent);
        self.journal = Some(parser::parse(&self.content, base_dir)?);
        Ok(())
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
