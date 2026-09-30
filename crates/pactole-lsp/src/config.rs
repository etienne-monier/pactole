//! Minimal server configuration, read once from the LSP `initialize`
//! request's `initializationOptions`.
//!
//! Only a single, optional setting is supported for now: `journal_file`, the
//! path (relative to the workspace root, or absolute) to the root `.pactole`
//! file that should be analyzed with full `include` resolution via
//! [`pactole_storage_fs::analyze_file_with_loader`]. When absent, the server
//! falls back to analyzing each open document on its own via
//! [`pactole_storage_fs::analyze_file`], without following its `include`s.
//!
//! A relative `journal_file` is resolved against the workspace root when one
//! is known (from `workspace_folders`/`rootUri`/`rootPath`, see
//! [`workspace_root_from_initialize_params`]). When no workspace root is
//! known at all (e.g. a client that opens single files without a workspace),
//! it is instead resolved against the server process's current working
//! directory, rather than silently left relative: a relative path left
//! as-is would fail to resolve against whatever directory `include`
//! resolution happens to run from, silently dropping every diagnostic for
//! the whole include graph without any indication why.
//!
//! This does not yet read any configuration from `~/.config/pactole`; that
//! is explicitly out of scope for this first version.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Raw shape of the `initializationOptions` JSON value this server
/// understands. Unknown fields are ignored so future additions do not break
/// deserialization of older clients' payloads.
#[derive(Debug, Default, Deserialize)]
struct RawInitializationOptions {
    /// Path to the root `.pactole` journal file, relative to the workspace
    /// root if not absolute.
    journal_file: Option<String>,
}

/// Resolved server configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// The root journal file to analyze with include resolution, already
    /// resolved to an absolute-ish path (joined against the workspace root
    /// when it was given as relative, or against the current working
    /// directory when no workspace root is known at all). `None` means
    /// "analyze each open document standalone".
    pub journal_file: Option<PathBuf>,
}

impl Config {
    /// Builds a [`Config`] from the raw `initializationOptions` JSON value
    /// (as received in [`lsp_types::InitializeParams::initialization_options`])
    /// and the workspace root directory (derived from `rootUri`/`rootPath`,
    /// see [`workspace_root_from_initialize_params`]).
    ///
    /// A missing or unparsable `initializationOptions` value is treated the
    /// same as an empty configuration, rather than failing `initialize`.
    pub fn from_initialization_options(
        initialization_options: Option<&serde_json::Value>,
        workspace_root: Option<&Path>,
    ) -> Self {
        let raw: RawInitializationOptions = initialization_options
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();

        let journal_file = raw.journal_file.map(|raw_path| {
            let path = PathBuf::from(&raw_path);
            if path.is_absolute() {
                return path;
            }
            match workspace_root {
                Some(root) => root.join(&path),
                // No workspace root at all: resolve against the current
                // working directory rather than leaving the path relative
                // (which would silently fail to resolve later, dropping
                // every diagnostic for the whole include graph with no
                // indication why). If the current working directory itself
                // cannot be determined, the path is kept relative as a last
                // resort; `analyze_file_with_loader` will then simply fail
                // to load it, which the server already falls back from
                // gracefully (see `server::publish_for`).
                None => std::env::current_dir()
                    .map(|cwd| cwd.join(&path))
                    .unwrap_or(path),
            }
        });

        Config { journal_file }
    }
}

/// Derives the workspace root directory from an [`lsp_types::InitializeParams`],
/// preferring `workspace_folders`, then the deprecated `root_uri`, then the
/// even-older `root_path`.
pub fn workspace_root_from_initialize_params(
    params: &lsp_types::InitializeParams,
) -> Option<PathBuf> {
    if let Some(folders) = &params.workspace_folders
        && let Some(first) = folders.first()
        && let Ok(path) = first.uri.to_file_path()
    {
        return Some(path);
    }

    #[allow(deprecated)]
    if let Some(root_uri) = &params.root_uri
        && let Ok(path) = root_uri.to_file_path()
    {
        return Some(path);
    }

    #[allow(deprecated)]
    if let Some(root_path) = &params.root_path {
        return Some(PathBuf::from(root_path));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_initialization_options_yields_no_journal_file() {
        let config = Config::from_initialization_options(None, Some(Path::new("/workspace")));
        assert_eq!(config.journal_file, None);
    }

    #[test]
    fn relative_journal_file_is_resolved_against_the_workspace_root() {
        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/workspace/main.pactole"))
        );
    }

    #[test]
    fn absolute_journal_file_is_kept_as_is() {
        let options = serde_json::json!({ "journal_file": "/etc/journal.pactole" });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/etc/journal.pactole"))
        );
    }

    #[test]
    fn relative_journal_file_without_a_workspace_root_is_resolved_against_the_cwd() {
        let options = serde_json::json!({ "journal_file": "main.pactole" });
        let config = Config::from_initialization_options(Some(&options), None);
        assert_eq!(
            config.journal_file,
            Some(std::env::current_dir().unwrap().join("main.pactole"))
        );
    }

    #[test]
    fn unknown_fields_in_initialization_options_are_ignored() {
        let options = serde_json::json!({ "journal_file": "main.pactole", "unknown": 42 });
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(
            config.journal_file,
            Some(PathBuf::from("/workspace/main.pactole"))
        );
    }

    #[test]
    fn malformed_initialization_options_falls_back_to_defaults() {
        let options = serde_json::json!("not an object");
        let config =
            Config::from_initialization_options(Some(&options), Some(Path::new("/workspace")));
        assert_eq!(config.journal_file, None);
    }
}
