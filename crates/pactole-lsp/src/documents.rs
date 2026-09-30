//! In-memory state of open text documents.
//!
//! This server uses full-document synchronization
//! ([`lsp_types::TextDocumentSyncKind::FULL`]): each `didChange` notification
//! carries the whole new document text rather than incremental edits, so no
//! LSP-position-to-byte-offset translation is needed to apply changes. This
//! keeps document state trivial (just the latest full text per URI) at the
//! cost of larger `didChange` payloads, which is an acceptable trade-off for
//! `.pactole` files (typically small, hand-edited ledgers).

use std::collections::HashMap;
use std::path::Path;

use lsp_types::Url;
use pactole_storage_fs::{FsSourceLoader, SourceLoadError, SourceLoader};

/// The in-memory state of every currently-open document, keyed by URI.
#[derive(Debug, Default)]
pub struct Documents {
    open: HashMap<Url, String>,
}

impl Documents {
    /// Creates an empty document store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a newly opened document with its initial full text.
    pub fn open(&mut self, uri: Url, text: String) {
        self.open.insert(uri, text);
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
    }

    /// Forgets a document, e.g. on `didClose`.
    pub fn close(&mut self, uri: &Url) {
        self.open.remove(uri);
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
    fn get_by_path(&self, path: &Path) -> Option<&str> {
        let uri = Url::from_file_path(path).ok()?;
        self.get(&uri)
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
            return Ok(text.to_string());
        }
        self.fs.load(path)
    }
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
}
