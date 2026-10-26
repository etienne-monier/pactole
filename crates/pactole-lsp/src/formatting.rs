//! Full-document formatting for LSP clients.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lsp_types::{DocumentFormattingParams, Position, Range, TextEdit, Url};
use pactole_storage_fs::{PactoleFsStorageError, format_with_options};

use crate::config::Config;
use crate::conversion::end_position;
use crate::documents::Documents;

/// Errors returned while preparing a formatting response.
#[derive(Debug)]
pub enum FormattingError {
    /// The client requested formatting for a document that is not open.
    DocumentNotOpen(Url),
    /// The source cannot be formatted, normally because it has syntax errors.
    Formatter(PactoleFsStorageError),
}

impl Display for FormattingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentNotOpen(uri) => write!(formatter, "document is not open: {uri}"),
            Self::Formatter(error) => error.fmt(formatter),
        }
    }
}

impl Error for FormattingError {}

/// Formats the currently open document and returns a full-document edit.
pub fn format_document(
    config: &Config,
    documents: &Documents,
    params: DocumentFormattingParams,
) -> Result<Vec<TextEdit>, FormattingError> {
    let uri = params.text_document.uri;
    let source = documents
        .get(&uri)
        .ok_or_else(|| FormattingError::DocumentNotOpen(uri.clone()))?;
    let formatted =
        format_with_options(source, &config.format).map_err(FormattingError::Formatter)?;

    if formatted == source {
        return Ok(Vec::new());
    }

    Ok(vec![TextEdit {
        range: Range::new(Position::new(0, 0), end_position(source)),
        new_text: formatted,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{FormattingOptions, TextDocumentIdentifier};

    fn params(uri: Url) -> DocumentFormattingParams {
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: Default::default(),
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn formats_the_open_document_with_the_configured_width() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-01-01 open Assets:Checking\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());
        let mut config = Config::default();
        config.format.posting_width = 60;

        let edits = format_document(&config, &documents, params(uri)).unwrap();

        assert!(edits.is_empty());
    }

    #[test]
    fn returns_a_full_document_edit_for_non_canonical_source() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-01-01   open   Assets:Checking\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let edits = format_document(&Config::default(), &documents, params(uri)).unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start, Position::new(0, 0));
        assert_eq!(edits[0].new_text, "2026-01-01 open Assets:Checking\n");
    }

    #[test]
    fn refuses_to_format_invalid_source() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "not valid pactole".to_string());

        assert!(matches!(
            format_document(&Config::default(), &documents, params(uri)),
            Err(FormattingError::Formatter(_))
        ));
    }

    #[test]
    fn reports_a_closed_document_as_not_open() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();

        assert!(matches!(
            format_document(&Config::default(), &Documents::new(), params(uri)),
            Err(FormattingError::DocumentNotOpen(_))
        ));
    }
}
