//! In-memory state of open text documents.
//!
//! This server uses full-document synchronization
//! ([`lsp_types::TextDocumentSyncKind::FULL`]): each `didChange` notification
//! carries the whole new document text rather than incremental edits, so no
//! LSP-position-to-byte-offset translation is needed to apply changes. This
//! keeps document state trivial (just the latest full text per URI) at the
//! cost of larger `didChange` payloads, which is an acceptable trade-off for
//! `.pactole` files (typically small, hand-edited ledgers).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::Url;
use pactole_storage_fs::{
    AnalyzedProject, FsSourceLoader, SourceLoadError, SourceLoader,
    analyze_file_with_loader_with_options,
};

use crate::config::Config;

/// The in-memory state of every currently-open document, keyed by URI.
#[derive(Default)]
pub struct Documents {
    open: HashMap<Url, String>,
    cached_scope: RefCell<Option<CachedScope>>,
    generation: u64,
}

struct CachedScope {
    entry: PathBuf,
    validation: pactole_config::ValidationOptions,
    project: AnalyzedProject,
}

/// A loader snapshot used by the background project-analysis worker.
pub(crate) struct SnapshotSourceLoader {
    open: HashMap<PathBuf, String>,
    fs: FsSourceLoader,
}

impl Documents {
    /// Creates an empty document store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a newly opened document with its initial full text.
    pub fn open(&mut self, uri: Url, text: String) {
        self.open.insert(uri, text);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Replaces the full text of an already-open document with `text`, the
    /// last (and, under full-document sync, only) entry of a `didChange`
    /// notification's `content_changes`.
    ///
    /// If the document was not previously open (e.g. a stray `didChange`),
    /// it is inserted anyway so subsequent analysis has something to work
    /// with.
    pub fn change(&mut self, uri: Url, text: String) {
        self.open.insert(uri, text);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Forgets a document, e.g. on `didClose`.
    pub fn close(&mut self, uri: &Url) {
        self.open.remove(uri);
        self.generation = self.generation.wrapping_add(1);
    }

    /// The current full text of an open document, if any.
    pub fn get(&self, uri: &Url) -> Option<&str> {
        self.open.get(uri).map(String::as_str)
    }

    /// The current full text of an open document identified by its
    /// filesystem `path`, if any.
    ///
    /// This re-derives a `file://` URI from `path` (mirroring how
    /// `did*TextDocument` notifications key documents) to look it up in the
    /// open-document map; a `path` that cannot be turned into a URI (should
    /// not normally happen for the paths this server deals with) is treated
    /// as not open.
    pub fn get_by_path(&self, path: &Path) -> Option<&str> {
        let uri = Url::from_file_path(path).ok()?;
        self.get(&uri)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn snapshot_loader(&self) -> SnapshotSourceLoader {
        let open = self
            .open
            .iter()
            .filter_map(|(uri, text)| uri.to_file_path().ok().map(|path| (path, text.clone())))
            .collect();
        SnapshotSourceLoader {
            open,
            fs: FsSourceLoader,
        }
    }

    pub(crate) fn store_analyzed_scope(
        &mut self,
        entry: PathBuf,
        validation: pactole_config::ValidationOptions,
        project: AnalyzedProject,
    ) {
        *self.cached_scope.get_mut() = Some(CachedScope {
            entry,
            validation,
            project,
        });
    }

    /// Runs `callback` with the last successfully analyzed project for the
    /// configured journal or current document. A background worker updates this
    /// cache after a document change; this method never performs analysis.
    pub fn with_analyzed_scope<R>(
        &self,
        config: &Config,
        uri: &Url,
        callback: impl FnOnce(Option<&AnalyzedProject>) -> R,
    ) -> R {
        let Some(entry) = config.journal_file.clone().or_else(|| uri_to_path(uri)) else {
            return callback(None);
        };

        let cached = self.cached_scope.borrow();
        let project = cached.as_ref().and_then(|scope| {
            (scope.entry == entry && scope.validation == config.validation)
                .then_some(&scope.project)
        });
        callback(project)
    }
}

impl SourceLoader for SnapshotSourceLoader {
    fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
        if let Some(text) = self.open.get(path) {
            return Ok(text.clone());
        }
        self.fs.load(path)
    }
}

/// A [`SourceLoader`] that lets open, possibly-unsaved editor buffers take
/// precedence over what is on disk.
///
/// `include` resolution must reflect what the user is actually editing: if a
/// file is currently open in the editor with unsaved changes, those changes
/// (held in [`Documents`]) must be used instead of the last-saved-to-disk
/// content that [`FsSourceLoader`] would otherwise read. Falls back to
/// [`FsSourceLoader`] for any path that is not currently open.
pub struct DocumentsSourceLoader<'a> {
    documents: &'a Documents,
    fs: FsSourceLoader,
}

impl<'a> DocumentsSourceLoader<'a> {
    /// Wraps `documents` so it takes precedence over the filesystem when
    /// loading source text.
    pub fn new(documents: &'a Documents) -> Self {
        Self {
            documents,
            fs: FsSourceLoader,
        }
    }
}

impl SourceLoader for DocumentsSourceLoader<'_> {
    fn load(&self, path: &Path) -> Result<String, SourceLoadError> {
        if let Some(text) = self.documents.get_by_path(path) {
            log::trace!("loaded {} from an open buffer", path.display());
            return Ok(text.to_string());
        }
        log::trace!("loaded {} from the filesystem", path.display());
        self.fs.load(path)
    }
}

/// The filesystem path behind a document URI, or `None` for any scheme with
/// no file path (e.g. an `untitled:` buffer).
pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

