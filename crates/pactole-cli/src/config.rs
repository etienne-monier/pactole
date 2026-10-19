use std::path::Path;
use std::process::ExitCode;

use pactole_config::{Config, ConfigError, Overrides, ValidationLevel};

use crate::commands::ValidationArg;

/// Resolves the layered `pactole.toml` configuration for a command.
///
/// `project_dir` is the directory holding the journal being operated on:
/// its `pactole.toml` is the project layer, overridden by the CLI's
/// in-memory flags. When `explicit` is given, that file is the only layer.
pub(crate) fn load(
    explicit: Option<&Path>,
    validation: Option<ValidationArg>,
    project_dir: Option<&Path>,
) -> Result<Config, ConfigError> {
    let project = project_dir.map(|dir| dir.join("pactole.toml"));
    let overrides = Overrides {
        payee: validation.map(ValidationLevel::from),
    };

    Config::load(explicit, project.as_deref(), &overrides)
}

/// Reports a configuration error and returns a failing exit code.
pub(crate) fn failure(error: ConfigError) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
