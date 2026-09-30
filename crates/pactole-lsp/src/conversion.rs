//! Conversions between `pactole-syntax`'s byte-offset-based
//! [`pactole_syntax::Span`]/[`pactole_syntax::Point`] and LSP's UTF-16-based
//! [`lsp_types::Range`]/[`lsp_types::Position`].
//!
//! LSP positions are `(line, character)` pairs where `character` counts
//! UTF-16 code units within the line, while `pactole-syntax` spans carry a
//! byte offset and a byte-offset column (mirroring tree-sitter). Converting
//! between the two requires re-scanning the relevant source line, since a
//! byte offset alone is not enough to know how many UTF-16 code units
//! precede it once the source contains non-ASCII characters.

use pactole_syntax::{Point, Span};

/// Converts a byte offset within `source` into an LSP [`lsp_types::Position`]
/// (UTF-16 line/character).
///
/// `line` is the zero-indexed row also carried by [`Point::row`]; passing it
/// in avoids re-scanning the whole source to find which line `byte_offset`
/// falls on when the caller already knows it (as is always the case here,
/// since [`Point`] already carries `row`).
fn byte_offset_to_utf16_position(
    source: &str,
    line: usize,
    byte_offset_in_line: usize,
) -> lsp_types::Position {
    let line_start = line_start_byte_offset(source, line);
    let line_text = &source[line_start..];
    let mut utf16_character = 0u32;
    let mut byte_pos = 0usize;
    for ch in line_text.chars() {
        if byte_pos >= byte_offset_in_line {
            break;
        }
        byte_pos += ch.len_utf8();
        utf16_character += ch.len_utf16() as u32;
    }
    lsp_types::Position {
        line: line as u32,
        character: utf16_character,
    }
}

/// Finds the byte offset of the start of `line` (zero-indexed) within
/// `source`.
fn line_start_byte_offset(source: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    source
        .match_indices('\n')
        .nth(line - 1)
        .map(|(idx, _)| idx + 1)
        .unwrap_or(source.len())
}

/// Converts a [`Point`] into an LSP [`lsp_types::Position`], using `source`
/// to translate the byte-offset column into a UTF-16 character offset.
pub fn point_to_position(source: &str, point: &Point) -> lsp_types::Position {
    let line_start = line_start_byte_offset(source, point.row);
    let byte_offset_in_line = point.byte.saturating_sub(line_start);
    byte_offset_to_utf16_position(source, point.row, byte_offset_in_line)
}

/// Converts a [`Span`] into an LSP [`lsp_types::Range`], using `source` to
/// translate byte-offset columns into UTF-16 character offsets.
pub fn span_to_range(source: &str, span: &Span) -> lsp_types::Range {
    lsp_types::Range {
        start: point_to_position(source, &span.start),
        end: point_to_position(source, &span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_syntax::{Point, Span};

    #[test]
    fn ascii_single_line_span_maps_directly() {
        let source = "2026-09-03 open Assets:Checking\n";
        let span = Span::new(
            Point {
                byte: 0,
                row: 0,
                column: 0,
            },
            Point {
                byte: 11,
                row: 0,
                column: 11,
            },
        );

        let range = span_to_range(source, &span);
        assert_eq!(range.start, lsp_types::Position::new(0, 0));
        assert_eq!(range.end, lsp_types::Position::new(0, 11));
    }

    #[test]
    fn span_on_a_later_line_uses_the_column_within_that_line() {
        let source = "2026-09-03 open Assets:Checking\n2026-09-04 close Assets:Checking\n";
        // Second line starts at byte 32; a span covering "close" (bytes 43..48)
        // has column 11 within line 1.
        let span = Span::new(
            Point {
                byte: 43,
                row: 1,
                column: 11,
            },
            Point {
                byte: 48,
                row: 1,
                column: 16,
            },
        );

        let range = span_to_range(source, &span);
        assert_eq!(range.start, lsp_types::Position::new(1, 11));
        assert_eq!(range.end, lsp_types::Position::new(1, 16));
    }

    #[test]
    fn multi_byte_utf8_characters_shift_the_utf16_character_offset() {
        // "café" has a 2-byte UTF-8 'é' but occupies a single UTF-16 code
        // unit, so a byte column past it must not equal the UTF-16 character
        // offset naively.
        let source = "payee \"café\"\n";
        // Byte offset right after "café" (6 + 5 bytes for "café" = "c","a","f","é"(2 bytes) => 5 bytes).
        let byte_after_cafe = 6 + 5;
        let point = Point {
            byte: byte_after_cafe,
            row: 0,
            column: byte_after_cafe,
        };

        let position = point_to_position(source, &point);
        // UTF-16 character offset: "payee \"" (7 chars) + "café" (4 UTF-16 units) = 11.
        assert_eq!(position, lsp_types::Position::new(0, 11));
    }

    #[test]
    fn line_start_byte_offset_finds_subsequent_lines() {
        let source = "a\nbb\nccc\n";
        assert_eq!(line_start_byte_offset(source, 0), 0);
        assert_eq!(line_start_byte_offset(source, 1), 2);
        assert_eq!(line_start_byte_offset(source, 2), 5);
    }
}
