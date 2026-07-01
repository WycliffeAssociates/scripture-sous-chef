//! Fused per-character classification (ADR 0020).
//!
//! The char-walking rules (casing, repeated-character-run) ask several
//! Unicode questions per grapheme — is it a letter? cased? whitespace? a
//! digit? Each std `char` predicate is a separate table lookup, and on
//! non-ASCII text (Devanagari, Thai, CJK …) that's ~five table walks per
//! character. This module answers all of them in **one** lookup by packing
//! the answers into a [`Class`] byte, precomputed per distinct character into
//! a small two-level [`CharClass`] table.
//!
//! The table is built **per analyze** over the text being analyzed (a page is
//! allocated only for a codepoint block the text actually uses), so it is a
//! few KB — not the 128 KB a flat `[_; 0x10000]` would need — and carries no
//! process-global or per-corpus resident state. See ADR 0020 for the
//! flat-table and stateful-reuse alternatives that were weighed and deferred.

/// Packed per-character classification bits. Only specific bits are queried,
/// so the internal [`COMPUTED`] marker is inert to callers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Class(u8);

const ALPHA: u8 = 1 << 0;
const LOWER: u8 = 1 << 1;
const UPPER: u8 = 1 << 2;
const WHITESPACE: u8 = 1 << 3;
const NUMERIC: u8 = 1 << 4;
const DECIMAL: u8 = 1 << 5;
// bit 6 reserved — e.g. a future `clinging` flag (closing quotes/brackets).
/// Set on every classified cell so a char whose real bits are 0 (no flags)
/// still reads non-zero — otherwise the build can't tell "unfilled" from
/// "classified as none" and would re-classify it on every occurrence.
const COMPUTED: u8 = 1 << 7;

impl Class {
    #[inline]
    pub fn is_alphabetic(self) -> bool {
        self.0 & ALPHA != 0
    }
    #[inline]
    pub fn is_lowercase(self) -> bool {
        self.0 & LOWER != 0
    }
    #[inline]
    pub fn is_uppercase(self) -> bool {
        self.0 & UPPER != 0
    }
    #[inline]
    pub fn is_whitespace(self) -> bool {
        self.0 & WHITESPACE != 0
    }
    #[inline]
    pub fn is_numeric(self) -> bool {
        self.0 & NUMERIC != 0
    }
    #[inline]
    pub fn is_decimal_digit(self) -> bool {
        self.0 & DECIMAL != 0
    }
}

/// Compute a character's bits from the std/UCD predicates. Called once per
/// distinct char at build time (and directly for the rare astral fallback).
fn classify(c: char) -> u8 {
    let mut b = COMPUTED;
    if c.is_alphabetic() {
        b |= ALPHA;
    }
    if c.is_lowercase() {
        b |= LOWER;
    }
    if c.is_uppercase() {
        b |= UPPER;
    }
    if c.is_whitespace() {
        b |= WHITESPACE;
    }
    if c.is_numeric() {
        b |= NUMERIC;
    }
    if crate::unicode::is_decimal_digit(c) {
        b |= DECIMAL;
    }
    b
}

/// Two-level page table over the Basic Multilingual Plane: the high byte of a
/// codepoint selects a 256-byte block, the low byte indexes within it. Only
/// pages the input actually uses get a block, so a single-script corpus needs
/// ~1–3 KB. Astral codepoints (≥ U+10000) — vanishingly rare in scripture —
/// take a direct `classify` fallback rather than a fourth plane of table.
pub struct CharClass {
    index: Vec<u16>,        // 256 page slots -> block id (0 = shared zero block)
    blocks: Vec<[u8; 256]>, // block 0 is all-zero
}

impl CharClass {
    /// Build a table covering every character in `texts`, classifying each
    /// distinct scalar exactly once.
    pub fn build<'a>(texts: impl Iterator<Item = &'a str>) -> CharClass {
        let mut index = vec![0u16; 256];
        let mut blocks: Vec<[u8; 256]> = vec![[0u8; 256]];
        for text in texts {
            for c in text.chars() {
                let cp = c as u32;
                if cp >= 0x10000 {
                    continue; // astral -> direct fallback in `get`
                }
                let page = (cp >> 8) as usize;
                let off = (cp & 0xFF) as usize;
                let mut bid = index[page] as usize;
                if bid == 0 {
                    blocks.push([0u8; 256]);
                    bid = blocks.len() - 1;
                    index[page] = bid as u16;
                }
                if blocks[bid][off] == 0 {
                    blocks[bid][off] = classify(c);
                }
            }
        }
        CharClass { index, blocks }
    }

    /// The classification of `c` — one table read for BMP chars.
    #[inline]
    pub fn get(&self, c: char) -> Class {
        let cp = c as u32;
        if cp < 0x10000 {
            Class(self.blocks[self.index[(cp >> 8) as usize] as usize][(cp & 0xFF) as usize])
        } else {
            Class(classify(c))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fused table must agree with the std predicates for every char it
    /// was built over — including combining marks, digits, and astral.
    #[test]
    fn matches_std_predicates() {
        let sample = "Aa1 .;Ελληνικά देवनागरी ๗ไทย \u{0301}\u{0E48}𝐀🙏";
        let cc = CharClass::build(std::iter::once(sample));
        for c in sample.chars() {
            let cl = cc.get(c);
            assert_eq!(cl.is_alphabetic(), c.is_alphabetic(), "alpha {c:?}");
            assert_eq!(cl.is_lowercase(), c.is_lowercase(), "lower {c:?}");
            assert_eq!(cl.is_uppercase(), c.is_uppercase(), "upper {c:?}");
            assert_eq!(cl.is_whitespace(), c.is_whitespace(), "ws {c:?}");
            assert_eq!(cl.is_numeric(), c.is_numeric(), "numeric {c:?}");
            assert_eq!(
                cl.is_decimal_digit(),
                crate::unicode::is_decimal_digit(c),
                "decimal {c:?}"
            );
        }
    }

    /// A char whose real bits are 0 (no flags) must still classify once and
    /// read back as all-false — the COMPUTED marker must stay inert.
    #[test]
    fn zero_classification_reads_all_false() {
        // U+0E48 THAI CHARACTER MAI EK — a tone mark: not alpha/case/ws/digit.
        let cc = CharClass::build(std::iter::once("\u{0E48}"));
        let cl = cc.get('\u{0E48}');
        assert!(!cl.is_alphabetic() && !cl.is_lowercase() && !cl.is_uppercase());
        assert!(!cl.is_whitespace() && !cl.is_numeric() && !cl.is_decimal_digit());
    }
}
