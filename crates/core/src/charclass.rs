//! Fused per-character classification (ADR 0020, amended by 0021 and 0022).
//!
//! The char-walking rules ask several Unicode questions per grapheme — is it a
//! letter? cased? a digit? a mark/punctuation/symbol? which script? — and the
//! grapheme segmenter ([`crate::grapheme`]) asks grapheme-break questions — does
//! this scalar glue to the previous cluster? does it need the full UAX-#29
//! rules? This module answers **all** of them in one lookup by packing the
//! answers into a [`Class`] `u32` (flag bits + an 8-bit script lane) and reading
//! it from a single static table.
//!
//! **Why static, not per-analyze (amending ADR 0020).** ADR 0020 backed this
//! with a per-analyze trie because casing bits are std predicates, computable
//! per-char from nothing. Grapheme-break and script bits (`Extend`/`Prepend`/
//! `InCB`/script identity …) are *not* computable from `std` and are not
//! exposed by `unicode-segmentation`; they can only come from committed Unicode
//! property data, resident in the binary. Once that data must be resident to
//! segment at all, a per-analyze rebuild earns nothing — so the fused `u32`
//! table is built **once** at first use from the compact committed range table
//! ([`crate::charclass_table`], generated offline from UCD 17.0 + the
//! `unicode-*` crates). The `.wasm` grows only by the ranges (~tens of KB); the
//! flat BMP table is a ~256 KB heap allocation (`u32 × 65536`) for process life.
//! See ADR 0022 for the u32-vs-parallel-byte-table reasoning.

use std::sync::OnceLock;

use crate::charclass_table::CLASS_RANGES;
use crate::script::{self, ScriptTag};

/// Packed per-character classification (ADR 0020/0021/0022): casing + lexical
/// booleans and grapheme-break bits in the flag lanes, the coarse script tag in
/// bits 16..=23. One `class_of` read answers every per-char question a rule
/// asks. Only specific fields are queried by each consumer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Class(u32);

// Casing / lexical bits.
const ALPHA: u32 = 1 << 0;
const LOWER: u32 = 1 << 1;
const UPPER: u32 = 1 << 2;
const WHITESPACE: u32 = 1 << 3;
const NUMERIC: u32 = 1 << 4;
const DECIMAL: u32 = 1 << 5;
// bit 6 reserved — a future `clinging` flag (closing quotes/brackets).
const SENTENCE_TERMINAL: u32 = 1 << 7; // PropList Sentence_Terminal (STerm)

// Grapheme-break bits, consumed by `crate::grapheme` (ADR 0021).
const EXTENDER: u32 = 1 << 8; // GCB ∈ {Extend, SpacingMark, ZWJ}: glue to prev
const COMPLEX: u32 = 1 << 9; // needs the full UAX-#29 rules -> fallback
const INCB_CONSONANT: u32 = 1 << 10; // InCB=Consonant (GB9c conjunct base)
const INCB_LINKER: u32 = 1 << 11; // InCB=Linker (virama): arms a conjunct join
const INCB_MARK: u32 = 1 << 12; // InCB ∈ {Extend, Linker}: allowed in the gap

// General_Category-group bits (ADR 0022), backing `crate::unicode`'s predicates.
const MARK: u32 = 1 << 13; // group Mark (Mn/Mc/Me)
const PUNCT: u32 = 1 << 14; // group Punctuation (P*)
const SYMBOL: u32 = 1 << 15; // group Symbol (S*)

// Coarse script tag (ADR 0015/0022) packed into bits 16..=23: 0 = None,
// otherwise a `ScriptTag` discriminant (see `crate::script::from_repr`).
const SCRIPT_SHIFT: u32 = 16;
const SCRIPT_MASK: u32 = 0xFF << SCRIPT_SHIFT;

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
    /// UCD `Sentence_Terminal` (STerm): the marks that end sentences in
    /// their scripts — `.` `!` `?`, danda `।`, Ethiopic `።`, Arabic `؟ ۔`,
    /// Burmese `။`, CJK `。`, and kin. Deliberately *not*
    /// `Terminal_Punctuation`, which also holds commas and list separators.
    #[inline]
    pub fn is_sentence_terminal(self) -> bool {
        self.0 & SENTENCE_TERMINAL != 0
    }

    // General_Category-group queries (ADR 0022).
    #[inline]
    pub fn is_mark(self) -> bool {
        self.0 & MARK != 0
    }
    #[inline]
    pub fn is_punctuation(self) -> bool {
        self.0 & PUNCT != 0
    }
    #[inline]
    pub fn is_symbol(self) -> bool {
        self.0 & SYMBOL != 0
    }

    /// The coarse script tag, or `None` for `Common`/`Inherited`/untracked.
    #[inline]
    pub fn script(self) -> Option<ScriptTag> {
        script::from_repr(((self.0 & SCRIPT_MASK) >> SCRIPT_SHIFT) as u8)
    }

    // Grapheme-break queries — crate-internal, for the segmenter.
    #[inline]
    pub(crate) fn is_extender(self) -> bool {
        self.0 & EXTENDER != 0
    }
    #[inline]
    pub(crate) fn is_complex(self) -> bool {
        self.0 & COMPLEX != 0
    }
    #[inline]
    pub(crate) fn is_incb_consonant(self) -> bool {
        self.0 & INCB_CONSONANT != 0
    }
    #[inline]
    pub(crate) fn is_incb_linker(self) -> bool {
        self.0 & INCB_LINKER != 0
    }
    #[inline]
    pub(crate) fn is_incb_mark(self) -> bool {
        self.0 & INCB_MARK != 0
    }
}

