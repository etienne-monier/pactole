//! `pactole-lsp` binary entry point: runs a minimal Language Server Protocol
//! server for `.pactole` files over stdio.

fn main() -> std::process::ExitCode {
    // Logs must never go to stdout: it is the LSP protocol's own transport
    // (stdio), so any stray write there would corrupt the message stream.
    // `env_logger::Target::Stderr` makes this explicit rather than relying
    // on the (already stderr) default. Verbosity is controlled by `RUST_LOG`
    // (e.g. `RUST_LOG=pactole_lsp=debug`); with no `RUST_LOG` set, logging
    // stays silent.
    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Stderr)
        .init();

    match pactole_lsp::server::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pactole-lsp: fatal error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
