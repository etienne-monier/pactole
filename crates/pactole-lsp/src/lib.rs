//! A minimal Language Server Protocol (LSP) server for `.pactole` files.
//!
//! Scope of this first implementation: `initialize`/`shutdown`/`exit`
//! lifecycle, `textDocument/didOpen`/`didChange`/`didClose` document
//! synchronization (full-document sync), and `textDocument/publishDiagnostics`
//! for syntax diagnostics, lowering diagnostics, and include issues.
//!
//! When a `journal_file` is configured, `include` resolution prefers
//! currently-open, possibly-unsaved editor buffers over what is on disk (see
//! [`documents::DocumentsSourceLoader`]), and diagnostics published for a
//! previous analysis are cleared for any file no longer covered by the
//! latest one (see `server::publish_for`).
//!
//! Completion, hover, and formatting are intentionally not implemented yet.

pub mod config;
pub mod conversion;
pub mod diagnostics;
pub mod documents;
pub mod server;
