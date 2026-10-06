//! Completion logic for `.pactole` files.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, InsertTextFormat, Url,
};

use pactole_core::Entry;
use pactole_storage_fs::{AnalyzedProject, ParsedFile};

use crate::config::Config;
use crate::conversion::position_to_offset;
use crate::documents::{Documents, DocumentsSourceLoader};
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
    DirectiveKeyword,
    IncludePath {
        prefix: String,
    },
    Payee {
        prefix: String,
    },
    Account {
        prefix: String,
    },
    PostingAccount {
        prefix: String,
    },
    Commodity {
        prefix: String,
    },
    Tag {
        prefix: String,
    },
    Link {
        prefix: String,
    },
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

fn text_before_cursor(line_text: &str, char_utf16: u32) -> String {
    let mut utf16_count: u32 = 0;
    let mut result = String::new();
    for ch in line_text.chars() {
        if utf16_count >= char_utf16 {
            break;
        }
        result.push(ch);
        utf16_count += ch.len_utf16() as u32;
    }
    result
}

fn is_indented(line_text: &str) -> bool {
    line_text.starts_with(' ') || line_text.starts_with('\t')
}

fn directive_keywords_list() -> Vec<(&'static str, &'static str)> {
    vec![
        ("open", "Open an account"),
        ("close", "Close an account"),
        ("commodity", "Declare a commodity"),
        ("payee", "Declare a payee"),
        ("balance", "Balance assertion"),
        ("include", "Include another file"),
    ]
}

fn context_at(text: &str, offset: usize) -> CompletionContext {
    // Find line and char
    let mut line_start: usize = 0;
    let mut _line: usize = 0;
    for (i, ch) in text.char_indices().take(offset) {
        if ch == '\n' {
            line_start = i + 1;
            _line += 1;
        }
    }
    let line_text = &text[line_start..offset.min(text.len())];
    let rest = &text[offset..];

    // Check if inside quotes
    // Simple heuristic: count quotes on this line before offset? Or just look backwards
    // For include/payee we look for " before
    if let Some(quote_idx) = line_text.rfind('"') {
        // Look backwards from quote
        let before_quote = &line_text[..quote_idx];
        if before_quote.trim_end().ends_with("include") {
            let prefix = rest
                .chars()
                .take_while(|c| *c != '"' && *c != '\n' && *c != '\r')
                .collect();
            return CompletionContext::IncludePath { prefix };
        }
        if before_quote.trim_end().ends_with("payee") {
            let prefix = rest
                .chars()
                .take_while(|c| *c != '"' && *c != '\n' && *c != '\r')
                .collect();
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

    // Check for commodity after number
    // Look backwards for number pattern followed by space
    if let Some(space_idx) = line_text.rfind(' ') {
        let after_space = &line_text[space_idx + 1..];
        if after_space.chars().any(|c| c.is_ascii_digit() || c == '.')
            && !after_space.contains(':')
        {
            let prefix = after_space.to_string();
            // If it's just a number fragment, still maybe commodity? But better check - look ahead? No, rest is after offset
            // But we're at end of line_text; prefix is after_space up to cursor
            return CompletionContext::Commodity { prefix };
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
        return CompletionContext::DirectiveKeyword;
    }
    // Check for keywords
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.is_empty() {
        return CompletionContext::DirectiveKeyword;
    }
    let first = words[0];
    match first {
        "open" | "close" | "balance" => {
            if words.len() >= 2 {
                let prefix = words[words.len() - 1].to_string();
                return CompletionContext::Account { prefix };
            }
            return CompletionContext::Account { prefix: String::new() };
        }
        "commodity" => {
            if words.len() >= 2 {
                let prefix = words[words.len() - 1].to_string();
                return CompletionContext::Commodity { prefix };
            }
            return CompletionContext::Commodity { prefix: String::new() };
        }
        "payee" => CompletionContext::Payee { prefix: String::new() },
        "include" => CompletionContext::IncludePath { prefix: String::new() },
        _ => {
            if !is_indented(line_text) && !trimmed.contains(' ') {
                return CompletionContext::DirectiveKeyword;
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
        CompletionContext::DirectiveKeyword => {
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
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if ext == "pactole" {
                                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                        if name.starts_with(prefix) {
                                            seen.insert(path.clone());
                                        }
                                    }
                                }
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

fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

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

    let ctx = context_at(&source, offset);
    let current_doc_path = uri_to_path(&uri);

    let index = if let Some(journal_file) = &config.journal_file {
        let loader = DocumentsSourceLoader::new(documents);
        match pactole_storage_fs::analyze_file_with_loader(journal_file, &loader) {
            Ok(project) => build_index_from_project(&project),
            Err(_) => {
                let file = pactole_storage_fs::analyze_file(&source);
                build_index_from_file(&file)
            }
        }
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

