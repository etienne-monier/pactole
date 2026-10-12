use std::path::PathBuf;

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
