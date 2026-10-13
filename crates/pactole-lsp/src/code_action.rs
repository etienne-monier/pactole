//! Code actions for common `.pactole` editing fixes.

use chrono::Local;
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Position, TextEdit, Url,
    WorkspaceEdit,
};

use crate::documents::Documents;

fn is_date_character(byte: u8) -> bool {
    byte.is_ascii_digit() || byte == b'-'
}

fn starts_with_date(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 10
        && bytes[..10].iter().enumerate().all(|(index, byte)| {
            (index == 4 || index == 7) == (*byte == b'-') && is_date_character(*byte)
        })
        && bytes
            .get(10)
            .is_none_or(|byte| *byte == b'=' || byte.is_ascii_whitespace())
}

fn needs_date(line: &str) -> bool {
    if line.starts_with(char::is_whitespace) {
        return false;
    }

    let trimmed = line.trim_end();
    if trimmed.is_empty() || starts_with_date(trimmed) {
        return false;
    }

    let first_word = trimmed.split_whitespace().next().unwrap_or_default();
    matches!(first_word, "open" | "close" | "commodity" | "balance")
        || matches!(first_word, "*" | "!" | "?")
}

fn line_at(source: &str, line: u32) -> Option<&str> {
    source.lines().nth(line as usize)
}

pub fn actions_for(
    documents: &Documents,
    uri: &Url,
    params: &CodeActionParams,
) -> Vec<CodeActionOrCommand> {
    let Some(source) = documents.get(uri) else {
        return Vec::new();
    };
    let line_number = params.range.start.line;
    let Some(line) = line_at(source, line_number) else {
        return Vec::new();
    };
    if !needs_date(line) {
        return Vec::new();
    }

    let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let edit = TextEdit {
        range: lsp_types::Range::new(Position::new(line_number, 0), Position::new(line_number, 0)),
        new_text: format!("{date} "),
    };
    let action = CodeAction {
        title: "Insert today's date".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    };

    vec![CodeActionOrCommand::CodeAction(action)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::Documents;

    fn params(line: u32) -> CodeActionParams {
        CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: Url::parse("file:///ledger.pactole").unwrap(),
            },
            range: lsp_types::Range::new(Position::new(line, 0), Position::new(line, 0)),
            context: lsp_types::CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    #[test]
    fn offers_date_fix_for_an_undated_directive() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "open Assets:Checking\n".to_string());

        let actions = actions_for(&documents, &uri, &params(0));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CodeActionOrCommand::CodeAction(_)));
    }

    #[test]
    fn does_not_offer_date_fix_for_excluded_or_dated_lines() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "payee \"Shop\"\ninclude \"other.pactole\"\n2026-10-13 open Assets:Checking\n"
                .to_string(),
        );

        assert!(actions_for(&documents, &uri, &params(0)).is_empty());
        assert!(actions_for(&documents, &uri, &params(1)).is_empty());
        assert!(actions_for(&documents, &uri, &params(2)).is_empty());
    }
}
