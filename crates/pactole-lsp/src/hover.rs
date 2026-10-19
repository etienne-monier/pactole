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
    let offset = offset.min(text.len());
    if offset > 0 && !text.is_char_boundary(offset) {
        return String::new();
    }

    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    let line_before_cursor = &text[line_start..offset];
    if line_before_cursor.matches('"').count() % 2 == 1
        && let Some(open_quote) = line_before_cursor.rfind('"')
    {
        let content_start = line_start + open_quote + 1;
        let content_end = text[content_start..line_end]
            .find('"')
            .map_or(line_end, |index| content_start + index);
        return text[content_start..content_end].to_string();
    }

    fn is_word_character(ch: char) -> bool {
        ch.is_alphanumeric() || matches!(ch, '_' | '-' | ':' | '#' | '^')
    }

    let mut start = offset;
    while start > 0 {
        let Some((byte, ch)) = text[..start].char_indices().next_back() else {
            break;
        };
        if is_word_character(ch) {
            start = byte;
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < text.len() {
        let ch = text[end..].chars().next().unwrap_or('\n');
        if is_word_character(ch) {
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
    if let Some(tag) = word.strip_prefix('#') {
        return Some(SymbolInfo {
            kind: "Tag",
            detail: format!("Tag `#{}`", tag),
        });
    }
    if let Some(link) = word.strip_prefix('^') {
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
            Entry::Close(c) if c.account.to_string() == word => {
                return Some(SymbolInfo {
                    kind: "Account",
                    detail: format!("Account `{}`\n- closed on {}", word, c.date),
                });
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
                    Entry::Close(c) if c.account.to_string() == word => {
                        return Some(SymbolInfo {
                            kind: "Account",
                            detail: format!("Account `{}`\n- closed on {}", word, c.date),
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Computes hover information for the symbol at the requested document position.
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
        pactole_storage_fs::analyze_file_with_loader_with_options(
            journal_file,
            &loader,
            &config.validation,
        )
        .ok()
    } else {
        let loader = DocumentsSourceLoader::new(documents);
        if let Some(p) = current_doc_path.as_deref() {
            pactole_storage_fs::analyze_file_with_loader_with_options(
                p,
                &loader,
                &config.validation,
            )
            .ok()
        } else {
            None
        }
    };

    if let Some(info) = build_hover_for_word_combined(&word, project.as_ref(), &file) {
        let contents = HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("**{}**\n\n{}", info.kind, info.detail),
        });
        return Ok(Some(Hover {
            contents,
            range: None,
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_symbol_without_splitting_utf8_characters() {
        let source = "note café\n";
        let offset = source.find("café").unwrap() + "café".len();

        assert_eq!(find_word_at(source, offset), "café");
    }

    #[test]
    fn finds_the_complete_quoted_payee() {
        let source = "2026-01-01 * \"Whole Foods\"\n";
        let offset = source.find("Foods").unwrap() + 2;

        assert_eq!(find_word_at(source, offset), "Whole Foods");
    }

    #[test]
    fn finds_tags_and_links() {
        let source = "2026-01-01 * \"Shop\" #groceries ^receipt\n";

        assert_eq!(
            find_word_at(source, source.find("groceries").unwrap() + 4),
            "#groceries"
        );
        assert_eq!(
            find_word_at(source, source.find("receipt").unwrap() + 3),
            "^receipt"
        );
    }

    #[test]
    fn builds_hover_for_a_declared_symbol() {
        let source = "2026-01-01 open Assets:Checking\n";
        let file = pactole_storage_fs::analyze_file(source);
        let info = build_hover_for_word_combined("Assets:Checking", None, &file).unwrap();

        assert_eq!(info.kind, "Account");
        assert!(info.detail.contains("opened on 2026-01-01"));
    }
}
