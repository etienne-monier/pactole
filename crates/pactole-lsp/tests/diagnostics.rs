//! End-to-end tests for the diagnostics a `.pactole` project produces, with a
//! focus on UTF-16 positions.
//!
//! LSP positions count UTF-16 code units, not bytes, so a non-ASCII character
//! placed before a diagnostic on the same line must not shift its range. The
//! first test checks the analysis and conversion pipeline directly; the
//! second drives a real `pactole-lsp` process over stdio, with a project
//! served entirely from in-memory editor buffers, and checks the range the
//! server actually publishes.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use lsp_types::{Diagnostic, NumberOrString, Position, Range};
use pactole_lsp::conversion::text_at_range;
use pactole_lsp::diagnostics::project_diagnostics;
use pactole_storage_fs::{InMemorySourceLoader, analyze_file_with_loader};
use serde_json::json;

/// Path to the binary built by cargo for this test run.
const BIN: &str = env!("CARGO_BIN_EXE_pactole-lsp");

const MAIN_PATH: &str = "/journal/main.pactole";
const TXNS_PATH: &str = "/journal/txns.pactole";
const MAIN_URI: &str = "file:///journal/main.pactole";
const TXNS_URI: &str = "file:///journal/txns.pactole";

const MAIN: &str = "2026-01-01 open Actifs:Compte\n\
                    2026-01-01 open Depenses:Divers\n\
                    2026-01-01 commodity EUR\n\
                    include \"txns.pactole\"\n";

/// The payee is not declared: the whole `string` node is reported, and it
/// contains a two-byte `é`.
const TXNS: &str = "2026-01-02 * \"Café de la Gare\" \"Déjeuner\"\n  \
                    Actifs:Compte -12.50 EUR\n  \
                    Depenses:Divers 12.50 EUR\n";

/// The byte range of `"Café de la Gare"` within [`TXNS`].
const PAYEE_START: Position = Position {
    line: 0,
    character: 13,
};
const PAYEE_END: Position = Position {
    line: 0,
    character: 30,
};
const PAYEE_TEXT: &str = "\"Café de la Gare\"";

fn code(diagnostic: &Diagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        NumberOrString::String(code) => Some(code),
        NumberOrString::Number(_) => None,
    }
}

#[test]
fn a_non_ascii_payee_is_positioned_in_utf16_code_units() {
    let mut loader = InMemorySourceLoader::new();
    loader.insert(MAIN_PATH, MAIN);
    loader.insert(TXNS_PATH, TXNS);

    let project = analyze_file_with_loader(Path::new(MAIN_PATH), &loader)
        .expect("the in-memory project loads");
    let published = project_diagnostics(&project);

    let txns = published
        .iter()
        .find(|(path, _)| path.ends_with("txns.pactole"))
        .map(|(_, diagnostics)| diagnostics)
        .expect("the included file is part of the project");
    let payee = txns
        .iter()
        .find(|diagnostic| code(diagnostic) == Some("payee-not-declared"))
        .unwrap_or_else(|| panic!("the undeclared payee is not reported: {txns:#?}"));

    assert_eq!(payee.range, Range::new(PAYEE_START, PAYEE_END));
    assert_eq!(text_at_range(TXNS, &payee.range), Some(PAYEE_TEXT));
    // The payee spans 18 bytes but only 17 UTF-16 code units, so a
    // byte-offset-based implementation would have ended at column 31 and
    // dropped the closing quote from the reported range.
    assert_eq!(PAYEE_TEXT.len(), 18);
}

// -----------------------------------------------------------------
// A real server, driven over stdio
// -----------------------------------------------------------------

