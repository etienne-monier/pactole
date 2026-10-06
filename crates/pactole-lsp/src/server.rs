//! The LSP server's stdio transport, main loop, and request/notification
//! handling.

use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

use lsp_server::{Connection, Message};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
    PublishDiagnostics,
};
use lsp_types::{
    Diagnostic, InitializeParams, PublishDiagnosticsParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};

use pactole_storage_fs::analyze_file_with_loader;

use crate::config::{Config, workspace_root_from_initialize_params};
use crate::diagnostics::{diagnostics_for_file, project_diagnostics};
use crate::documents::{Documents, DocumentsSourceLoader};

/// Runs the server over stdio until the client sends `exit`.
pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    })?;
    let initialize_params = connection.initialize(server_capabilities)?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;

    let workspace_root = workspace_root_from_initialize_params(&initialize_params);
    let config = Config::from_initialization_options(
        initialize_params.initialization_options.as_ref(),
        workspace_root.as_deref(),
    );
    log::info!(
        "initialized: workspace_root={:?} journal_file={:?}",
        workspace_root,
        config.journal_file
    );

    main_loop(connection, config)?;
    io_threads.join()?;
    log::info!("shutting down");
    Ok(())
}

fn main_loop(connection: Connection, config: Config) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut documents = Documents::new();
    // URIs published with non-empty diagnostics by the last `publish_for`
    // call, so a subsequent analysis that no longer covers one of them
    // (e.g. an `include` was removed) can clear its now-stale diagnostics
    // instead of leaving them stuck forever.
    let mut published: HashSet<Url> = HashSet::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
                // No requests are handled yet (completion/hover/formatting
                // are out of scope for this first version); reply with a
                // clear "method not found" rather than silently dropping it.
                let response = lsp_server::Response::new_err(
                    req.id,
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("method not implemented: {}", req.method),
                );
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(note) => {
                handle_notification(&connection, &config, &mut documents, &mut published, note)?;
            }
            Message::Response(_) => {
                // The server never sends requests to the client yet, so no
                // response is ever expected here.
            }
        }
    }

    Ok(())
}

fn handle_notification(
    connection: &Connection,
    config: &Config,
    documents: &mut Documents,
    published: &mut HashSet<Url>,
    note: lsp_server::Notification,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    match note.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: lsp_types::DidOpenTextDocumentParams = serde_json::from_value(note.params)?;
            let uri = params.text_document.uri;
            // Document contents are never logged: only identifying
            // information (URI) and sizes, to avoid leaking journal data.
            log::debug!(
                "didOpen: uri={} len={}",
                uri,
                params.text_document.text.len()
            );
            documents.open(uri.clone(), params.text_document.text);
            publish_for(connection, config, documents, published, &uri)?;
        }
        DidChangeTextDocument::METHOD => {
            let params: lsp_types::DidChangeTextDocumentParams =
                serde_json::from_value(note.params)?;
            let uri = params.text_document.uri;
            // Full-document sync: the last (and only) content change carries
            // the whole new text (see `documents.rs`'s module docs).
            if let Some(change) = params.content_changes.into_iter().next_back() {
                log::debug!("didChange: uri={} len={}", uri, change.text.len());
                documents.change(uri.clone(), change.text);
                publish_for(connection, config, documents, published, &uri)?;
            }
        }
        DidCloseTextDocument::METHOD => {
            let params: lsp_types::DidCloseTextDocumentParams =
                serde_json::from_value(note.params)?;
            log::debug!("didClose: uri={}", params.text_document.uri);
            documents.close(&params.text_document.uri);
            // Clear diagnostics for the now-closed document.
            publish_diagnostics(connection, &params.text_document.uri, Vec::new())?;
            published.remove(&params.text_document.uri);
        }
        _ => {}
    }
    Ok(())
}

