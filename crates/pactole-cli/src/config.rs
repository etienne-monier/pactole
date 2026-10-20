use std::path::Path;
use std::process::ExitCode;

use pactole_config::{Config, ConfigError, Overrides};

/// Resolves the layered `pactole.toml` configuration for a command.
///
/// `project_dir` is the directory holding the journal being operated on:
/// its `pactole.toml` is the project layer. When `explicit` is given, that
/// file is the only layer.
pub(crate) fn load(
    explicit: Option<&Path>,
    project_dir: Option<&Path>,
) -> Result<Config, ConfigError> {
    let project = project_dir.map(|dir| dir.join("pactole.toml"));
    let overrides = Overrides::default();

    Config::load(explicit, project.as_deref(), &overrides)
}

/// Reports a configuration error and returns a failing exit code.
pub(crate) fn failure(error: ConfigError) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