/// Analyzes the request scope shared by hover, completion and navigation:
/// the configured root journal and its whole include graph, or — when no
/// `journal_file` is configured — the current document as its own entry
/// point, so its own `include`s are followed too. Open buffers take
/// precedence over their on-disk content through [`DocumentsSourceLoader`].
///
/// Returns `None` when neither entry point can be analyzed, in which case
/// callers fall back to the current document alone.
pub fn analyze_scope(config: &Config, documents: &Documents, uri: &Url) -> Option<AnalyzedProject> {
    let loader = DocumentsSourceLoader::new(documents);
    let entry = config.journal_file.clone().or_else(|| uri_to_path(uri))?;
    analyze_file_with_loader_with_options(&entry, &loader, &config.validation).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn open_then_get_returns_the_stored_text() {
        let mut docs = Documents::new();
        docs.open(uri("file:///a.pactole"), "hello".to_string());
        assert_eq!(docs.get(&uri("file:///a.pactole")), Some("hello"));
    }

    #[test]
    fn change_replaces_the_full_text() {
        let mut docs = Documents::new();
        docs.open(uri("file:///a.pactole"), "hello".to_string());
        docs.change(uri("file:///a.pactole"), "world".to_string());
        assert_eq!(docs.get(&uri("file:///a.pactole")), Some("world"));
    }

    #[test]
    fn close_removes_the_document() {
        let mut docs = Documents::new();
        docs.open(uri("file:///a.pactole"), "hello".to_string());
        docs.close(&uri("file:///a.pactole"));
        assert_eq!(docs.get(&uri("file:///a.pactole")), None);
    }

    #[test]
    fn unopened_document_is_absent() {
        let docs = Documents::new();
        assert_eq!(docs.get(&uri("file:///missing.pactole")), None);
    }

    #[test]
    fn documents_source_loader_falls_back_to_the_filesystem_when_unopened() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-documents-loader-test-{}-a",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("saved.pactole");
        std::fs::write(&file, "2026-09-03 open Assets:Checking\n").unwrap();

        let docs = Documents::new();
        let loader = DocumentsSourceLoader::new(&docs);
        assert_eq!(
            loader.load(&file).unwrap(),
            "2026-09-03 open Assets:Checking\n"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An unsaved, in-editor modification to an included file must be used
    /// instead of the last-saved-to-disk content: this is the whole point of
    /// [`DocumentsSourceLoader`] taking precedence over
    /// [`pactole_storage_fs::FsSourceLoader`].
    #[test]
    fn documents_source_loader_prefers_an_open_unsaved_buffer_over_disk() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-documents-loader-test-{}-b",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("included.pactole");
        std::fs::write(&file, "2026-09-03 open Assets:Checking\n").unwrap();

        let mut docs = Documents::new();
        let file_uri = Url::from_file_path(&file).unwrap();
        // The editor has an unsaved change: a `close` directive that is not
        // yet written to disk.
        docs.open(
            file_uri,
            "2026-09-03 open Assets:Checking\n2026-09-04 close Assets:Checking\n".to_string(),
        );

        let loader = DocumentsSourceLoader::new(&docs);
        assert_eq!(
            loader.load(&file).unwrap(),
            "2026-09-03 open Assets:Checking\n2026-09-04 close Assets:Checking\n"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// End-to-end through `analyze_file_with_loader`: an included file's
    /// unsaved editor buffer, not its on-disk content, must drive the
    /// resulting analysis (e.g. an account usable in the unsaved version but
    /// not yet declared on disk).
    #[test]
    fn analyze_file_with_loader_uses_the_open_buffer_for_an_included_file() {
        use pactole_storage_fs::analyze_file_with_loader;

        let dir = std::env::temp_dir().join(format!(
            "pactole-documents-loader-test-{}-c",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.pactole");
        let included = dir.join("included.pactole");
        std::fs::write(&main, "include \"included.pactole\"\n").unwrap();
        // On disk, `included.pactole` is empty of directives.
        std::fs::write(&included, "").unwrap();

        let mut docs = Documents::new();
        let included_uri = Url::from_file_path(&included).unwrap();
        // The editor has an unsaved addition: an `open` directive.
        docs.open(
            included_uri,
            "2026-09-03 open Assets:Checking\n".to_string(),
        );

        let loader = DocumentsSourceLoader::new(&docs);
        let project = analyze_file_with_loader(&main, &loader).unwrap();

        let included_file = project
            .files()
            .iter()
            .find(|f| f.path == included)
            .expect("included file should be part of the analyzed project");
        assert_eq!(included_file.file.entries().len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Without a `journal_file`, the current document becomes its own entry
    /// point, so its own `include`s are followed too (the scope hover,
    /// completion and navigation all share).
    #[test]
    fn analyze_scope_falls_back_to_the_current_document_without_a_journal_file() {
        let dir = std::env::temp_dir().join(format!(
            "pactole-documents-scope-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.pactole");
        let included = dir.join("included.pactole");
        std::fs::write(&main, "include \"included.pactole\"\n").unwrap();
        std::fs::write(&included, "2026-09-03 open Assets:Checking\n").unwrap();

        let docs = Documents::new();
        let main_uri = Url::from_file_path(&main).unwrap();
        let project = analyze_scope(&Config::default(), &docs, &main_uri)
            .expect("the current document is analyzable as an entry point");
        assert_eq!(project.files().len(), 2);

        // A document URI with no filesystem path cannot be an entry point.
        let untitled = uri("untitled:Untitled-1");
        assert!(analyze_scope(&Config::default(), &docs, &untitled).is_none());

        // `uri_to_path` only accepts `file:` URIs.
        assert_eq!(uri_to_path(&main_uri), Some(main));
        assert_eq!(uri_to_path(&untitled), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
