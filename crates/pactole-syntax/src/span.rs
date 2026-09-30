//! Byte-offset and row/column spans for syntax nodes.

use std::fmt;

/// A single position in the source text, expressed both as a byte offset and
/// as a zero-indexed (row, column) pair.
///
/// The `column` is a byte offset within its `row`, matching tree-sitter's
/// [`tree_sitter::Point`] semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    /// Zero-indexed byte offset of the position within the whole source text.
    pub byte: usize,
    /// Zero-indexed row (line number).
    pub row: usize,
    /// Zero-indexed column (byte offset within `row`).
    pub column: usize,
}

impl Point {
    /// Builds a [`Point`] from a tree-sitter [`tree_sitter::Point`] and a byte offset.
    pub fn new(byte: usize, ts_point: tree_sitter::Point) -> Self {
        Self {
            byte,
            row: ts_point.row,
            column: ts_point.column,
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.row + 1, self.column + 1)
    }
}

/// A contiguous range in the source text, delimited by a start and end [`Point`].
///
/// The range is half-open: `start` is inclusive and `end` is exclusive, mirroring
/// tree-sitter's node range semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Inclusive start position of the span.
    pub start: Point,
    /// Exclusive end position of the span.
    pub end: Point,
}

impl Span {
    /// Builds a [`Span`] from a start and end [`Point`].
    pub fn new(start: Point, end: Point) -> Self {
        Self { start, end }
    }

    /// Builds a [`Span`] covering the full range of a tree-sitter [`tree_sitter::Node`].
    pub fn from_node(node: &tree_sitter::Node) -> Self {
        Self {
            start: Point::new(node.start_byte(), node.start_position()),
            end: Point::new(node.end_byte(), node.end_position()),
        }
    }

    /// The byte range covered by this span, usable to slice the original source text.
    pub fn byte_range(&self) -> std::ops::Range<usize> {
        self.start.byte..self.end.byte
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}
