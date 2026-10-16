use pactole_core::ModelError;
use thiserror::Error;

/// Errors raised while importing an external format into Pactole domain
/// models.
#[derive(Error, Debug)]
pub enum ImportError {
    /// The source document could not be parsed by the underlying format
    /// parser.
    #[error("failed to parse OFX document: {0}")]
    Ofx(String),

    /// The document parsed, but contained no bank, credit card or
    /// investment statement that could be imported.
    #[error("no bank, credit card or investment statement found in the OFX document")]
    NoStatement,

    /// The document visibly contains `<STMTTRN>` transactions, but the
    /// parser produced none from any statement: an OFX dialect (or an
    /// account aggregate) was mishandled and the transactions were dropped.
    #[error(
        "the OFX document contains transactions but none could be read; this usually means an \
         unsupported OFX dialect — please report the file"
    )]
    TransactionsDropped,

    /// A transaction could not be converted to a domain model (e.g. an
    /// unrepresentable date).
    #[error("cannot import OFX transaction: {0}")]
    InvalidTransaction(String),

    /// A configured account name is not a valid Pactole account.
    #[error("invalid account name `{account}`: {source}")]
    InvalidAccount {
        /// Configured or imported account name that failed validation.
        account: String,
        #[source]
        /// Core validation error.
        source: ModelError,
    },

    /// A commodity name (from the OFX currency or the configuration) is not a
    /// valid Pactole commodity.
    #[error("invalid commodity name `{commodity}`: {source}")]
    InvalidCommodity {
        /// Configured or imported commodity name that failed validation.
        commodity: String,
        #[source]
        /// Core validation error.
        source: ModelError,
    },

    /// No currency could be determined for a transaction: the source record
    /// carries none and none is configured as a default.
    #[error(
        "missing currency for a transaction: the OFX statement has no currency and no default \
         `currency` is configured"
    )]
    MissingCurrency,

    /// A core model value (currently only the `fitid` metadata key) could not
    /// be built.
    #[error("invalid model value: {0}")]
    Model(#[from] ModelError),
}