/// Recomputes and publishes diagnostics for `uri` (and, when a
/// `journal_file` is configured and reachable, every file in its include
/// graph).
///
/// Any URI that was published with diagnostics by a previous call but is no
/// longer covered by this one (e.g. an `include` directive was removed, so
/// the included file drops out of the project) is published an empty
/// diagnostics list, so stale diagnostics are not left dangling on a file
/// this call no longer analyzes. This also covers the edge case where the
/// configured `journal_file` becomes unreadable and `uri` itself is no
/// longer an open document (e.g. it was closed in between): nothing is
/// published this time, so every previously-published URI is cleared
/// instead of being silently left with stale diagnostics forever.
fn publish_for(
    connection: &Connection,
    config: &Config,
    documents: &Documents,
    published: &mut HashSet<Url>,
    uri: &Url,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    if let Some(journal_file) = &config.journal_file {
        // Open, possibly-unsaved buffers must take precedence over what is
        // on disk: an included file being edited (but not yet saved) should
        // have its in-editor content analyzed, not its last-saved-to-disk
        // content.
        let loader = DocumentsSourceLoader::new(documents);
        match analyze_file_with_loader(journal_file, &loader) {
            Ok(project) => {
                let mut newly_published = HashSet::new();
                let per_file = project_diagnostics(&project);
                let file_count = per_file.len();
                let diagnostic_count: usize = per_file.values().map(Vec::len).sum();
                for (path, diags) in per_file {
                    if let Some(file_uri) = path_to_url(&path) {
                        newly_published.insert(file_uri.clone());
                        publish_diagnostics(connection, &file_uri, diags)?;
                    }
                }
                log::debug!(
                    "project analysis succeeded: journal_file={:?} files={} diagnostics={}",
                    journal_file,
                    file_count,
                    diagnostic_count
                );
                clear_stale(connection, published, &newly_published)?;
                *published = newly_published;
                return Ok(());
            }
            Err(_) => {
                // Fall through to standalone analysis of the current
                // document: an unreadable configured journal file should
                // not prevent diagnostics on the document the user is
                // actually editing.
                log::warn!(
                    "project analysis failed, falling back to standalone analysis of {}: journal_file={:?}",
                    uri,
                    journal_file
                );
            }
        }
    }

    if let Some(source) = documents.get(uri) {
        let file = pactole_storage_fs::analyze_file(source);
        let diags = diagnostics_for_file(&file);
        log::debug!(
            "standalone analysis: uri={} diagnostics={}",
            uri,
            diags.len()
        );
        let mut newly_published = HashSet::new();
        newly_published.insert(uri.clone());
        publish_diagnostics(connection, uri, diags)?;
        clear_stale(connection, published, &newly_published)?;
        *published = newly_published;
    } else {
        // Neither the configured `journal_file` nor `uri` itself could be
        // analyzed (e.g. the journal file became unreadable and `uri` is a
        // document that has since been closed). Nothing is published this
        // time, so every URI previously published must be cleared instead
        // of being left stuck with stale diagnostics forever.
        log::warn!("no analysis available for {uri}, clearing all published diagnostics");
        clear_stale(connection, published, &HashSet::new())?;
        published.clear();
    }

    Ok(())
}

/// Computes the set of URIs in `previously` that are absent from `now`,
/// i.e. the diagnostics that must be cleared because the latest analysis no
/// longer covers them. Pure and I/O-free so it can be unit-tested directly.
fn stale_uris(previously: &HashSet<Url>, now: &HashSet<Url>) -> HashSet<Url> {
    previously.difference(now).cloned().collect()
}

/// Publishes an empty diagnostics list for every URI in `previously` that is
/// absent from `now`, clearing diagnostics for files no longer covered by
/// the latest analysis.
fn clear_stale(
    connection: &Connection,
    previously: &HashSet<Url>,
    now: &HashSet<Url>,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    for stale_uri in stale_uris(previously, now) {
        publish_diagnostics(connection, &stale_uri, Vec::new())?;
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: &Url,
    diagnostics: Vec<Diagnostic>,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };
    let notification = lsp_server::Notification::new(
        PublishDiagnostics::METHOD.to_string(),
        serde_json::to_value(params)?,
    );
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn path_to_url(path: &PathBuf) -> Option<Url> {
    Url::from_file_path(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(path: &str) -> Url {
        Url::from_file_path(path).expect("valid file path")
    }

    #[test]
    fn stale_uris_is_empty_when_now_covers_everything_previously_published() {
        let previously: HashSet<Url> = [url("/a.pactole"), url("/b.pactole")].into();
        let now = previously.clone();

        assert!(stale_uris(&previously, &now).is_empty());
    }

    #[test]
    fn stale_uris_returns_uris_dropped_from_the_latest_analysis() {
        let previously: HashSet<Url> = [url("/a.pactole"), url("/b.pactole")].into();
        let now: HashSet<Url> = [url("/a.pactole")].into();

        let stale = stale_uris(&previously, &now);

        assert_eq!(stale, [url("/b.pactole")].into());
    }

    #[test]
    fn stale_uris_clears_every_previously_published_uri_when_nothing_is_published_now() {
        // Reproduces the edge case where the configured `journal_file`
        // became unreadable and the current document is no longer open:
        // `now` is empty, so every previously-published URI must be
        // reported as stale instead of being left dangling.
        let previously: HashSet<Url> = [url("/a.pactole"), url("/b.pactole")].into();
        let now: HashSet<Url> = HashSet::new();

        let stale = stale_uris(&previously, &now);

        assert_eq!(stale, previously);
    }
}
