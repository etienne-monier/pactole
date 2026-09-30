//! `pactole-lsp` binary entry point: runs a minimal Language Server Protocol
//! server for `.pactole` files over stdio.

fn main() -> std::process::ExitCode {
    match pactole_lsp::server::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pactole-lsp: fatal error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
