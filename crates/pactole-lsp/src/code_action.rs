//! Code actions for common `.pactole` editing fixes.
//!
//! Two families of quick fixes are offered:
//!
//! - an "insert today's date" fix on an undated directive header, derived
//!   purely from the document text;
//! - declaration fixes derived from the business diagnostics the server
//!   already publishes: a missing `payee`, a not-yet-open `account`, or an
//!   undeclared `commodity` is declared by inserting a new directive line
//!   above the offending transaction, in the current document or in a
//!   configured declaration file.
//!
//! Declaration paths come from the `[declaration]` section of `pactole.toml`
//! (see [`crate::config::Config::declarations`]) and are only honoured when
//! the target file is part of the analyzed project (i.e. reachable from the
//! root journal through `include`); otherwise the fix falls back to the
//! current document. This also means declaration fixes only ever appear when
//! a `journal_file` is configured, since business diagnostics require a
//! project context.

use chrono::{Local, NaiveDate};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic as LspDiagnostic,
    DocumentChanges, OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit,
    TextEdit, Url, WorkspaceEdit,
};
use pactole_storage_fs::AnalyzedProject;

use crate::config::Config;
use crate::conversion::text_at_range;
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

/// The kind of declaration a business diagnostic is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclarationKind {
    Payee,
    Account,
    Commodity,
}

impl DeclarationKind {
    /// Maps a business diagnostic code to the declaration it requires.
    fn from_code(code: Option<&str>) -> Option<Self> {
        match code? {
            "payee-not-declared" => Some(Self::Payee),
            "account-not-open" => Some(Self::Account),
            "commodity-not-declared" => Some(Self::Commodity),
            _ => None,
        }
    }

    /// The declared value as it appears verbatim in the source, without the
    /// surrounding quotes for a payee.
    ///
    /// The diagnostic span points at the offending token itself (`payee` node
    /// for a transaction, `account`/`commodity_name` for a posting), so the
    /// source slice only needs unquoting, never re-parsing.
    fn declared_value(self, raw: &str) -> Option<String> {
        let raw = raw.trim();
        match self {
            Self::Payee => {
                let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
                Some(inner.to_string())
            }
            Self::Account | Self::Commodity => (!raw.is_empty()).then(|| raw.to_string()),
        }
    }

    /// The directive line declaring `value`.
    ///
    /// `date` is the offending transaction's date: an `open` or `commodity`
    /// declaration is dated, a `payee` is not (it has no temporal life cycle).
    fn directive_line(self, value: &str, date: NaiveDate) -> String {
        match self {
            Self::Payee => format!("payee \"{}\"", quote_string(value)),
            Self::Account => format!("{date} open {value}"),
            Self::Commodity => format!("{date} commodity {value}"),
        }
    }

    /// Title of the quick fix, phrased for the user.
    fn title(self, value: &str) -> String {
        match self {
            Self::Payee => format!("Declare payee \"{value}\""),
            Self::Account => format!("Open account {value}"),
            Self::Commodity => format!("Declare commodity {value}"),
        }
    }
}