/// A running `pactole-lsp` process with an initialized session.
///
/// The [`BufReader`] is kept for the whole session: creating a new one per
/// message could buffer (and lose) the bytes of the messages that follow.
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Session {
    /// Starts the server and initializes it with the given `journal_file`.
    fn start(journal_file: &str) -> Self {
        let mut child = Command::new(BIN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn pactole-lsp");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        let mut session = Self {
            child,
            stdin,
            stdout,
        };

        session.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {},
                "initializationOptions": { "journal_file": journal_file },
            },
        }));
        let response = session.recv();
        assert_eq!(response["id"], json!(1), "unexpected response: {response}");
        assert!(
            response["result"]["capabilities"].is_object(),
            "initialize failed: {response}"
        );

        session.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        session
    }

    fn send(&mut self, message: serde_json::Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())
            .expect("write to pactole-lsp");
        self.stdin.flush().expect("flush pactole-lsp");
    }

    fn recv(&mut self) -> serde_json::Value {
        read_message(&mut self.stdout)
    }

    fn open(&mut self, uri: &str, text: &str) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "pactole",
                    "version": 1,
                    "text": text,
                },
            },
        }));
    }

    fn change(&mut self, uri: &str, version: i32, text: &str) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": version },
                // Full-document synchronization: one change carrying the
                // whole new text.
                "contentChanges": [{ "text": text }],
            },
        }));
    }

    /// Reads notifications until the diagnostics published for `uri` satisfy
    /// `matches`, then returns them.
    fn diagnostics_for(
        &mut self,
        uri: &str,
        matches: impl Fn(&[serde_json::Value]) -> bool,
    ) -> Vec<serde_json::Value> {
        for _ in 0..20 {
            let message = self.recv();
            if message["method"] != json!("textDocument/publishDiagnostics") {
                continue;
            }
            let params = &message["params"];
            if params["uri"] != json!(uri) {
                continue;
            }
            let diagnostics = params["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if matches(&diagnostics) {
                return diagnostics;
            }
        }
        panic!("the server never published the expected diagnostics for {uri}");
    }

    fn shutdown(mut self) {
        self.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }));
        // Notifications (a final `publishDiagnostics`, for instance) may be
        // interleaved with the response.
        let response = loop {
            let message = self.recv();
            if message["id"] == json!(2) {
                break message;
            }
        };
        assert!(response["error"].is_null(), "shutdown failed: {response}");
        self.send(json!({ "jsonrpc": "2.0", "method": "exit" }));

        drop(self.stdin);
        let status = self.child.wait().expect("wait for pactole-lsp");
        assert!(status.success(), "pactole-lsp exited with {status}");
    }
}

/// Reads one LSP message (header + JSON body) from `reader`.
fn read_message(reader: &mut impl BufRead) -> serde_json::Value {
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).expect("read header");
        assert!(read > 0, "the server closed stdout before answering");
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().expect("a numeric Content-Length");
        }
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).expect("read body");
    serde_json::from_slice(&body).expect("the body is valid JSON")
}

#[test]
fn the_server_publishes_utf16_ranges_for_non_ascii_payees() {
    let mut session = Session::start(MAIN_PATH);

    // The project is served entirely from the two open buffers: no file has
    // to exist on disk.
    session.open(MAIN_URI, MAIN);
    session.open(TXNS_URI, TXNS);

    let diagnostics = session.diagnostics_for(TXNS_URI, |diagnostics| !diagnostics.is_empty());

    assert_eq!(
        diagnostics.len(),
        1,
        "unexpected diagnostics: {diagnostics:#?}"
    );
    assert_eq!(diagnostics[0]["code"], json!("payee-not-declared"));

    let range: Range =
        serde_json::from_value(diagnostics[0]["range"].clone()).expect("a valid range");
    assert_eq!(range, Range::new(PAYEE_START, PAYEE_END));
    assert_eq!(text_at_range(TXNS, &range), Some(PAYEE_TEXT));

    session.shutdown();
}

#[test]
fn the_server_clears_the_diagnostics_of_a_file_dropped_from_the_project() {
    let mut session = Session::start(MAIN_PATH);

    session.open(MAIN_URI, MAIN);
    session.open(TXNS_URI, TXNS);
    session.diagnostics_for(TXNS_URI, |diagnostics| !diagnostics.is_empty());

    // Removing the `include` drops the transaction file out of the project:
    // the diagnostics published for it previously must be cleared, not left
    // dangling in the editor.
    session.change(
        MAIN_URI,
        2,
        "2026-01-01 open Actifs:Compte\n2026-01-01 open Depenses:Divers\n",
    );

    let diagnostics = session.diagnostics_for(TXNS_URI, |diagnostics| diagnostics.is_empty());
    assert!(diagnostics.is_empty());

    session.shutdown();
}
