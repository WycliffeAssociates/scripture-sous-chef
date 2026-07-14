//! Addressing.
//!
//! Byte offsets into the verse `text` sous was handed are the canonical
//! unit (`Span`) — Rust-native, zero-cost to slice, and matching onion's
//! `source_span` so the two engines share coordinates. Other units are
//! pure projections computed against the *same* `&str`; none allocate an
//! owned `String` (the only string copy is the one the wasm boundary
//! forces). See ADR 0010.

use unicode_segmentation::UnicodeSegmentation;

/// UTF-8 byte offsets into the verse text. The canonical addressing unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// UTF-16 code-unit offsets into the verse text. The JS/web target unit
/// (what `Range.setStart` and `String.slice` consume natively); the name
/// flags the unusual unit, the same convention onion uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Utf16Span {
    pub start: u32,
    pub end: u32,
}

/// Grapheme-cluster (user-perceived character) offsets into the verse
/// text. For selection/preview UIs that count "characters" the way a
/// human does — NOT for DOM ranges, which take UTF-16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GraphemeSpan {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Zero-copy borrow of the addressed text. The Rust consumer's
    /// preview path — no allocation.
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start as usize..self.end as usize]
    }

    /// Project to UTF-16 code-unit offsets against `text`. The wasm
    /// wrapper applies this once at the boundary so JS never converts.
    pub fn to_utf16(&self, text: &str) -> Utf16Span {
        Utf16Span {
            start: text[..self.start as usize].encode_utf16().count() as u32,
            end: text[..self.end as usize].encode_utf16().count() as u32,
        }
    }

    /// Project to grapheme-cluster offsets against `text`.
    pub fn to_graphemes(&self, text: &str) -> GraphemeSpan {
        GraphemeSpan {
            start: text[..self.start as usize].graphemes(true).count() as u32,
            end: text[..self.end as usize].graphemes(true).count() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_is_the_addressed_text() {
        let text = "a  b";
        let s = Span { start: 1, end: 3 };
        assert_eq!(s.slice(text), "  ");
    }

    #[test]
    fn utf16_projection_counts_code_units() {
        // "😀" is 4 UTF-8 bytes but 2 UTF-16 code units.
        let text = "😀x";
        let s = Span { start: 4, end: 5 }; // the "x"
        assert_eq!(s.to_utf16(text), Utf16Span { start: 2, end: 3 });
    }

    #[test]
    fn grapheme_projection_counts_clusters() {
        // Family emoji ZWJ sequence is one grapheme, several codepoints.
        let text = "👨‍👩‍👧x";
        let x_start = text.len() as u32 - 1;
        let s = Span {
            start: x_start,
            end: text.len() as u32,
        };
        assert_eq!(s.to_graphemes(text), GraphemeSpan { start: 1, end: 2 });
    }
}
