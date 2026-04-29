//! Per-verse precomputed views.
//!
//! Decision (locked): the expensive Unicode work — NFC normalisation,
//! ICU4X word segmentation, token classification — runs **once at
//! ingest**. A signal that needs a cheaper derived form (casefolded
//! token, word-only iterator, punctuation-stripped slice) computes it
//! per-token at use time; we don't cache those as separate strings.
//!
//! Why precompute and keep all verses in memory rather than stream
//! verse-by-verse: several rules (sentence-start capitalisation,
//! sentence-spanning duplicate-word, etc.) need to look across verse
//! boundaries — the previous verse's terminal punctuation determines
//! whether *this* verse should start with a capital. A streaming fold
//! would force every cross-verse rule to carry its own state machine.
//!
//! PERF NOTE: rough budget for a 5 MB Bible is ~5 MB `raw` + ~5 MB
//! `nfc` + ~30–50 MB tokens (24 B per token × ~1.5M tokens). Acceptable
//! for desktop and almost certainly fine for WASM; if it bites on a
//! constrained target, the lever is to drop `raw` (re-derivable from
//! ingest) and/or pack `Token` smaller, not to stream.

use std::collections::BTreeMap;

use icu_segmenter::options::WordBreakInvariantOptions;
use icu_segmenter::{WordSegmenter, WordSegmenterBorrowed};
use unicode_normalization::UnicodeNormalization;

use crate::sid::Sid;

/// A single verse plus every precomputed view a signal might want. Owns
/// its NFC string so spans and tokens can be `&str` slices into it.
#[derive(Debug, Clone)]
pub struct Verse {
    pub sid: Sid,

    /// Verse text exactly as ingested (post USFM-stripping, pre any
    /// Unicode work).
    pub raw: String,

    /// `raw` after NFC normalisation. Spans in `Finding` point into this
    /// string. Use this as the canonical text for any positional rule.
    pub nfc: String,

    /// ICU4X word-segmenter output over `nfc`, classified. Indices are
    /// byte offsets into `nfc`. Casefolding, punctuation filtering, and
    /// other cheap per-token transforms are the signal's job — those
    /// derivations are nanoseconds and not worth caching as full strings.
    pub tokens: Vec<Token>,
}

impl Verse {
    /// Slice `nfc` for a given token. Cheap, no allocation.
    pub fn token_text<'a>(&'a self, t: &Token) -> &'a str {
        &self.nfc[t.start..t.end]
    }

    /// Iterate token slices of a given kind, paired with the token record.
    pub fn tokens_of(&self, kind: TokenKind) -> impl Iterator<Item = (&Token, &str)> + '_ {
        self.tokens
            .iter()
            .filter(move |t| t.kind == kind)
            .map(|t| (t, &self.nfc[t.start..t.end]))
    }
}

/// One word-segmenter output unit. Kept as offsets (not borrowed slices)
/// so `Verse` is `Send + Sync` and self-contained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// Contains at least one alphanumeric / letter codepoint.
    Word,
    /// All ASCII digits (or other Number-class codepoints).
    Number,
    /// All punctuation / symbol codepoints.
    Punctuation,
    /// All whitespace.
    Whitespace,
    /// Anything else ICU4X emitted (control chars, format chars, etc.).
    Other,
}

/// Build a fully-populated `Verse` from raw ingested text. Convenience
/// for one-off use. For batch builds (a whole corpus at once), use
/// `build_verses` so the segmenter is constructed once instead of
/// per-verse — `new_auto` loads dictionaries for Thai/Lao/Khmer/Burmese
/// and an ML model for CJK, which has non-trivial init cost.
pub fn build_verse(sid: Sid, raw: String) -> Verse {
    let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    let nfc: String = raw.nfc().collect();
    let tokens = tokenise(&nfc, segmenter);
    Verse {
        sid,
        raw,
        nfc,
        tokens,
    }
}

/// Batch build with a shared segmenter.
pub fn build_verses(items: impl IntoIterator<Item = (Sid, String)>) -> BTreeMap<Sid, Verse> {
    let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    items
        .into_iter()
        .map(|(sid, raw)| {
            let nfc: String = raw.nfc().collect();
            let tokens = tokenise(&nfc, segmenter);
            (
                sid,
                Verse {
                    sid,
                    raw,
                    nfc,
                    tokens,
                },
            )
        })
        .collect()
}

fn tokenise(nfc: &str, segmenter: WordSegmenterBorrowed<'_>) -> Vec<Token> {
    let bounds: Vec<usize> = segmenter.segment_str(nfc).collect();
    let mut tokens = Vec::with_capacity(bounds.len().saturating_sub(1));
    for w in bounds.windows(2) {
        let (start, end) = (w[0], w[1]);
        if start >= end || end > nfc.len() {
            continue;
        }
        // ICU4X bounds are codepoint-aligned by contract, so direct
        // slicing is safe; the guard above is belt-and-braces.
        let seg = &nfc[start..end];
        tokens.push(Token {
            start,
            end,
            kind: classify(seg),
        });
    }
    tokens
}

fn classify(s: &str) -> TokenKind {
    if s.is_empty() {
        return TokenKind::Other;
    }
    let mut all_ws = true;
    let mut has_letter = false;
    let mut has_digit = false;
    let mut has_alnum_or_ws = false;
    for c in s.chars() {
        if !c.is_whitespace() {
            all_ws = false;
        }
        if c.is_alphabetic() {
            has_letter = true;
        }
        if c.is_numeric() {
            has_digit = true;
        }
        if c.is_alphanumeric() || c.is_whitespace() {
            has_alnum_or_ws = true;
        }
    }
    if all_ws {
        return TokenKind::Whitespace;
    }
    if has_letter {
        return TokenKind::Word;
    }
    if has_digit {
        return TokenKind::Number;
    }
    if !has_alnum_or_ws {
        return TokenKind::Punctuation;
    }
    TokenKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn sid() -> Sid {
        Sid::new(BookId::from_str("GEN").unwrap(), 1, 1)
    }

    #[test]
    fn nfc_normalises() {
        // "café" with combining acute (U+0065 U+0301) should become
        // precomposed (U+00E9) under NFC.
        let v = build_verse(sid(), "cafe\u{0301}".to_string());
        assert_eq!(v.nfc, "caf\u{00E9}");
    }

    #[test]
    fn classifies_basic_kinds() {
        let v = build_verse(sid(), "Hello, world 42!".to_string());
        let kinds: Vec<TokenKind> = v.tokens.iter().map(|t| t.kind).collect();
        // Expect at least one of each: Word, Punctuation, Whitespace, Number.
        assert!(kinds.contains(&TokenKind::Word));
        assert!(kinds.contains(&TokenKind::Punctuation));
        assert!(kinds.contains(&TokenKind::Whitespace));
        assert!(kinds.contains(&TokenKind::Number));
    }

    #[test]
    fn token_offsets_are_byte_correct() {
        let v = build_verse(sid(), "abc def".to_string());
        // First Word token must slice back to "abc".
        let first_word = v.tokens.iter().find(|t| t.kind == TokenKind::Word).unwrap();
        assert_eq!(v.token_text(first_word), "abc");
    }

    #[test]
    fn tab_survives_to_nfc() {
        // Hygiene rule needs to find this; make sure NFC doesn't eat it.
        let v = build_verse(sid(), "foo\tbar".to_string());
        assert!(v.nfc.contains('\t'));
    }
}
