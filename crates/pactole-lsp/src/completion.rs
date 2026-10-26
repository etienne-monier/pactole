//! Completion logic for `.pactole` files.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionTextEdit,
    InsertTextFormat, Position, Range, TextEdit,
};

use chrono::Local;
use pactole_core::Entry;
use pactole_storage_fs::{AnalyzedProject, ParsedFile};

use crate::config::Config;
use crate::conversion::position_to_offset;
use crate::documents::{Documents, analyze_scope, uri_to_path};
use std::error::Error;

#[derive(Clone, Debug, Default)]
struct SymbolIndex {
    payees: BTreeSet<String>,
    commodities: BTreeSet<String>,
    accounts_open: BTreeSet<String>,
    accounts_all: BTreeSet<String>,
    tags: BTreeSet<String>,
    links: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CompletionContext {
    DirectiveKeyword { date_range: Option<Range> },
    IncludePath { prefix: String },
    Payee { prefix: String },
    Account { prefix: String },
    PostingAccount { prefix: String },
    Commodity { prefix: String },
    PriceBase { prefix: String },
    PriceQuote { prefix: String },
    Tag { prefix: String },
    Link { prefix: String },
    Unknown,
}

fn build_index_from_file(file: &ParsedFile) -> SymbolIndex {
    let mut index = SymbolIndex::default();
    for entry in file.entries() {
        match &entry.entry {
            Entry::Payee(p) => {
                index.payees.insert(p.name.to_string());
            }
            Entry::Commodity(c) => {
                index.commodities.insert(c.name.to_string());
            }
            Entry::Price(_) => {}
            Entry::Open(o) => {
                let name = o.account.to_string();
                index.accounts_open.insert(name.clone());
                index.accounts_all.insert(name);
            }
            Entry::Close(c) => {
                index.accounts_all.insert(c.account.to_string());
            }
            Entry::Transaction(t) => {
                if let Some(payee) = &t.payee {
                    index.payees.insert(payee.to_string());
                }
                for tag in &t.tags {
                    index.tags.insert(tag.to_string());
                }
                for link in &t.links {
                    index.links.insert(link.to_string());
                }
                for posting in &t.postings {
                    let name = posting.account.to_string();
                    index.accounts_open.insert(name.clone());
                    index.accounts_all.insert(name);
                }
            }
            Entry::Balance(b) => {
                index.accounts_all.insert(b.account.to_string());
            }
        }
    }
    index
}

fn build_index_from_project(project: &AnalyzedProject) -> SymbolIndex {
    let mut merged = SymbolIndex::default();
    for file in project.files() {
        let idx = build_index_from_file(&file.file);
        merged.payees.extend(idx.payees);
        merged.commodities.extend(idx.commodities);
        merged.accounts_open.extend(idx.accounts_open);
        merged.accounts_all.extend(idx.accounts_all);
        merged.tags.extend(idx.tags);
        merged.links.extend(idx.links);
    }
    merged
}

fn is_indented(line_text: &str) -> bool {
    line_text.starts_with(' ') || line_text.starts_with('\t')
}

fn is_complete_date_token(token: &str) -> bool {
    let date = token.split_once('=').map_or(
        token,
        |(date, effective)| {
            if effective.len() == 10 { date } else { "" }
        },
    );
    date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn transaction_header_before_payee(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.len() == 2 && is_complete_date_token(words[0]) && matches!(words[1], "*" | "!" | "?")
}

fn directive_keywords_list() -> Vec<(&'static str, &'static str)> {
    vec![
        ("open", "Open an account"),
        ("close", "Close an account"),
        ("commodity", "Declare a commodity"),
        ("price", "Record a dated market price"),
        ("payee", "Declare a payee"),
        ("balance", "Balance assertion"),
        ("include", "Include another file"),
    ]
}

fn context_at(text: &str, offset: usize, position: &Position) -> CompletionContext {
    // `offset` is a byte offset. Find the line start using byte slices rather
    // than taking `offset` characters, which would overshoot when the source
    // contains multi-byte UTF-8 characters.
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_text = &text[line_start..offset];
    let rest = &text[offset..];

    // Check if inside quotes
    // Simple heuristic: count quotes on this line before offset? Or just look backwards
    // For include/payee we look for " before
    if let Some(quote_idx) = line_text.rfind('"') {
        // Look backwards from quote
        let before_quote = &line_text[..quote_idx];
        if before_quote.trim_end().ends_with("include") {
            let prefix = line_text[quote_idx + 1..].to_string();
            return CompletionContext::IncludePath { prefix };
        }
        if before_quote.trim_end().ends_with("payee")
            || (before_quote.matches('"').count() == 0
                && transaction_header_before_payee(before_quote.trim_end()))
        {
            let prefix = line_text[quote_idx + 1..].to_string();
            return CompletionContext::Payee { prefix };
        }
    }

    // Check tags/links at current position (rest starts with or we see #/^ before)
    if let Some(_hash_idx) = line_text.rfind('#') {
        let prefix: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        return CompletionContext::Tag { prefix };
    }
    if let Some(_caret_idx) = line_text.rfind('^') {
        let prefix: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        return CompletionContext::Link { prefix };
    }

    // A commodity follows the number either of a posting amount or of a
    // transaction-cost annotation (`@`/`@@`).
    let words: Vec<&str> = line_text.split_whitespace().collect();
    let trailing_whitespace = line_text.chars().last().is_some_and(char::is_whitespace);
    let is_price_header = !is_indented(line_text) && words.contains(&"price");
    if is_indented(line_text) && matches!(words.last(), Some(&"@" | &"@@")) {
        // The next token is a numeric cost, not another posting account. There
        // is no numeric completion provider, so avoid misleading account items.
        return CompletionContext::Unknown;
    }
    if !is_indented(line_text) && trailing_whitespace {
        let keyword_index = usize::from(
            words
                .first()
                .is_some_and(|word| is_complete_date_token(word)),
        );
        if words.get(keyword_index) == Some(&"balance") {
            let tail = &words[keyword_index + 1..];
            let commodity_follows = match tail {
                [_, number] => number_looks_valid(number),
                [_, number, "~", tolerance] => {
                    number_looks_valid(number) && number_looks_valid(tolerance)
                }
                _ => false,
            };
            if commodity_follows {
                return CompletionContext::Commodity {
                    prefix: String::new(),
                };
            }
        }
    }
    if !is_price_header && let Some(last) = words.last().copied() {
        if !trailing_whitespace && words.len() >= 2 && number_looks_valid(words[words.len() - 2]) {
            return CompletionContext::Commodity {
                prefix: last.to_string(),
            };
        }
        if !trailing_whitespace
            && number_looks_valid(last)
            && words.len() >= 2
            && (words.len() == 2 || matches!(words.get(words.len() - 2), Some(&"@" | &"@@")))
        {
            return CompletionContext::Commodity {
                prefix: String::new(),
            };
        }
        if trailing_whitespace
            && number_looks_valid(last)
            && words.len() >= 2
            && (words.len() == 2 || matches!(words.get(words.len() - 2), Some(&"@" | &"@@")))
        {
            return CompletionContext::Commodity {
                prefix: String::new(),
            };
        }
    }

    // Indented line -> likely posting account
    if is_indented(line_text) {
        // Trim leading whitespace to get content
        let trimmed = line_text.trim_start();
        // If it looks like posting (has spaces, account before space)
        if trimmed.contains(' ') && !trimmed.starts_with('#') && !trimmed.starts_with('^') {
            // Account part is before first space
            if let Some(sp) = trimmed.find(' ') {
                let acc_part = &trimmed[..sp];
                let prefix = acc_part.to_string();
                return CompletionContext::PostingAccount { prefix };
            }
        } else {
            let prefix = trimmed.to_string();
            return CompletionContext::PostingAccount { prefix };
        }
    }

    // Not indented -> directive header
    let trimmed = line_text.trim_start();
    if trimmed.is_empty() {
        return CompletionContext::DirectiveKeyword {
            date_range: Some(Range::new(Position::new(position.line, 0), *position)),
        };
    }
    let date_prefix = trimmed.len() <= 10
        && trimmed
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-');
    if !is_indented(line_text) && date_prefix {
        return CompletionContext::DirectiveKeyword {
            date_range: Some(Range::new(Position::new(position.line, 0), *position)),
        };
    }
    // Check for keywords
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.is_empty() {
        return CompletionContext::DirectiveKeyword { date_range: None };
    }
    let keyword_index = usize::from(is_complete_date_token(words[0]));
    let first = words.get(keyword_index).copied().unwrap_or_default();
    match first {
        "open" | "close" | "balance" => {
            if words.len() > keyword_index + 1 {
                let prefix = words[words.len() - 1].to_string();
                return CompletionContext::Account { prefix };
            }
            CompletionContext::Account {
                prefix: String::new(),
            }
        }
        "commodity" => {
            if words.len() > keyword_index + 1 {
                let prefix = words[words.len() - 1].to_string();
                return CompletionContext::Commodity { prefix };
            }
            CompletionContext::Commodity {
                prefix: String::new(),
            }
        }
        "price" => {
            let tail = &words[keyword_index + 1..];
            let trailing_whitespace = line_text.chars().last().is_some_and(char::is_whitespace);
            match tail {
                [] => CompletionContext::PriceBase {
                    prefix: String::new(),
                },
                [base] if !trailing_whitespace => CompletionContext::PriceBase {
                    prefix: (*base).to_string(),
                },
                [_, number] if trailing_whitespace && number_looks_valid(number) => {
                    CompletionContext::PriceQuote {
                        prefix: String::new(),
                    }
                }
                [_, number, quote] if !trailing_whitespace && number_looks_valid(number) => {
                    CompletionContext::PriceQuote {
                        prefix: (*quote).to_string(),
                    }
                }
                _ => CompletionContext::Unknown,
            }
        }
        "payee" => CompletionContext::Payee {
            prefix: String::new(),
        },
        "include" => CompletionContext::IncludePath {
            prefix: String::new(),
        },
        _ => {
            if !is_indented(line_text) && !trimmed.contains(' ') {
                return CompletionContext::DirectiveKeyword { date_range: None };
            }
            CompletionContext::Unknown
        }
    }
}

fn completions_for_context(
    ctx: &CompletionContext,
    index: &SymbolIndex,
    current_doc_path: Option<&Path>,
) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();
    match ctx {
        CompletionContext::DirectiveKeyword { date_range } => {
            if let Some(range) = date_range {
                let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
                items.push(CompletionItem {
                    label: today.clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    detail: Some("Today's date".to_string()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: *range,
                        new_text: today,
                    })),
                    ..Default::default()
                });
            }
            for (kw, doc) in directive_keywords_list() {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    detail: Some(doc.to_string()),
                    ..Default::default()
                });
            }
        }
        CompletionContext::IncludePath { prefix } => {
            if let Some(dir) = current_doc_path.and_then(|p| p.parent()) {
                let mut seen: HashSet<PathBuf> = HashSet::new();
                if let Ok(entries) = dir.read_dir() {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str())
                                && ext == "pactole"
                                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                                && name.starts_with(prefix)
                            {
                                seen.insert(path.clone());
                            }
                        } else if path.is_dir() {
                            // skip
                        }
                    }
                }
                for p in seen {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        items.push(CompletionItem {
                            label: format!("\"{}\"", name),
                            kind: Some(CompletionItemKind::FILE),
                            insert_text: Some(format!("{}\"", name)),
                            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        CompletionContext::Payee { prefix } => {
            for p in &index.payees {
                if p.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: p.clone(),
                        kind: Some(CompletionItemKind::TEXT),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::Account { prefix } | CompletionContext::PostingAccount { prefix } => {
            for acc in &index.accounts_open {
                if acc.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: acc.clone(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        ..Default::default()
                    });
                }
            }
            if items.is_empty() {
                for acc in &index.accounts_all {
                    if acc.starts_with(prefix) {
                        items.push(CompletionItem {
                            label: acc.clone(),
                            kind: Some(CompletionItemKind::CONSTANT),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        CompletionContext::Commodity { prefix } => {
            for c in &index.commodities {
                if c.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: c.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::PriceBase { prefix } | CompletionContext::PriceQuote { prefix } => {
            for commodity in &index.commodities {
                if commodity.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: commodity.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::Tag { prefix } => {
            for t in &index.tags {
                if t.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: t.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::Link { prefix } => {
            for l in &index.links {
                if l.starts_with(prefix) {
                    items.push(CompletionItem {
                        label: l.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        ..Default::default()
                    });
                }
            }
        }
        CompletionContext::Unknown => {}
    }
    items
}

fn number_looks_valid(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b',' | b'.'))
}

/// Computes completion items for the requested document position.
pub fn handle_completion(
    config: &Config,
    documents: &Documents,
    params: CompletionParams,
) -> Result<CompletionList, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let offset = position_to_offset(&source, &pos);

    let ctx = context_at(&source, offset, &pos);
    let current_doc_path = uri_to_path(&uri);

    let index = if let Some(project) = analyze_scope(config, documents, &uri) {
        build_index_from_project(&project)
    } else {
        let file = pactole_storage_fs::analyze_file(&source);
        build_index_from_file(&file)
    };

    let items = completions_for_context(&ctx, &index, current_doc_path.as_deref());
    Ok(CompletionList {
        is_incomplete: false,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_directive_line_offers_today_date() {
        let position = Position::new(0, 0);
        let context = context_at("", 0, &position);
        let items = completions_for_context(&context, &SymbolIndex::default(), None);

        let date = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let item = items
            .iter()
            .find(|item| item.label == date)
            .expect("today's date completion");

        assert_eq!(item.detail.as_deref(), Some("Today's date"));
        assert!(matches!(
            item.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit { range, .. }))
                if range == Range::new(Position::new(0, 0), Position::new(0, 0))
        ));
    }

    #[test]
    fn partial_date_is_replaced_by_today() {
        let position = Position::new(0, 5);
        let context = context_at("2026-", 5, &position);
        let items = completions_for_context(&context, &SymbolIndex::default(), None);

        let item = items
            .iter()
            .find(|item| item.detail.as_deref() == Some("Today's date"))
            .expect("today's date completion");
        assert!(matches!(
            &item.text_edit,
            Some(CompletionTextEdit::Edit(TextEdit { range, new_text }))
                if *range == Range::new(Position::new(0, 0), Position::new(0, 5))
                    && new_text == &Local::now().date_naive().format("%Y-%m-%d").to_string()
        ));
    }

    #[test]
    fn ordinary_keyword_prefix_does_not_offer_a_date() {
        let position = Position::new(0, 2);
        let context = context_at("op", 2, &position);
        let items = completions_for_context(&context, &SymbolIndex::default(), None);

        assert!(
            items
                .iter()
                .all(|item| item.detail.as_deref() != Some("Today's date"))
        );
        assert!(items.iter().any(|item| item.label == "open"));
    }

    #[test]
    fn recognizes_account_completion_after_a_dated_open_directive() {
        let source = "2026-01-01 open Assets:Che";
        let position = Position::new(0, source.len() as u32);
        let context = context_at(source, source.len(), &position);
        let items = completions_for_context(
            &context,
            &SymbolIndex {
                accounts_open: ["Assets:Checking".to_string()].into(),
                ..Default::default()
            },
            None,
        );

        assert!(items.iter().any(|item| item.label == "Assets:Checking"));
    }

    #[test]
    fn offers_commodity_completions_for_price_base_and_quote() {
        let commodities: BTreeSet<String> = ["EUR".to_string(), "USD".to_string()].into();
        for (source, expected_context, prefix) in [
            (
                "2026-01-01 price US",
                CompletionContext::PriceBase {
                    prefix: "US".to_string(),
                },
                "USD",
            ),
            (
                "2026-01-01 price USD 0.92 E",
                CompletionContext::PriceQuote {
                    prefix: "E".to_string(),
                },
                "EUR",
            ),
        ] {
            let position = Position::new(0, source.len() as u32);
            let context = context_at(source, source.len(), &position);
            assert_eq!(context, expected_context);
            let items = completions_for_context(
                &context,
                &SymbolIndex {
                    commodities: commodities.clone(),
                    ..Default::default()
                },
                None,
            );
            assert!(items.iter().any(|item| item.label == prefix));
        }
    }

    #[test]
    fn offers_commodity_completions_after_unit_and_total_costs() {
        for source in [
            "  Assets:USD -100 USD @ 0.93 E",
            "  Assets:EUR 93 EUR @@ 93 E",
        ] {
            let position = Position::new(0, source.len() as u32);
            assert_eq!(
                context_at(source, source.len(), &position),
                CompletionContext::Commodity {
                    prefix: "E".to_string(),
                }
            );
        }
    }

    #[test]
    fn offers_commodity_completion_after_a_balance_amount() {
        for source in [
            "2026-01-01 balance Assets:Checking 10.00 ",
            "2026-01-01 balance Assets:Checking 10.00 ~ 0.01 ",
        ] {
            let position = Position::new(0, source.len() as u32);
            assert_eq!(
                context_at(source, source.len(), &position),
                CompletionContext::Commodity {
                    prefix: String::new(),
                }
            );
        }
    }

    #[test]
    fn does_not_offer_account_completions_after_a_cost_operator() {
        for source in ["  Assets:USD 10 USD @", "  Assets:USD 10 USD @@ "] {
            let position = Position::new(0, source.len() as u32);
            assert_eq!(
                context_at(source, source.len(), &position),
                CompletionContext::Unknown
            );
        }
    }

    #[test]
    fn recognizes_payee_completion_in_a_transaction_header() {
        let source = "2026-01-01 * \"Car";
        let position = Position::new(0, source.len() as u32);
        let context = context_at(source, source.len(), &position);
        let items = completions_for_context(
            &context,
            &SymbolIndex {
                payees: ["Carrefour".to_string()].into(),
                ..Default::default()
            },
            None,
        );

        assert!(items.iter().any(|item| item.label == "Carrefour"));
    }

    #[test]
    fn handles_utf8_characters_before_the_cursor() {
        let source = "payee \"é\"\nop";
        let offset = position_to_offset(source, &Position::new(0, 9));

        assert_eq!(offset, 10);
        assert_eq!(
            context_at(source, offset, &Position::new(0, 9)),
            CompletionContext::Payee {
                prefix: String::new(),
            }
        );
    }

    #[test]
    fn handles_a_single_numeric_token_without_panicking() {
        for source in [" 10", " 10 "] {
            let position = Position::new(0, source.chars().count() as u32);
            let offset = position_to_offset(source, &position);

            assert_eq!(
                context_at(source, offset, &position),
                CompletionContext::PostingAccount {
                    prefix: "10".to_string(),
                }
            );
        }
    }
}
