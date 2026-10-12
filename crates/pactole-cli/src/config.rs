use std::path::PathBuf;

use serde::Deserialize;

const DEFAULT_POSTING_WIDTH: usize = 50;

/// Directory where Pactole user configuration files live.
pub(crate) fn pactole_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("pactole"));
    }

    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("pactole"))
}

/// Default path of the OFX import configuration file.
pub(crate) fn default_import_config_path() -> Option<PathBuf> {
    pactole_config_dir().map(|dir| dir.join("import-ofx.toml"))
}

/// Default path of the formatter configuration file.
pub(crate) fn default_format_config_path() -> Option<PathBuf> {
    pactole_config_dir().map(|dir| dir.join("format.toml"))
}

#[derive(Debug, Deserialize)]
struct FormatConfig {
    #[serde(default = "default_posting_width")]
    posting_width: usize,
}

fn default_posting_width() -> usize {
    DEFAULT_POSTING_WIDTH
}

/// Load formatter options, using defaults when no configuration file exists.
pub(crate) fn format_options() -> Result<pactole_storage_fs::FormatOptions, String> {
    let Some(path) = default_format_config_path() else {
        return Ok(pactole_storage_fs::FormatOptions::default());
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(pactole_storage_fs::FormatOptions::default());
        }
        Err(err) => {
            return Err(format!(
                "cannot read configuration {}: {err}",
                path.display()
            ));
        }
    };

    let config: FormatConfig = toml::from_str(&source)
        .map_err(|err| format!("cannot parse configuration {}: {err}", path.display()))?;

    if config.posting_width == 0 {
        return Err(format!(
            "invalid configuration {}: posting_width must be greater than zero",
            path.display()
        ));
    }

    Ok(pactole_storage_fs::FormatOptions {
        posting_width: config.posting_width,
    })
}

#[cfg(test)]
mod tests {
    use super::{FormatConfig, default_posting_width};

    #[test]
    fn formatter_config_defaults_to_fifty_columns() {
        let config: FormatConfig = toml::from_str("").unwrap();

        assert_eq!(config.posting_width, default_posting_width());
    }

    #[test]
    fn formatter_config_accepts_a_custom_width() {
        let config: FormatConfig = toml::from_str("posting_width = 72").unwrap();

        assert_eq!(config.posting_width, 72);
    }
}
