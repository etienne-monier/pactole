/// Formatter options (the `[format]` section of `pactole.toml`).
///
/// These used to live in `pactole-storage-fs`; they moved here so that a
/// single crate owns the whole `pactole.toml` schema. `pactole-storage-fs`
/// re-exports this type to keep its public API stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Column at which posting amounts are aligned.
    pub posting_width: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { posting_width: 50 }
    }
}
