//! Completion for `pactole.toml` files.

#![allow(clippy::missing_docs_in_private_items)]

use std::collections::BTreeSet;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionTextEdit,
    Position, Range, TextEdit,
};

use crate::completion::{SymbolIndex, build_index_from_project};
use crate::config::Config;
use crate::conversion::position_to_offset;
use crate::documents::{Documents, analyze_scope};

/// Completes a configuration document at the requested position.
pub(crate) fn handle_completion(
    config: &Config,
    documents: &Documents,
    params: CompletionParams,
) -> Result<CompletionList, Box<dyn std::error::Error + Sync + Send>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?;
    let offset = position_to_offset(source, &position);
    let context = context_at(source, offset, &position);

    let index = documents.with_analyzed_scope(config, &uri, |cached| {
        cached
            .map(build_index_from_project)
            .or_else(|| {
                analyze_scope(config, documents, &uri)
                    .as_ref()
                    .map(build_index_from_project)
            })
            .unwrap_or_default()
    });

    Ok(CompletionList {
        is_incomplete: false,
        items: completion_items(&context, &index),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextKind {
    Section,
    Key,
    Value,
}

#[derive(Debug, Clone)]
struct Context {
    kind: ContextKind,
    section: String,
    key: Option<String>,
    prefix: String,
    range: Range,
    quoted: bool,
    closed_quote: bool,
}

fn context_at(source: &str, offset: usize, position: &Position) -> Context {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    let line = &source[line_start..offset];
    let line_after = &source[offset..line_end];
    let section = source[..line_start]
        .lines()
        .rev()
        .find_map(section_from_line)
        .unwrap_or_default();
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();

    if trimmed.is_empty() && section.is_empty() {
        return Context {
            kind: ContextKind::Section,
            section,
            key: None,
            prefix: String::new(),
            range: Range::new(Position::new(position.line, leading as u32), *position),
            quoted: false,
            closed_quote: false,
        };
    }

    if trimmed.starts_with('[') && !trimmed.contains(']') {
        let prefix = trimmed[1..].to_string();
        return Context {
            kind: ContextKind::Section,
            section,
            key: None,
            prefix,
            range: Range::new(Position::new(position.line, leading as u32), *position),
            quoted: false,
            closed_quote: false,
        };
    }

    let key_end = line.find('=').unwrap_or(line.len());
    if key_end == line.len() {
        let key_text = line.trim();
        return Context {
            kind: ContextKind::Key,
            section,
            key: None,
            prefix: key_text.to_string(),
            range: Range::new(
                Position::new(
                    position.line,
                    position.character.saturating_sub(utf16_len(key_text)),
                ),
                *position,
            ),
            quoted: false,
            closed_quote: false,
        };
    }

    let key = line[..key_end].trim().to_string();
    let value_start = key_end + 1;
    let value = &line[value_start..];
    let comma_offset = value.rfind(',').map_or(0, |index| index + 1);
    let segment = &value[comma_offset..];
    let segment_trimmed = segment.trim_start();
    let segment_whitespace = segment.len() - segment_trimmed.len();
    let bracket = usize::from(segment_trimmed.starts_with('['));
    let after_bracket = &segment_trimmed[bracket..];
    let after_bracket = after_bracket.trim_start();
    let bracket_whitespace = segment_trimmed[bracket..].len() - after_bracket.len();
    let value_trimmed = after_bracket;
    let quoted = value_trimmed.starts_with('"');
    let quote_offset = usize::from(quoted);
    let prefix_start = line_start
        + value_start
        + comma_offset
        + segment_whitespace
        + bracket
        + bracket_whitespace
        + quote_offset;
    let typed_value = &source[prefix_start..offset];
    let typed_prefix = typed_value.trim_end_matches('"');
    let closed_quote = quoted && (typed_value.contains('"') || line_after.contains('"'));
    Context {
        kind: ContextKind::Value,
        section,
        key: Some(key),
        prefix: typed_prefix.to_string(),
        range: Range::new(
            Position::new(
                position.line,
                position.character.saturating_sub(utf16_len(typed_prefix)),
            ),
            *position,
        ),
        quoted,
        closed_quote,
    }
}

fn section_from_line(line: &str) -> Option<String> {
    let line = line.split('#').next()?.trim();
    if let Some(inner) = line
        .strip_prefix("[[")
        .and_then(|line| line.strip_suffix("]]"))
    {
        return Some(inner.to_string());
    }
    Some(line.strip_prefix('[')?.strip_suffix(']')?.to_string())
}

fn utf16_len(text: &str) -> u32 {
    u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX)
}

fn completion_items(context: &Context, index: &SymbolIndex) -> Vec<CompletionItem> {
    match context.kind {
        ContextKind::Section => section_items(&context.prefix, context.range),
        ContextKind::Key => {
            let keys = section_keys(&context.section);
            items(
                &keys,
                &context.prefix,
                context.range,
                CompletionItemKind::PROPERTY,
                false,
                false,
            )
        }
        ContextKind::Value => {
            let key = context.key.as_deref().unwrap_or_default();
            let values = value_candidates(&context.section, key, index);
            items(
                &values,
                &context.prefix,
                context.range,
                CompletionItemKind::VALUE,
                !context.quoted,
                context.closed_quote,
            )
        }
    }
}

fn section_keys(section: &str) -> Vec<&'static str> {
    match section {
        "validation" => vec!["payee", "unused", "config"],
        "format" => vec!["posting_width"],
        "declaration" => vec!["payees", "accounts", "commodities"],
        "reports" => vec![
            "assets",
            "liabilities",
            "equity",
            "income",
            "expenses",
            "uncategorized",
        ],
        "import" => vec![
            "bank_account",
            "accounts",
            "currency",
            "default_counter_account",
            "payee_metadata_key",
            "no_imported_payee",
            "omit_negative_amount",
            "date_field",
            "effective_date_field",
            "rules",
        ],
        "import.rules" => vec![
            "payee_contains",
            "payee_in",
            "payee_regex",
            "memo_contains",
            "memo_in",
            "memo_regex",
            "trntype",
            "trntype_in",
            "account",
            "direction",
            "counter_account",
            "payee",
        ],
        _ => Vec::new(),
    }
}

