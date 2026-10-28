//! The LSP server's stdio transport, main loop, and request/notification
//! handling.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, select, unbounded};
use lsp_server::{Connection, Message, Request as LspRequest};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
    PublishDiagnostics,
};
use lsp_types::request::Request;
use lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CompletionOptions, CompletionParams,
    Diagnostic, HoverProviderCapability, InitializeParams, OneOf, PublishDiagnosticsParams,
    RenameOptions, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};

use pactole_storage_fs::analyze_file_with_loader_with_options;

use crate::config::{Config, workspace_root_from_initialize_params};
use crate::diagnostics::{project_diagnostics, standalone_diagnostics};
use crate::documents::{Documents, DocumentsSourceLoader, SnapshotSourceLoader};

const ANALYSIS_DEBOUNCE: Duration = Duration::from_millis(150);

struct AnalysisJob {
    generation: u64,
    uri: Url,
    entry: PathBuf,
    validation: pactole_config::ValidationOptions,
    loader: SnapshotSourceLoader,
}

struct AnalysisResult {
    generation: u64,
    uri: Url,
    entry: PathBuf,
    validation: pactole_config::ValidationOptions,
    project: Option<pactole_storage_fs::AnalyzedProject>,
    elapsed_ms: f64,
}

/// Runs the server over stdio until the client sends `exit`.
pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    // `Connection::initialize` can only answer with the server capabilities;
    // `initialize_start`/`initialize_finish` are used instead so that the
    // result can also carry the server's name and version.
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let server_capabilities = serde_json::to_value(server_capabilities())?;
    connection.initialize_finish(initialize_id, initialize_result(server_capabilities))?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;

    let workspace_root = workspace_root_from_initialize_params(&initialize_params);
    let mut config = Config::from_initialization_options(
        initialize_params.initialization_options.as_ref(),
        workspace_root.as_deref(),
    );
    config.load_layered(workspace_root.as_deref());
    log::info!(
        "initialized: workspace_root={:?} journal_file={:?} payee_level={} unused_level={}",
        workspace_root,
        config.journal_file,
        config.validation.payee.as_str(),
        config.validation.unused.as_str()
    );

    main_loop(connection, config)?;
    io_threads.join()?;
    log::info!("shutting down");
    Ok(())
}

/// Builds the result of the `initialize` request: the server capabilities,
/// plus the server's name and version so that clients can display them.
fn initialize_result(capabilities: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "capabilities": capabilities,
        "serverInfo": {
            "name": "pactole-lsp",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![
                " ".to_string(),
                "\"".to_string(),
                ":".to_string(),
                "#".to_string(),
                "^".to_string(),
                "\t".to_string(),
                "0".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
                "6".to_string(),
                "7".to_string(),
                "8".to_string(),
                "9".to_string(),
                "-".to_string(),
            ]),
            all_commit_characters: None,
            work_done_progress_options: Default::default(),
        }),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

