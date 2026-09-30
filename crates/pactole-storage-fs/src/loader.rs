//! An abstraction over "where source text comes from", so that multi-file
//! analysis (see [`crate::analysis::analyze_file_with_loader`]) is not tied
//! to reading real files from the filesystem.
//!
//! A future `pactole-lsp` needs to resolve `include` directives against
//! whatever the editor currently has open (possibly-unsaved buffers), not
//! against what is on disk. [`SourceLoader`] lets both cases share the same
//! multi-file resolution logic: [`FsSourceLoader`] reads real files, while a
//! test/editor-facing implementation (see [`InMemorySourceLoader`]) can serve
//! in-memory content keyed by path.
//!
//! This module intentionally knows nothing about `.pactole` syntax: it only
//! loads raw source text for a given path.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// An error produced while loading the source text for a path via a
/// [`SourceLoader`].
#[derive(Debug, Error)]
pub enum SourceLoadError {
    /// No source is available for this path.
    #[error("source not found: {0}")]
    NotFound(PathBuf),
    /// The path exists but could not be read (e.g. a filesystem I/O error).
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Provides source text for a path, abstracting over where that text
/// actually lives (a file on disk, an editor's open buffers, an in-memory
/// fixture in tests, ...).
///
/// Implementations are expected to be cheap to call repeatedly and are not
/// required to cache anything themselves.
pub trait SourceLoader {
    /// Load the source text at `path`.
    ///
    /// `path` is always the path an `include` directive resolved to (already
    /// joined against the including file's directory when it was relative),
    /// never a raw, unresolved include string.
    fn load(&self, path: &Path) -> Result<String, SourceLoadError>;
}

/// A [`SourceLoader`] that reads source text from real files on disk.
///
/// This is the loader used implicitly by [`crate::parser::parse`] and
/// [`crate::PactoleFileStorage`] (whose behavior is unchanged); it is
/// exposed here so multi-file callers of
/// [`crate::analysis::analyze_file_with_loader`] can opt into the same
/// filesystem-backed resolution explicitly.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsSourceLoader;

impl SourceLoader for FsSourceLoader {
    fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
        fs::read_to_string(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                SourceLoadError::NotFound(path.to_path_buf())
            } else {
                SourceLoadError::Io {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })
    }
}

/// A [`SourceLoader`] backed by an in-memory map of path to source text.
///
/// Useful for tests, and as a stand-in for a future editor-facing loader
/// that would serve currently-open (possibly unsaved) buffers instead of
/// what is on disk.
#[derive(Debug, Default, Clone)]
pub struct InMemorySourceLoader {
    files: HashMap<PathBuf, String>,
}

impl InMemorySourceLoader {
    /// Create an empty in-memory loader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the source text served for `path`.
    pub fn insert(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }
}

impl SourceLoader for InMemorySourceLoader {
    fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| SourceLoadError::NotFound(path.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_loader_reads_an_existing_file() {
        let dir = std::env::temp_dir().join(format!("pactole-loader-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.pactole");
        fs::write(&file, "2026-09-03 open Assets:Checking\n").unwrap();

        let loader = FsSourceLoader;
        let content = loader.load(&file).unwrap();
        assert_eq!(content, "2026-09-03 open Assets:Checking\n");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fs_loader_reports_not_found_for_a_missing_path() {
        let loader = FsSourceLoader;
        let err = loader
            .load(Path::new("/nonexistent/pactole/path.pactole"))
            .unwrap_err();
        assert!(matches!(err, SourceLoadError::NotFound(_)));
    }

    #[test]
    fn in_memory_loader_serves_registered_content() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/virtual/main.pactole", "include \"other.pactole\"\n");

        let content = loader.load(Path::new("/virtual/main.pactole")).unwrap();
        assert_eq!(content, "include \"other.pactole\"\n");
    }

    #[test]
    fn in_memory_loader_reports_not_found_for_an_unregistered_path() {
        let loader = InMemorySourceLoader::new();
        let err = loader
            .load(Path::new("/virtual/missing.pactole"))
            .unwrap_err();
        assert!(matches!(err, SourceLoadError::NotFound(_)));
    }
}
