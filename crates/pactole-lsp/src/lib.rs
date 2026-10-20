//! A minimal Language Server Protocol (LSP) server for `.pactole` files.
//!
//! Scope of this first implementation: `initialize`/`shutdown`/`exit`
//! lifecycle, `textDocument/didOpen`/`didChange`/`didClose` document
//! synchronization (full-document sync), and `textDocument/publishDiagnostics`
//! for syntax, lowering, include and business diagnostics.
//!
//! When a `journal_file` is configured, `include` resolution prefers
//! currently-open, possibly-unsaved editor buffers over what is on disk (see
//! [`documents::DocumentsSourceLoader`]), and diagnostics published for a
//! previous analysis are cleared for any file no longer covered by the
//! latest one.
//!
//! Validation settings come from the same layered `pactole.toml` files as the
//! CLI (global configuration directory, then the project file next to
//! `journal_file` or at the workspace root), overridden by the
//! `initializationOptions` `validation` key (see [`config`]).
//!
//! Completion, code actions, hover, navigation (`textDocument/definition`,
//! `textDocument/references`) and rename (`textDocument/prepareRename`,
//! `textDocument/rename`) are supported for editor assistance; formatting
//! is intentionally left to the `pactole fmt` command. Incremental
//! synchronization is not supported yet.

pub mod code_action;
pub mod completion;
pub mod config;
pub mod conversion;
pub mod diagnostics;
pub mod documents;
pub mod hover;
pub mod navigation;
pub mod rename;
pub mod server;
