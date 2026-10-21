//! Integration tests for the `pactole-lsp` binary: the command line
//! interface, and one real `initialize` round-trip over the stdio transport.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

/// Path to the binary built by cargo for this test run.
const BIN: &str = env!("CARGO_BIN_EXE_pactole-lsp");

#[test]
fn version_flag_prints_the_server_name_and_version() {
    let output = Command::new(BIN)
        .arg("--version")
        .output()
        .expect("run pactole-lsp");

    assert!(output.status.success(), "pactole-lsp --version failed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("pactole-lsp"),
        "unexpected --version output: {stdout}"
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "unexpected --version output: {stdout}"
    );
}

#[test]
fn stdio_flag_is_accepted() {
    // Editors may be configured to launch the server with `--stdio`; the flag
    // is accepted (and ignored, stdio being the only transport) so that such
    // a configuration keeps working.
    let output = Command::new(BIN)
        .arg("--stdio")
        .stdin(Stdio::null())
        .output()
        .expect("run pactole-lsp");

    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        !stderr.contains("unexpected argument"),
        "pactole-lsp --stdio was rejected: {stderr}"
    );
}

#[test]
fn initialize_result_carries_the_server_name_and_version() {
    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn pactole-lsp");

    let stdin = child.stdin.as_mut().expect("piped stdin");
    let stdout = child.stdout.as_mut().expect("piped stdout");

    write_message(
        stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": {} },
        }),
    );

    let response = read_message(stdout);
    let result = &response["result"];

    assert_eq!(
        result["serverInfo"]["name"],
        serde_json::json!("pactole-lsp")
    );
    assert_eq!(
        result["serverInfo"]["version"],
        serde_json::json!(env!("CARGO_PKG_VERSION"))
    );
    assert!(
        result["capabilities"]["hoverProvider"].is_boolean(),
        "the server capabilities are missing from the initialize result: {result}"
    );

    // The server waits for the `initialized` notification before serving
    // anything else, then for `shutdown` and `exit`.
    write_message(
        stdin,
        &serde_json::json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );
    write_message(
        stdin,
        &serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }),
    );
    let shutdown_response = read_message(stdout);
    assert_eq!(shutdown_response["id"], serde_json::json!(2));
    write_message(
        stdin,
        &serde_json::json!({ "jsonrpc": "2.0", "method": "exit" }),
    );

    drop(child.stdin.take());
    let status = child.wait().expect("wait for pactole-lsp");
    assert!(status.success(), "pactole-lsp exited with {status}");
}

/// Writes one LSP message (header + JSON body) to `writer`.
fn write_message(writer: &mut impl Write, message: &serde_json::Value) {
    let body = message.to_string();
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write to pactole-lsp");
    writer.flush().expect("flush pactole-lsp");
}

/// Reads one LSP message (header + JSON body) from `reader`.
fn read_message(reader: &mut impl Read) -> serde_json::Value {
    let mut reader = BufReader::new(reader);

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
