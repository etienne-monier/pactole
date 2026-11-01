#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Local HTTP adapter for Pactole's transport-independent application layer.
//!
//! The server is deliberately generic over [`pactole_app::LedgerReader`]. It
//! exposes reports and diagnostics, but never parses source files or performs
//! accounting calculations itself.

/// HTTP server, routing and transport conversion.
mod server;

/// Frontend files embedded at build time.
mod static_assets;

pub use server::{ServerError, ServerOptions, router, serve};