/// The close glyph paired with `c` if `c` is a UCD paired-bracket opener
/// (BidiBrackets.txt). Binary search over the generated open-sorted table.
pub(crate) fn bracket_close_of(c: char) -> Option<char> {
    let cp = c as u32;
    crate::charclass_table::BRACKET_PAIRS
        .binary_search_by_key(&cp, |&(o, _)| o)
        .ok()
        .and_then(|i| char::from_u32(crate::charclass_table::BRACKET_PAIRS[i].1))
}

/// The open glyph paired with `c` if `c` is a UCD paired-bracket closer.
/// Linear over ~64 entries — callers gate on punctuation first, so this is
/// off the hot path.
pub(crate) fn bracket_open_of(c: char) -> Option<char> {
    let cp = c as u32;
    crate::charclass_table::BRACKET_PAIRS
        .iter()
        .find(|&&(_, cl)| cl == cp)
        .and_then(|&(o, _)| char::from_u32(o))
}

/// The fused table: a flat BMP array (one indexed read) plus the sorted astral
/// ranges (binary-searched by the vanishingly rare astral char). Built once
/// from [`CLASS_RANGES`].
struct Table {
    bmp: Box<[u32]>,              // len 0x10000
    astral: Vec<(u32, u32, u32)>, // sorted, non-overlapping
}

static TABLE: OnceLock<Table> = OnceLock::new();

fn table() -> &'static Table {
    TABLE.get_or_init(|| {
        let mut bmp = vec![0u32; 0x10000].into_boxed_slice();
        let mut astral: Vec<(u32, u32, u32)> = Vec::new();
        for &(lo, hi, bits) in CLASS_RANGES {
            let bmp_hi = hi.min(0xFFFF);
            if lo <= 0xFFFF {
                for cp in lo..=bmp_hi {
                    bmp[cp as usize] = bits;
                }
            }
            if hi >= 0x10000 {
                astral.push((lo.max(0x10000), hi, bits));
            }
        }
        Table { bmp, astral }
    })
}

/// The fused classification of `c` — one table read for the BMP (every char
/// our corpora use bar one emoji), a binary search over astral ranges otherwise.
#[inline]
pub fn class_of(c: char) -> Class {
    let cp = c as u32;
    let t = table();
    if cp < 0x10000 {
        Class(t.bmp[cp as usize])
    } else {
        let bits = t
            .astral
            .binary_search_by(|&(lo, hi, _)| {
                use std::cmp::Ordering::*;
                if cp < lo {
                    Greater
                } else if cp > hi {
                    Less
                } else {
                    Equal
                }
            })
            .map_or(0, |i| t.astral[i].2);
        Class(bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fused table must agree with the std predicates for a spread of
    /// scripts — letters, digits, combining marks, and astral.
    #[test]
    fn matches_std_predicates() {
        use unicode_properties::{
            GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory,
        };
        // Includes numeric-but-not-decimal chars (½ U+00BD No, Ⅷ U+2167 Nl,
        // ² U+00B2 No) so NUMERIC and DECIMAL are exercised independently, and
        // a spread of category/script cases.
        let sample = "Aa1 .;+$½Ⅷ²Ελληνικά देवनागरी ๗ไทย \u{0301}\u{0E48}𝐀🙏";
        for c in sample.chars() {
            let cl = class_of(c);
            assert_eq!(cl.is_alphabetic(), c.is_alphabetic(), "alpha {c:?}");
            assert_eq!(cl.is_lowercase(), c.is_lowercase(), "lower {c:?}");
            assert_eq!(cl.is_uppercase(), c.is_uppercase(), "upper {c:?}");
            assert_eq!(cl.is_whitespace(), c.is_whitespace(), "ws {c:?}");
            assert_eq!(cl.is_numeric(), c.is_numeric(), "numeric {c:?}");
            assert_eq!(
                cl.is_decimal_digit(),
                c.general_category() == GeneralCategory::DecimalNumber,
                "decimal {c:?}"
            );
            let g = c.general_category_group();
            assert_eq!(cl.is_mark(), g == GeneralCategoryGroup::Mark, "mark {c:?}");
            assert_eq!(
                cl.is_punctuation(),
                g == GeneralCategoryGroup::Punctuation,
                "punct {c:?}"
            );
            assert_eq!(cl.is_symbol(), g == GeneralCategoryGroup::Symbol, "symbol {c:?}");
            assert_eq!(cl.script(), crate::script::script_from_unicode(c), "script {c:?}");
        }
    }

    /// A math-alphanumeric letter (astral, U+1D400 𝐀) is upper+alphabetic —
    /// exercises the astral binary-search path.
    #[test]
    fn astral_letter_classifies() {
        let cl = class_of('𝐀');
        assert!(cl.is_alphabetic() && cl.is_uppercase() && !cl.is_lowercase());
    }

    /// A tone mark with no casing/lexical bits still reads all-false there,
    /// while carrying its grapheme-break bit (Thai MAI EK is an Extend).
    #[test]
    fn zero_lexical_still_has_grapheme_bits() {
        let cl = class_of('\u{0E48}');
        assert!(!cl.is_alphabetic() && !cl.is_numeric() && !cl.is_decimal_digit());
        assert!(cl.is_extender());
    }
}