fn start_analysis_worker() -> (Sender<AnalysisJob>, Receiver<AnalysisResult>) {
    let (job_sender, job_receiver) = unbounded::<AnalysisJob>();
    let (result_sender, result_receiver) = unbounded::<AnalysisResult>();

    thread::spawn(move || {
        while let Ok(mut job) = job_receiver.recv() {
            // Debounce bursts of edits. Only the newest snapshot is useful.
            loop {
                match job_receiver.recv_timeout(ANALYSIS_DEBOUNCE) {
                    Ok(newer) => job = newer,
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }

            let started = Instant::now();
            let project =
                analyze_file_with_loader_with_options(&job.entry, &job.loader, &job.validation)
                    .ok();
            let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
            log::debug!(
                "scope analysis: background elapsed_ms={} generation={} success={}",
                elapsed_ms,
                job.generation,
                project.is_some()
            );

            if result_sender
                .send(AnalysisResult {
                    generation: job.generation,
                    uri: job.uri,
                    entry: job.entry,
                    validation: job.validation,
                    project,
                    elapsed_ms,
                })
                .is_err()
            {
                return;
            }
        }
    });

    (job_sender, result_receiver)
}

fn schedule_analysis(
    config: &Config,
    documents: &Documents,
    uri: &Url,
    jobs: &Sender<AnalysisJob>,
) -> bool {
    let Some(entry) = config
        .journal_file
        .clone()
        .or_else(|| uri.to_file_path().ok())
    else {
        return false;
    };
    let job = AnalysisJob {
        generation: documents.generation(),
        uri: uri.clone(),
        entry,
        validation: config.validation,
        loader: documents.snapshot_loader(),
    };
    if jobs.send(job).is_err() {
        log::warn!("project analysis worker is unavailable");
    }
    true
}

fn main_loop(connection: Connection, config: Config) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut documents = Documents::new();
    // URIs published with non-empty diagnostics by the last `publish_for`
    // call, so a subsequent analysis that no longer covers one of them
    // (e.g. an `include` was removed) can clear its now-stale diagnostics
    // instead of leaving them stuck forever.
    let mut published: HashSet<Url> = HashSet::new();
    let (analysis_jobs, analysis_results) = start_analysis_worker();

    loop {
        select! {
            recv(analysis_results) -> result => {
                let Ok(result) = result else { break };
                if result.generation != documents.generation() {
                    log::debug!(
                        "discarding stale analysis: generation={} current_generation={}",
                        result.generation,
                        documents.generation()
                    );
                    continue;
                }
                log::debug!(
                    "accepting analysis: generation={} elapsed_ms={}",
                    result.generation,
                    result.elapsed_ms
                );
                if let Some(project) = result.project {
                    documents.store_analyzed_scope(result.entry, result.validation, project);
                    publish_for(
                        &connection,
                        &config,
                        &documents,
                        &mut published,
                        &result.uri,
                    )?;
                } else {
                    // Keep the last successful project for completion, but do
                    // not publish its stale diagnostics after a failed
                    // analysis. Fall back to standalone diagnostics for the
                    // document that triggered the analysis and clear all
                    // project-wide publications.
                    let mut current = HashSet::new();
                    if let Some(source) = documents.get(&result.uri) {
                        let file = pactole_storage_fs::analyze_file(source);
                        let path = result.uri.to_file_path().unwrap_or_default();
                        publish_diagnostics(
                            &connection,
                            &result.uri,
                            standalone_diagnostics(&file, &path),
                        )?;
                        current.insert(result.uri.clone());
                    }
                    clear_stale(&connection, &published, &current)?;
                    published = current;
                }
            }
            recv(connection.receiver) -> message => {
                let Ok(msg) = message else { break };
                match msg {
            Message::Request(req) => {
                // Break if message is shutdown
                if connection.handle_shutdown(&req)? {
                    break;
                }
                handle_request(req, &connection, &config, &mut documents)?;
            }
            Message::Notification(note) => {
                handle_notification(
                    &connection,
                    &config,
                    &mut documents,
                    &mut published,
                    &analysis_jobs,
                    note,
                )?;
            }
            Message::Response(_) => {
                // The server never sends requests to the client yet, so no
                // response is ever expected here.
            }
                }
            }
        }
    }

    Ok(())
}