/// Mirrors `pactole-storage-fs`'s formatter: the grammar has no string
/// escapes, so quotes and line breaks inside a value become spaces instead.
fn quote_string(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c == '"' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Where a declaration line should be inserted.
enum Target {
    /// The document the code action was requested for.
    Current,
    /// A configured declaration file, resolved to its path.
    External(std::path::PathBuf),
}

/// Resolves the target file of a declaration, honouring the `[declaration]`
/// configuration only when the configured file actually belongs to the
/// analyzed project.
fn target_for(kind: DeclarationKind, config: &Config, project: Option<&AnalyzedProject>) -> Target {
    let configured = match kind {
        DeclarationKind::Payee => config.declarations.payees.as_ref(),
        DeclarationKind::Account => config.declarations.accounts.as_ref(),
        DeclarationKind::Commodity => config.declarations.commodities.as_ref(),
    };

    let Some(path) = configured else {
        return Target::Current;
    };

    let is_included =
        project.is_some_and(|project| project.files().iter().any(|file| file.path == *path));

    if is_included {
        Target::External(path.clone())
    } else {
        Target::Current
    }
}

/// Returns the line at which the directive containing `line` starts, by
/// walking up to the first line that is not indented and is not blank.
fn directive_start_line(source: &str, line: u32) -> u32 {
    let lines: Vec<&str> = source.lines().collect();
    let is_continuation =
        |line: Option<&&str>| line.is_some_and(|l| l.starts_with(char::is_whitespace));

    let mut index = (line as usize).min(lines.len().saturating_sub(1));
    while index > 0 && is_continuation(lines.get(index)) {
        index -= 1;
    }

    index as u32
}

/// Extracts the date of the directive covering `line`, falling back to
/// today's date when the line carries none.
fn directive_date(source: &str, line: u32) -> NaiveDate {
    let lines: Vec<&str> = source.lines().collect();
    let start = directive_start_line(source, line) as usize;

    lines
        .get(start)
        .and_then(|l| l.get(..10))
        .and_then(|prefix| NaiveDate::parse_from_str(prefix, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive())
}

/// Builds the text edit inserting `directive` at the end of `target_source`.
fn append_edit(target_source: &str, directive: &str) -> TextEdit {
    let needs_separator = !target_source.is_empty() && !target_source.ends_with("\n\n");
    let line_count = target_source.lines().count() as u32;
    // `lines()` drops a trailing newline, so an empty final line still needs
    // to be addressed at the position just after the last real line.
    let end_line = if target_source.is_empty() {
        0
    } else {
        line_count
    };

    TextEdit {
        range: Range::new(Position::new(end_line, 0), Position::new(end_line, 0)),
        new_text: if needs_separator {
            format!("\n{directive}\n")
        } else {
            format!("{directive}\n")
        },
    }
}

/// Builds the text edit inserting `directive` above the directive starting at
/// `start_line` of the current document.
fn insert_above_edit(start_line: u32, directive: &str) -> TextEdit {
    TextEdit {
        range: Range::new(Position::new(start_line, 0), Position::new(start_line, 0)),
        new_text: format!("{directive}\n"),
    }
}

/// Builds a `WorkspaceEdit` targeting either the current document (`changes`)
/// or another file (`document_changes`, with no version requirement so the
/// client may edit a file that is not open).
fn workspace_edit(current: &Url, target: &Target, edit: TextEdit) -> Option<WorkspaceEdit> {
    match target {
        Target::Current => Some(WorkspaceEdit {
            changes: Some([(current.clone(), vec![edit])].into_iter().collect()),
            document_changes: None,
            change_annotations: None,
        }),
        Target::External(path) => {
            let uri = Url::from_file_path(path).ok()?;
            Some(WorkspaceEdit {
                changes: None,
                document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                    edits: vec![lsp_types::OneOf::Left(edit)],
                }])),
                change_annotations: None,
            })
        }
    }
}

fn declaration_actions(
    documents: &Documents,
    uri: &Url,
    source: &str,
    params: &CodeActionParams,
    config: &Config,
    project: Option<&AnalyzedProject>,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let mut seen: Vec<(DeclarationKind, String)> = Vec::new();

    for diagnostic in &params.context.diagnostics {
        if diagnostic.source.as_deref() != Some("pactole-business") {
            continue;
        }
        let Some(kind) =
            DeclarationKind::from_code(diagnostic.code.as_ref().and_then(|code| match code {
                lsp_types::NumberOrString::String(code) => Some(code.as_str()),
                lsp_types::NumberOrString::Number(_) => None,
            }))
        else {
            continue;
        };
        let Some(raw) = text_at_range(source, &diagnostic.range) else {
            continue;
        };
        let Some(value) = kind.declared_value(raw) else {
            continue;
        };
        if seen.contains(&(kind, value.clone())) {
            continue;
        }
        seen.push((kind, value.clone()));

        let target = target_for(kind, config, project);
        let directive =
            kind.directive_line(&value, directive_date(source, diagnostic.range.start.line));

        let edit = match &target {
            Target::Current => insert_above_edit(
                directive_start_line(source, diagnostic.range.start.line),
                &directive,
            ),
            Target::External(path) => {
                let target_source = documents
                    .get_by_path(path)
                    .map(str::to_string)
                    .or_else(|| std::fs::read_to_string(path).ok())
                    .unwrap_or_default();
                append_edit(&target_source, &directive)
            }
        };

        let Some(edit) = workspace_edit(uri, &target, edit) else {
            continue;
        };

        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: kind.title(&value),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone() as LspDiagnostic]),
            edit: Some(edit),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        }));
    }

    actions
}

