/// Declaration files (the `[declaration]` section of `pactole.toml`).
///
/// These optional paths tell an editor front-end (the language server) which
/// file should receive a newly declared payee, account or commodity, instead
/// of the file currently being edited.
///
/// Paths are kept raw and unresolved on purpose: `pactole-config` is a leaf
/// crate that knows nothing about the journal location, so resolving a
/// relative path against the journal's directory is left to the consumer
/// (see `pactole-lsp`'s `config`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeclarationOptions {
    /// File receiving new `payee "..."` declarations.
    pub payees: Option<String>,
    /// File receiving new `DATE open ACCOUNT` declarations.
    pub accounts: Option<String>,
    /// File receiving new `DATE commodity COMMODITY` declarations.
    pub commodities: Option<String>,
}

impl DeclarationOptions {
    /// Returns whether no declaration file is configured at all.
    pub fn is_empty(&self) -> bool {
        self.payees.is_none() && self.accounts.is_none() && self.commodities.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::DeclarationOptions;

    #[test]
    fn default_is_empty() {
        let options = DeclarationOptions::default();

        assert!(options.is_empty());
        assert_eq!(options.payees, None);
        assert_eq!(options.accounts, None);
        assert_eq!(options.commodities, None);
    }

    #[test]
    fn any_configured_path_is_not_empty() {
        let options = DeclarationOptions {
            payees: Some("payees.pactole".to_string()),
            ..DeclarationOptions::default()
        };

        assert!(!options.is_empty());
    }
}