fn handle_request(
    request: LspRequest,
    connection: &Connection,
    config: &Config,
    documents: &mut Documents,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    // -----------------------------
    // Code actions
    // -----------------------------
    if request.method == lsp_types::request::CodeActionRequest::METHOD {
        let params: CodeActionParams = serde_json::from_value(request.params)?;
        // Declaration quick fixes need the project (to know which files are
        // actually reachable through `include`), so the same analysis the
        // diagnostics were computed from is reused here.
        let project = analyze_project(config, documents);
        let result = crate::code_action::actions_for(
            documents,
            &params.text_document.uri,
            &params,
            config,
            project.as_ref(),
        );
        let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
        connection.sender.send(Message::Response(resp))?;
        return Ok(());
    }
    // -----------------------------
    // Completion request
    // -----------------------------
    if request.method == lsp_types::request::Completion::METHOD {
        let params: CompletionParams = serde_json::from_value(request.params)?;
        match crate::completion::handle_completion(config, documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InternalError as i32,
                    err.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Hover request
    // -----------------------------
    if request.method == lsp_types::request::HoverRequest::METHOD {
        let params: lsp_types::HoverParams = serde_json::from_value(request.params)?;
        match crate::hover::handle_hover(config, documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InternalError as i32,
                    err.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Goto definition request
    // -----------------------------
    if request.method == lsp_types::request::GotoDefinition::METHOD {
        let params: lsp_types::GotoDefinitionParams = serde_json::from_value(request.params)?;
        match crate::navigation::handle_definition(config, documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InternalError as i32,
                    err.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Find references request
    // -----------------------------
    if request.method == lsp_types::request::References::METHOD {
        let params: lsp_types::ReferenceParams = serde_json::from_value(request.params)?;
        match crate::navigation::handle_references(config, documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InternalError as i32,
                    err.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Rename requests
    // -----------------------------
    if request.method == lsp_types::request::Rename::METHOD {
        let params: lsp_types::RenameParams = serde_json::from_value(request.params)?;
        match crate::rename::handle_rename(config, documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                // An invalid or conflicting new name is the user's input
                // problem, not a server failure: report it as `InvalidParams`
                // so editors surface the message as a rename error and
                // nothing is applied.
                let code = if err.downcast_ref::<crate::rename::RenameError>().is_some() {
                    lsp_server::ErrorCode::InvalidParams as i32
                } else {
                    lsp_server::ErrorCode::InternalError as i32
                };
                let response = lsp_server::Response::new_err(request.id, code, err.to_string());
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    if request.method == lsp_types::request::PrepareRenameRequest::METHOD {
        let params: lsp_types::TextDocumentPositionParams = serde_json::from_value(request.params)?;
        match crate::rename::handle_prepare_rename(documents, params) {
            Ok(result) => {
                let resp = lsp_server::Response::new_ok(request.id, serde_json::to_value(result)?);
                connection.sender.send(Message::Response(resp))?;
            }
            Err(err) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InternalError as i32,
                    err.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Document formatting request
    // -----------------------------
    if request.method == lsp_types::request::Formatting::METHOD {
        let params = serde_json::from_value(request.params)?;
        match crate::formatting::format_document(config, documents, params) {
            Ok(edits) => {
                let response =
                    lsp_server::Response::new_ok(request.id, serde_json::to_value(edits)?);
                connection.sender.send(Message::Response(response))?;
            }
            Err(crate::formatting::FormattingError::DocumentNotOpen(uri)) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::InvalidParams as i32,
                    format!("document is not open: {uri}"),
                );
                connection.sender.send(Message::Response(response))?;
            }
            Err(crate::formatting::FormattingError::Formatter(error)) => {
                let response = lsp_server::Response::new_err(
                    request.id,
                    lsp_server::ErrorCode::RequestFailed as i32,
                    error.to_string(),
                );
                connection.sender.send(Message::Response(response))?;
            }
        }
        return Ok(());
    }
    // -----------------------------
    // Other requests are out of scope; reply with a clear "method not found"
    // rather than silently dropping it.
    // -----------------------------
    let response = lsp_server::Response::new_err(
        request.id,
        lsp_server::ErrorCode::MethodNotFound as i32,
        format!("method not implemented: {}", request.method),
    );
    connection.sender.send(Message::Response(response))?;

    Ok(())
}

fn handle_notification(
    connection: &Connection,
    config: &Config,
    documents: &mut Documents,
    published: &mut HashSet<Url>,
    analysis_jobs: &Sender<AnalysisJob>,
    note: lsp_server::Notification,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    match note.method.as_str() {
        // -----------------------------
        // A new document opend
        // -----------------------------
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
            if !schedule_analysis(config, documents, &uri, analysis_jobs) {
                publish_for(connection, config, documents, published, &uri)?;
            }
        }
        // -----------------------------
        // A document changed
        // -----------------------------
        DidChangeTextDocument::METHOD => {
            let params: lsp_types::DidChangeTextDocumentParams =
                serde_json::from_value(note.params)?;
            let uri = params.text_document.uri;
            // Full-document sync: the last (and only) content change carries
            // the whole new text (see `documents.rs`'s module docs).
            if let Some(change) = params.content_changes.into_iter().next_back() {
                log::debug!("didChange: uri={} len={}", uri, change.text.len());
                documents.change(uri.clone(), change.text);
                if !schedule_analysis(config, documents, &uri, analysis_jobs) {
                    publish_for(connection, config, documents, published, &uri)?;
                }
            }
        }
        // -----------------------------
        // A document was closed
        // -----------------------------
        DidCloseTextDocument::METHOD => {
            let params: lsp_types::DidCloseTextDocumentParams =
                serde_json::from_value(note.params)?;
            log::debug!("didClose: uri={}", params.text_document.uri);
            documents.close(&params.text_document.uri);
            // Clear diagnostics for the now-closed document.
            publish_diagnostics(connection, &params.text_document.uri, Vec::new())?;
            published.remove(&params.text_document.uri);
            if config.journal_file.is_some() {
                schedule_analysis(config, documents, &params.text_document.uri, analysis_jobs);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Recomputes and publishes diagnostics for `uri` and, when a `journal_file`
/// is configured and reachable, every file in its include graph. A currently
/// open document outside that graph receives standalone diagnostics.
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
    let publications = collect_diagnostics(config, documents, uri);
    let newly_published: HashSet<Url> = publications.keys().cloned().collect();
    for (file_uri, diagnostics) in publications {
        publish_diagnostics(connection, &file_uri, diagnostics)?;
    }
    clear_stale(connection, published, &newly_published)?;
    *published = newly_published;
    Ok(())
}

/// Analyzes the configured project, following `include`s from the root
/// journal, or returns `None` when no `journal_file` is configured or it
/// cannot be loaded.
///
/// This is the single place project analysis happens: both diagnostics
/// publication and the declaration code actions reuse it, so they always
/// agree on which files belong to the project.
fn analyze_project(
    config: &Config,
    documents: &Documents,
) -> Option<pactole_storage_fs::AnalyzedProject> {
    let journal_file = config.journal_file.as_ref()?;
    let loader = DocumentsSourceLoader::new(documents);

    match analyze_file_with_loader_with_options(journal_file, &loader, &config.validation) {
        Ok(project) => Some(project),
        Err(_) => {
            log::warn!("project analysis failed for journal_file={journal_file:?}");
            None
        }
    }
}

/// Collects diagnostics without depending on the LSP transport.
///
/// A configured project is preferred. If the current document is not part of
/// that project, it receives standalone syntax/lowering diagnostics instead
/// of being silently ignored. Standalone analysis deliberately does not run
/// business validation because the file has no project context.
fn collect_diagnostics(
    config: &Config,
    documents: &Documents,
    uri: &Url,
) -> HashMap<Url, Vec<Diagnostic>> {
    let started = Instant::now();
    let project_publications = documents.with_analyzed_scope(config, uri, |project| {
        project.map(|project| {
            let render_started = Instant::now();
            let mut publications = HashMap::new();
            for (path, diagnostics) in project_diagnostics(project) {
                if let Some(file_uri) = path_to_url(&path) {
                    publications.insert(file_uri, diagnostics);
                }
            }
            log::debug!(
                "diagnostics render: elapsed_ms={} files={}",
                render_started.elapsed().as_secs_f64() * 1_000.0,
                publications.len()
            );
            publications
        })
    });

    if let Some(mut publications) = project_publications {
        if !publications.contains_key(uri)
            && let Some(source) = documents.get(uri)
        {
            let file = pactole_storage_fs::analyze_file(source);
            let path = uri.to_file_path().unwrap_or_default();
            publications.insert(uri.clone(), standalone_diagnostics(&file, &path));
        }
        log::debug!(
            "diagnostics total: elapsed_ms={} publications={}",
            started.elapsed().as_secs_f64() * 1_000.0,
            publications.len()
        );
        return publications;
    }

    let Some(source) = documents.get(uri) else {
        log::warn!("no analysis available for {uri}, clearing all published diagnostics");
        return HashMap::new();
    };
    let file = pactole_storage_fs::analyze_file(source);
    let path = uri.to_file_path().unwrap_or_default();
    let publications = HashMap::from([(uri.clone(), standalone_diagnostics(&file, &path))]);
    log::debug!(
        "diagnostics total: elapsed_ms={} publications={} mode=standalone",
        started.elapsed().as_secs_f64() * 1_000.0,
        publications.len()
    );
    publications
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
    fn advertises_hover_support() {
        let capabilities = serde_json::to_value(server_capabilities()).unwrap();

        assert_eq!(capabilities["hoverProvider"], serde_json::json!(true));
    }

    #[test]
    fn advertises_navigation_support() {
        let capabilities = serde_json::to_value(server_capabilities()).unwrap();

        assert_eq!(capabilities["definitionProvider"], serde_json::json!(true));
        assert_eq!(capabilities["referencesProvider"], serde_json::json!(true));
    }

    #[test]
    fn advertises_rename_support() {
        let capabilities = serde_json::to_value(server_capabilities()).unwrap();

        assert_eq!(
            capabilities["renameProvider"]["prepareProvider"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn advertises_document_formatting_support() {
        let capabilities = serde_json::to_value(server_capabilities()).unwrap();

        assert_eq!(
            capabilities["documentFormattingProvider"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn initialize_result_advertises_the_server_name_and_version() {
        let result = initialize_result(serde_json::json!({ "hoverProvider": true }));

        assert_eq!(
            result["serverInfo"]["name"],
            serde_json::json!("pactole-lsp")
        );
        assert_eq!(
            result["serverInfo"]["version"],
            serde_json::json!(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn initialize_result_keeps_the_server_capabilities() {
        let result = initialize_result(serde_json::json!({ "hoverProvider": true }));

        assert_eq!(
            result["capabilities"]["hoverProvider"],
            serde_json::json!(true)
        );
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
