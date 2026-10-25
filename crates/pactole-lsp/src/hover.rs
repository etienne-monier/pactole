//! Hover support for `.pactole` files.

use std::error::Error;

use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use pactole_core::Entry;
use pactole_storage_fs::{AnalyzedProject, ParsedFile};

use crate::config::Config;
use crate::conversion::position_to_offset;
use crate::documents::{Documents, analyze_scope};

#[derive(Clone, Debug)]
struct SymbolInfo {
    kind: &'static str,
    detail: String,
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

fn build_hover_for_price_at_offset(file: &ParsedFile, offset: usize) -> Option<SymbolInfo> {
    file.entries().iter().find_map(|entry| {
        let Entry::Price(price) = &entry.entry else {
            return None;
        };
        let [base, quote] = entry.spans.commodities.as_slice() else {
            return None;
        };

        // The rate is the only non-symbol token in the price header. Restrict
        // this hover to that token so commodity references retain their own
        // declaration hover and unrelated parts of the directive stay clear.
        let source = file.document().source();
        let between = source.get(base.byte_range().end..quote.byte_range().start)?;
        let rate_start = base.byte_range().end + (between.len() - between.trim_start().len());
        let rate_tail = source.get(rate_start..quote.byte_range().start)?;
        let rate_length = rate_tail
            .char_indices()
            .find(|(_, character)| character.is_whitespace())
            .map_or(rate_tail.len(), |(index, _)| index);
        if !(rate_start..rate_start + rate_length).contains(&offset) {
            return None;
        }

        Some(SymbolInfo {
            kind: "Market price",
            detail: format!(
                "{} {} = {} {}\n- quoted on {}",
                1, price.base, price.quote.number, price.quote.commodity, price.date
            ),
        })
    })
}

fn build_hover_at_offset(
    word: &str,
    project: Option<&AnalyzedProject>,
    file: &ParsedFile,
    offset: usize,
) -> Option<SymbolInfo> {
    build_hover_for_word_combined(word, project, file)
        .or_else(|| build_hover_for_price_at_offset(file, offset))
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

    let file = pactole_storage_fs::analyze_file(&source);
    let project = analyze_scope(config, documents, &uri);

    if let Some(info) = build_hover_at_offset(&word, project.as_ref(), &file, offset) {
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

    #[test]
    fn hovering_a_price_directive_shows_its_quote_and_date() {
        let source = concat!(
            "2026-01-01 commodity EUR\n",
            "2026-01-01 commodity USD\n",
            "2026-01-02 price USD 0.92 EUR\n",
        );
        let file = pactole_storage_fs::analyze_file(source);
        let offset = source.find("0.92").unwrap();
        let info = build_hover_for_price_at_offset(&file, offset).expect("price hover");

        assert_eq!(info.kind, "Market price");
        assert!(info.detail.contains("1 USD = 0.92 EUR"));
        assert!(info.detail.contains("2026-01-02"));
    }

    #[test]
    fn price_hover_does_not_hide_commodity_hover_or_apply_to_the_whole_directive() {
        let source = concat!(
            "2026-01-01 commodity EUR\n",
            "2026-01-01 commodity USD\n",
            "2026-01-02 price USD 0.92 EUR\n",
        );
        let file = pactole_storage_fs::analyze_file(source);

        let base_offset = source.rfind("USD").unwrap() + 1;
        let base_word = find_word_at(source, base_offset);
        let base_hover =
            build_hover_at_offset(&base_word, None, &file, base_offset).expect("commodity hover");
        assert_eq!(base_hover.kind, "Commodity");

        let quote_offset = source.rfind("EUR").unwrap() + 1;
        let quote_word = find_word_at(source, quote_offset);
        let quote_hover =
            build_hover_at_offset(&quote_word, None, &file, quote_offset).expect("commodity hover");
        assert_eq!(quote_hover.kind, "Commodity");

        let keyword_offset = source.rfind("price").unwrap() + 1;
        let keyword = find_word_at(source, keyword_offset);
        assert!(build_hover_at_offset(&keyword, None, &file, keyword_offset).is_none());
    }
}
