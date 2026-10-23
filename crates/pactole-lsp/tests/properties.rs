//! Property-based tests for the byte-offset <-> UTF-16 conversions.
//!
//! LSP positions count UTF-16 code units within a line, while `pactole-syntax`
//! spans carry byte offsets. The conversions in [`pactole_lsp::conversion`]
//! must therefore stay exact for any text, not only for the hand-written
//! examples in the unit tests: a single character counted in the wrong unit
//! shifts every subsequent diagnostic on the line.

use pactole_lsp::conversion::{
    point_to_position, position_to_offset, span_to_range, text_at_range,
};
use pactole_syntax::{Point, Span};
use proptest::prelude::*;

/// Arbitrary text, short enough to keep the tests fast. `any::<char>()`
/// covers ASCII, multi-byte UTF-8 and non-BMP characters (which occupy two
/// UTF-16 code units).
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..80).prop_map(|chars| chars.into_iter().collect())
}

/// The byte offset where `line` starts.
fn line_start(source: &str, byte: usize) -> usize {
    source[..byte]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

/// Builds the [`Point`] a tree-sitter node would carry for `byte`: the row
/// and the byte column within it.
fn point_at(source: &str, byte: usize) -> Point {
    let start = line_start(source, byte);
    Point {
        byte,
        row: source[..byte].matches('\n').count(),
        column: byte - start,
    }
}

/// Every byte offset at which a slice of `source` starts: all character
/// boundaries, plus the end of the text.
fn boundaries(source: &str) -> Vec<usize> {
    source
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(source.len()))
        .collect()
}

proptest! {
    #[test]
    fn byte_offsets_round_trip_through_utf16_positions(
        source in text(),
        index in any::<prop::sample::Index>(),
    ) {
        let boundaries = boundaries(&source);
        let byte = boundaries[index.index(boundaries.len())];

        let position = point_to_position(&source, &point_at(&source, byte));
        let round_tripped = position_to_offset(&source, &position);

        prop_assert_eq!(
            round_tripped,
            byte,
            "byte {} became {:?} and back to {} in {:?}",
            byte,
            position,
            round_tripped,
            source
        );
    }

    #[test]
    fn a_span_maps_to_the_text_it_covers(
        source in text(),
        start in any::<prop::sample::Index>(),
        length in any::<prop::sample::Index>(),
    ) {
        let boundaries = boundaries(&source);
        let start_index = start.index(boundaries.len());
        // Any boundary at or after the start, so the span is never reversed.
        let end_index = start_index + length.index(boundaries.len() - start_index);
        let start_byte = boundaries[start_index];
        let end_byte = boundaries[end_index];

        let span = Span::new(point_at(&source, start_byte), point_at(&source, end_byte));
        let range = span_to_range(&source, &span);

        prop_assert_eq!(
            text_at_range(&source, &range),
            source.get(span.byte_range()),
            "span {:?} mapped to {:?} in {:?}",
            span.byte_range(),
            range,
            source
        );
    }

    #[test]
    fn the_utf16_column_counts_code_units_not_bytes(
        source in text(),
        index in any::<prop::sample::Index>(),
    ) {
        let boundaries = boundaries(&source);
        let byte = boundaries[index.index(boundaries.len())];
        let start = line_start(&source, byte);

        // The definition of the UTF-16 column, computed independently of the
        // conversion under test.
        let expected: u32 = source[start..byte].chars().map(|ch| ch.len_utf16() as u32).sum();
        let position = point_to_position(&source, &point_at(&source, byte));

        prop_assert_eq!(position.character, expected, "at byte {} of {:?}", byte, source);
        prop_assert_eq!(position.line as usize, source[..byte].matches('\n').count());
    }
}