/// Returns code actions applicable to the requested document range.
pub fn actions_for(
    documents: &Documents,
    uri: &Url,
    params: &CodeActionParams,
    config: &Config,
    project: Option<&AnalyzedProject>,
) -> Vec<CodeActionOrCommand> {
    let Some(source) = documents.get(uri) else {
        return Vec::new();
    };

    let mut actions = declaration_actions(documents, uri, source, params, config, project);

    let line_number = params.range.start.line;
    if let Some(line) = line_at(source, line_number)
        && needs_date(line)
    {
        let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let edit = TextEdit {
            range: Range::new(Position::new(line_number, 0), Position::new(line_number, 0)),
            new_text: format!("{date} "),
        };
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
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
        }));
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::Documents;
    use pactole_storage_fs::{InMemorySourceLoader, analyze_file_with_loader};
    use std::path::{Path, PathBuf};

    fn params(line: u32) -> CodeActionParams {
        params_with(line, Vec::new())
    }

    fn params_with(line: u32, diagnostics: Vec<LspDiagnostic>) -> CodeActionParams {
        CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: Url::parse("file:///ledger.pactole").unwrap(),
            },
            range: Range::new(Position::new(line, 0), Position::new(line, 0)),
            context: lsp_types::CodeActionContext {
                diagnostics,
                only: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        }
    }

    fn business_diagnostic(code: &str, line: u32, start: u32, end: u32) -> LspDiagnostic {
        LspDiagnostic {
            range: Range::new(Position::new(line, start), Position::new(line, end)),
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            code: Some(lsp_types::NumberOrString::String(code.to_string())),
            source: Some("pactole-business".to_string()),
            message: String::new(),
            ..Default::default()
        }
    }

    fn project(path: &Path, sources: &[(&Path, &str)]) -> AnalyzedProject {
        let mut loader = InMemorySourceLoader::new();
        for (path, source) in sources {
            loader.insert(path.to_path_buf(), (*source).to_string());
        }
        analyze_file_with_loader(path, &loader).expect("entry path is loadable")
    }

    #[test]
    fn offers_date_fix_for_an_undated_directive() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let mut documents = Documents::new();
        documents.open(uri.clone(), "open Assets:Checking\n".to_string());

        let actions = actions_for(&documents, &uri, &params(0), &Config::default(), None);
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

        assert!(actions_for(&documents, &uri, &params(0), &Config::default(), None).is_empty());
        assert!(actions_for(&documents, &uri, &params(1), &Config::default(), None).is_empty());
        assert!(actions_for(&documents, &uri, &params(2), &Config::default(), None).is_empty());
    }

    #[test]
    fn declares_a_missing_payee_above_the_transaction() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        // Span of `"Shop"` on line 0: columns 13..19.
        let diagnostic = business_diagnostic("payee-not-declared", 0, 13, 19);
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(0, vec![diagnostic.clone()]),
            &Config::default(),
            None,
        );

        assert_eq!(actions.len(), 1);
        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a code action");
        };
        assert_eq!(action.title, "Declare payee \"Shop\"");
        assert_eq!(action.diagnostics.as_ref().map(Vec::len), Some(1));

        let edit = action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0].clone();
        assert_eq!(edit.range.start, Position::new(0, 0));
        assert_eq!(edit.new_text, "payee \"Shop\"\n");
    }

    #[test]
    fn declares_a_missing_account_with_the_transaction_date() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        // Span of `Expenses:Food` on line 2: columns 2..15.
        let diagnostic = business_diagnostic("account-not-open", 2, 2, 15);
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(2, vec![diagnostic]),
            &Config::default(),
            None,
        );

        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a code action");
        };
        assert_eq!(action.title, "Open account Expenses:Food");

        let edit = action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0].clone();
        assert_eq!(edit.range.start, Position::new(0, 0));
        assert_eq!(edit.new_text, "2026-01-02 open Expenses:Food\n");
    }
    #[test]
    fn inserts_the_declaration_at_the_end_of_a_configured_declaration_file() {
        let journal = Path::new("/journal/main.pactole");
        let declarations = Path::new("/journal/payees.pactole");
        let project = project(
            journal,
            &[
                (
                    journal,
                    "include \"payees.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n",
                ),
                (declarations, "payee \"Bakery\"\n"),
            ],
        );

        let uri = Url::from_file_path(journal).unwrap();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "include \"payees.pactole\"\n2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n".to_string(),
        );

        let config = Config::with_declarations(crate::config::DeclarationTargets {
            payees: Some(PathBuf::from(declarations)),
            ..Default::default()
        });

        // Span of `"Shop"` on line 1.
        let diagnostic = business_diagnostic("payee-not-declared", 1, 13, 19);
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(1, vec![diagnostic]),
            &config,
            Some(&project),
        );

        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a code action");
        };
        let changes = action
            .edit
            .as_ref()
            .unwrap()
            .document_changes
            .as_ref()
            .expect("expected document changes");
        let DocumentChanges::Edits(edits) = changes else {
            panic!("expected edits");
        };
        assert_eq!(
            edits[0].text_document.uri,
            Url::from_file_path(declarations).unwrap()
        );
        let lsp_types::OneOf::Left(edit) = &edits[0].edits[0] else {
            panic!("expected a text edit");
        };
        assert!(edit.new_text.contains("payee \"Shop\""));
    }

    #[test]
    fn falls_back_to_the_current_file_when_the_declaration_file_is_not_included() {
        let journal = Path::new("/journal/main.pactole");
        let project = project(
            journal,
            &[(
                journal,
                "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n",
            )],
        );

        let uri = Url::from_file_path(journal).unwrap();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n  Expenses:Food 10 EUR\n"
                .to_string(),
        );

        let config = Config::with_declarations(crate::config::DeclarationTargets {
            payees: Some(PathBuf::from("/journal/payees.pactole")),
            ..Default::default()
        });

        let diagnostic = business_diagnostic("payee-not-declared", 0, 13, 19);
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(0, vec![diagnostic]),
            &config,
            Some(&project),
        );

        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a code action");
        };
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0];
        assert_eq!(edit.new_text, "payee \"Shop\"\n");
    }

    #[test]
    fn declares_a_missing_commodity() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-03-04 balance Assets:Checking 10 EUR\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        // Span of `EUR` on line 0: columns 38..41.
        let diagnostic = business_diagnostic("commodity-not-declared", 0, 38, 41);
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(0, vec![diagnostic]),
            &Config::default(),
            None,
        );

        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("expected a code action");
        };
        assert_eq!(action.title, "Declare commodity EUR");
        let edit = &action.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0];
        assert_eq!(edit.new_text, "2026-03-04 commodity EUR\n");
    }

    #[test]
    fn deduplicates_identical_declarations() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let source = "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n2026-01-03 * \"Shop\"\n  Expenses:Food 10 EUR\n";
        let mut documents = Documents::new();
        documents.open(uri.clone(), source.to_string());

        let actions = actions_for(
            &documents,
            &uri,
            &params_with(
                0,
                vec![
                    business_diagnostic("payee-not-declared", 0, 13, 19),
                    business_diagnostic("payee-not-declared", 2, 13, 19),
                ],
            ),
            &Config::default(),
            None,
        );

        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn ignores_diagnostics_from_other_sources() {
        let uri = Url::parse("file:///ledger.pactole").unwrap();
        let mut documents = Documents::new();
        documents.open(
            uri.clone(),
            "2026-01-02 * \"Shop\"\n  Assets:Checking -10 EUR\n".to_string(),
        );

        let mut diagnostic = business_diagnostic("payee-not-declared", 0, 13, 19);
        diagnostic.source = Some("pactole-lowering".to_string());
        let actions = actions_for(
            &documents,
            &uri,
            &params_with(0, vec![diagnostic]),
            &Config::default(),
            None,
        );

        assert!(actions.is_empty());
    }
}