fn section_items(prefix: &str, range: Range) -> Vec<CompletionItem> {
    ["validation", "format", "declaration", "reports", "import"]
        .into_iter()
        .filter(|candidate| candidate.contains(prefix))
        .map(|candidate| CompletionItem {
            label: candidate.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: format!("[{candidate}]"),
            })),
            ..Default::default()
        })
        .collect()
}

fn value_candidates(section: &str, key: &str, index: &SymbolIndex) -> Vec<String> {
    match (section, key) {
        ("validation", _) => ["off", "warn", "error"]
            .into_iter()
            .map(String::from)
            .collect(),
        ("import.rules", "direction") => {
            ["debit", "credit"].into_iter().map(String::from).collect()
        }
        ("import", "currency") => index.commodities.iter().cloned().collect(),
        ("import", "bank_account")
        | ("import", "default_counter_account")
        | ("import.rules", "counter_account")
        | ("import.accounts", _)
        | ("reports", "uncategorized") => index.accounts_all.iter().cloned().collect(),
        ("import.rules", "payee") => index.payees.iter().cloned().collect(),
        ("reports", _) => index
            .accounts_open
            .iter()
            .filter_map(|account| account.split(':').next().map(String::from))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn items(
    candidates: &[impl AsRef<str>],
    prefix: &str,
    range: Range,
    kind: CompletionItemKind,
    quote: bool,
    closed_quote: bool,
) -> Vec<CompletionItem> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let candidate = candidate.as_ref();
            if !candidate.to_lowercase().contains(&prefix.to_lowercase()) {
                return None;
            }
            let new_text = if quote {
                format!("\"{candidate}\"")
            } else if !closed_quote {
                format!("{candidate}\"")
            } else {
                candidate.to_string()
            };
            Some(CompletionItem {
                label: candidate.to_string(),
                kind: Some(kind),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor_position(source: &str) -> Position {
        let line = source.lines().count().saturating_sub(1);
        Position::new(
            line as u32,
            utf16_len(source.lines().last().unwrap_or_default()),
        )
    }

    #[test]
    fn completes_root_sections_with_toml_headers() {
        let source = "[imp";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let items = completion_items(&context, &SymbolIndex::default());

        let import = items.iter().find(|item| item.label == "import").unwrap();
        assert_eq!(
            import.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit {
                range: Range::new(Position::new(0, 0), position),
                new_text: "[import]".to_string(),
            }))
        );
    }

    #[test]
    fn completes_keys_in_a_validation_section() {
        let source = "[validation]\ncon";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let items = completion_items(&context, &SymbolIndex::default());

        assert!(items.iter().any(|item| item.label == "config"));

        let source = "[validation]\n";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let items = completion_items(&context, &SymbolIndex::default());
        assert!(items.iter().any(|item| item.label == "payee"));
    }

    #[test]
    fn completes_validation_levels_inside_quotes() {
        let source = "[validation]\nconfig = \"wa";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let items = completion_items(&context, &SymbolIndex::default());

        let warning = items.iter().find(|item| item.label == "warn").unwrap();
        assert!(matches!(
            &warning.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit { new_text, .. }))
                if new_text == "warn\""
        ));
    }

    #[test]
    fn preserves_a_closing_quote_after_the_cursor() {
        let source = "[validation]\nconfig = \"wa\"\n";
        let offset = source.find("wa").unwrap() + 2;
        let position = Position::new(1, 15);
        let context = context_at(source, offset, &position);
        let items = completion_items(&context, &SymbolIndex::default());
        let warning = items.iter().find(|item| item.label == "warn").unwrap();

        assert!(matches!(
            &warning.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit { new_text, .. }))
                if new_text == "warn"
        ));
    }

    #[test]
    fn completes_sections_on_an_empty_root_line() {
        let source = "";
        let position = cursor_position(source);
        let context = context_at(source, 0, &position);
        let items = completion_items(&context, &SymbolIndex::default());

        assert!(items.iter().any(|item| item.label == "import"));
    }

    #[test]
    fn completes_the_current_element_after_a_array_comma() {
        let source = "[reports]\nassets = [\"Assets\", \"In";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let index = SymbolIndex {
            accounts_open: ["Income:Salary".to_string()].into(),
            ..Default::default()
        };
        let items = completion_items(&context, &index);

        let income = items.iter().find(|item| item.label == "Income").unwrap();
        assert!(matches!(
            &income.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit { range, .. }))
                if range.start.character == position.character - 2
        ));
    }

    #[test]
    fn completes_accounts_in_import_values() {
        let source = "[import]\nbank_account = \"Assets:";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let index = SymbolIndex {
            accounts_all: ["Assets:Checking".to_string()].into(),
            ..Default::default()
        };
        let items = completion_items(&context, &index);

        assert!(items.iter().any(|item| item.label == "Assets:Checking"));
    }

    #[test]
    fn does_not_complete_external_rule_account_ids_with_pactole_accounts() {
        let source = "[[import.rules]]\naccount = \"";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let index = SymbolIndex {
            accounts_all: ["Assets:Checking".to_string()].into(),
            ..Default::default()
        };

        assert!(completion_items(&context, &index).is_empty());
    }

    #[test]
    fn recognizes_array_of_table_sections() {
        let source = "[[import.rules]]\nco";
        let position = cursor_position(source);
        let context = context_at(source, source.len(), &position);
        let items = completion_items(&context, &SymbolIndex::default());

        assert!(items.iter().any(|item| item.label == "counter_account"));
    }
}
