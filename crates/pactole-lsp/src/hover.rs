//! Hover support for `.pactole` files.

use std::error::Error;
use std::path::PathBuf;

use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Url};

use pactole_core::Entry;
use pactole_storage_fs::{AnalyzedProject, ParsedFile};

use crate::config::Config;
use crate::conversion::position_to_offset;
use crate::documents::{Documents, DocumentsSourceLoader};

#[derive(Clone, Debug)]
struct SymbolInfo {
    kind: &'static str,
    detail: String,
}

fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

fn find_word_at(text: &str, offset: usize) -> String {
    if offset >= text.len() {
        return String::new();
    }
    let mut start = offset;
    while start > 0 {
        let byte = start - 1;
        if byte < text.len() {
            let ch = text[byte..start].chars().next().unwrap_or('\n');
            if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == ':' || ch == '#' || ch == '^' {
                start = byte;
                continue;
            }
        }
        break;
    }
    let mut end = offset;
    while end < text.len() {
        let ch = text[end..].chars().next().unwrap_or('\n');
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == ':' || ch == '#' || ch == '^' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    text[start..end].to_string()
}

fn build_hover_for_word_combined(
    word: &str,
    project: Option<&AnalyzedProject>,
    current_file: &ParsedFile,
) -> Option<SymbolInfo> {
    if word.is_empty() {
        return None;
    }
    if word.starts_with('#') {
        let tag = &word[1..];
        return Some(SymbolInfo {
            kind: "Tag",
            detail: format!("Tag `#{}`", tag),
        });
    }
    if word.starts_with('^') {
        let link = &word[1..];
        return Some(SymbolInfo {
            kind: "Link",
            detail: format!("Link `^{}`", link),
        });
    }

    for entry in current_file.entries() {
        match &entry.entry {
            Entry::Payee(p) => {
                if p.name == word {
                    return Some(SymbolInfo {
                        kind: "Payee",
                        detail: format!("Payee declared: `{}`", word),
                    });
                }
            }
            Entry::Commodity(c) => {
                if c.name.to_string() == word {
                    return Some(SymbolInfo {
                        kind: "Commodity",
                        detail: format!("Commodity declared: `{}`", word),
                    });
                }
            }
            Entry::Open(o) => {
                if o.account.to_string() == word {
                    return Some(SymbolInfo {
                        kind: "Account",
                        detail: format!("Account `{}`\n- opened on {}", word, o.date),
                    });
                }
            }
            Entry::Close(c) => {
                if c.account.to_string() == word {
                    return Some(SymbolInfo {
                        kind: "Account",
                        detail: format!("Account `{}`\n- closed on {}", word, c.date),
                    });
                }
            }
            _ => {}
        }
    }

    if let Some(project) = project {
        for file in project.files() {
            for entry in file.file.entries() {
                match &entry.entry {
                    Entry::Payee(p) => {
                        if p.name == word {
                            return Some(SymbolInfo {
                                kind: "Payee",
                                detail: format!("Payee declared: `{}`", word),
                            });
                        }
                    }
                    Entry::Commodity(c) => {
                        if c.name.to_string() == word {
                            return Some(SymbolInfo {
                                kind: "Commodity",
                                detail: format!("Commodity declared: `{}`", word),
                            });
                        }
                    }
                    Entry::Open(o) => {
                        if o.account.to_string() == word {
                            return Some(SymbolInfo {
                                kind: "Account",
                                detail: format!("Account `{}`\n- opened on {}", word, o.date),
                            });
                        }
                    }
                    Entry::Close(c) => {
                        if c.account.to_string() == word {
                            return Some(SymbolInfo {
                                kind: "Account",
                                detail: format!("Account `{}`\n- closed on {}", word, c.date),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

pub fn handle_hover(
    config: &Config,
    documents: &Documents,
    params: HoverParams,
) -> Result<Option<Hover>, Box<dyn Error + Sync + Send>> {
    let uri = params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;

    let source = documents
        .get(&uri)
        .ok_or_else(|| format!("no document for {uri}"))?
        .to_string();
    let offset = position_to_offset(&source, &pos);

    let word = find_word_at(&source, offset);
    if word.is_empty() {
        return Ok(None);
    }

    let current_doc_path = uri_to_path(&uri);
    let file = pactole_storage_fs::analyze_file(&source);

    let project = if let Some(journal_file) = &config.journal_file {
        let loader = DocumentsSourceLoader::new(documents);
        pactole_storage_fs::analyze_file_with_loader(journal_file, &loader).ok()
    } else {
        let loader = DocumentsSourceLoader::new(documents);
        if let Some(p) = current_doc_path.as_deref() {
            pactole_storage_fs::analyze_file_with_loader(p, &loader).ok()
        } else {
            None
        }
    };

    if let Some(info) = build_hover_for_word_combined(&word, project.as_ref(), &file) {
        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**{}**\n\n{}", info.kind, info.detail),
        });
        return Ok(Some(Hover { contents, range: None }));
    }

    Ok(None)
}
