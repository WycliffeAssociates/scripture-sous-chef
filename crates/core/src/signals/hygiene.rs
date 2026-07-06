//! Hygiene signals — things that are *never legitimate*, regardless of
//! corpus convention or language. No corpus statistics, no config knobs
//! beyond on/off. The bar is high: if there's any plausible language or
//! style where a pattern is fine, it belongs in a statistical signal (on
//! `labs`) that learns the corpus convention, not here.
//!
//! Each scan takes the verse `text` (onion's lossless projection — NOT a
//! normalised copy) and returns byte `Span`s into it. The runner stamps
//! `sid` + `code` + `severity`.

use crate::diagnostics::{RuleId, Severity};
use crate::rule::{PerVerseRule, TokenRule};
use crate::token::Token;
use crate::script::{ScriptTag, script_of};
use crate::span::Span;
use crate::unicode::{
    ZWJ, ZWNJ, ZWSP, is_c0_control, is_c1_control, is_combining_mark, is_decimal_digit,
    is_invalid_text_codepoint, is_punctuation, is_symbol, is_zero_width_or_format,
};

// ─────────────────────────────────────────────────────────────────────
// Tab in body
// ─────────────────────────────────────────────────────────────────────

/// Literal tab character anywhere in verse body. USFM doesn't use tabs
/// and they're never the intent.
pub const TAB_IN_BODY: RuleId = RuleId::TabInBody;

pub struct TabInBody;

impl PerVerseRule for TabInBody {
    fn id(&self) -> RuleId {
        TAB_IN_BODY
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_tab_in_body(text)
    }
}

pub fn scan_tab_in_body(text: &str) -> Vec<Span> {
    text.match_indices('\t')
        .map(|(i, _)| Span {
            start: i,
            end: i + 1,
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────
// Control characters (C0 / C1)
// ─────────────────────────────────────────────────────────────────────

/// C0/C1 control characters inside verse body. Tab is excluded (handled
/// by `TAB_IN_BODY`); newline is excluded (a projection may legitimately
/// preserve line breaks).
pub const CONTROL_CHARS: RuleId = RuleId::ControlChars;

pub struct ControlChars;

impl PerVerseRule for ControlChars {
    fn id(&self) -> RuleId {
        CONTROL_CHARS
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_control_chars(text)
    }
}

pub fn scan_control_chars(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    for (i, c) in text.char_indices() {
        let flagged = (is_c0_control(c) && c != '\t' && c != '\n') || is_c1_control(c);
        if flagged {
            spans.push(Span {
                start: i,
                end: i + c.len_utf8(),
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Zero-width misuse
// ─────────────────────────────────────────────────────────────────────

/// Zero-width and bidi/format controls that don't belong in scripture body:
/// BOM, RLM, LRM, the bidi embeddings/overrides, the word joiner and the rest
/// of the formatting-control range are flagged unconditionally.
///
/// **The orthography-dependent zero-width characters are not judged here.**
/// U+200B ZERO WIDTH SPACE and the joiners U+200C ZWNJ / U+200D ZWJ are each
/// legitimate in some scripts and a slip in others; a fixed predicate cannot
/// tell a convention from an error. ZWSP's corpus-relative context surprise is
/// scored at `Severity::Info` by
/// [`uni.zero-width-space-anomaly`](crate::signals::zero_width_space); the
/// joiners are simply skipped for now, awaiting their own corpus-relative rule.
/// (They were previously flagged via a Latin-centric script allow-list, which
/// produced false-positive storms on legitimate Khmer/Indic joiner use — worse
/// than flagging nothing. A property-driven successor is future work.)
pub const ZERO_WIDTH_MISUSE: RuleId = RuleId::ZeroWidthMisuse;

pub struct ZeroWidthMisuse;

impl PerVerseRule for ZeroWidthMisuse {
    fn id(&self) -> RuleId {
        ZERO_WIDTH_MISUSE
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_zero_width_misuse(text)
    }
}

pub fn scan_zero_width_misuse(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    for (i, c) in text.char_indices() {
        if !is_zero_width_or_format(c) {
            continue;
        }
        // The orthography-dependent zero-width characters are never a
        // deterministic error: U+200B is scored corpus-relative by
        // `uni.zero-width-space-anomaly`, and the joiners U+200C/U+200D are
        // skipped entirely pending their own corpus-relative rule. Everything
        // else in the format range (BOM, bidi, word joiner, …) is flagged.
        if c == ZWSP || c == ZWNJ || c == ZWJ {
            continue;
        }
        spans.push(Span {
            start: i,
            end: i + c.len_utf8(),
        });
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Empty verse
// ─────────────────────────────────────────────────────────────────────

/// Verse text empty (or whitespace-only). Often legitimate (`<range>`
/// continuation, deliberately-elided verse), so severity is Info —
/// surfaced for confirmation, not flagged as wrong.
pub const EMPTY_VERSE: RuleId = RuleId::EmptyVerse;

pub struct EmptyVerse;

impl PerVerseRule for EmptyVerse {
    fn id(&self) -> RuleId {
        EMPTY_VERSE
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_empty_verse(text)
    }
}

pub fn scan_empty_verse(text: &str) -> Vec<Span> {
    if text.chars().all(|c| c.is_whitespace()) {
        // Span the whole (whitespace-only or empty) text.
        vec![Span {
            start: 0,
            end: text.len(),
        }]
    } else {
        Vec::new()
    }
}

// ─────────────────────────────────────────────────────────────────────
// Invalid codepoint
// ─────────────────────────────────────────────────────────────────────

/// Codepoints that can never validly appear in interchange text:
/// U+FFFD (decode failure), Unicode noncharacters, and the
/// U+FFF9..=U+FFFC special-format leftovers. Always corruption,
/// regardless of language or script — see [`is_invalid_text_codepoint`].
pub const INVALID_CODEPOINT: RuleId = RuleId::InvalidCodepoint;

pub struct InvalidCodepoint;

impl PerVerseRule for InvalidCodepoint {
    fn id(&self) -> RuleId {
        INVALID_CODEPOINT
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_invalid_codepoint(text)
    }
}

pub fn scan_invalid_codepoint(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    for (i, c) in text.char_indices() {
        if is_invalid_text_codepoint(c) {
            spans.push(Span {
                start: i,
                end: i + c.len_utf8(),
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Combining mark without base
// ─────────────────────────────────────────────────────────────────────

/// A combining mark with nothing to combine with: at verse start, or
/// directly after whitespace or punctuation. Always an encoding/editing
/// error — a mark's base was deleted out from under it.
pub const COMBINING_MARK_WITHOUT_BASE: RuleId = RuleId::CombiningMarkWithoutBase;

pub struct CombiningMarkWithoutBase;

impl PerVerseRule for CombiningMarkWithoutBase {
    fn id(&self) -> RuleId {
        COMBINING_MARK_WITHOUT_BASE
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_combining_mark_without_base(text)
    }
}

pub fn scan_combining_mark_without_base(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if is_combining_mark(c) {
            let baseless = match prev {
                None => true,
                Some(p) => p.is_whitespace() || is_punctuation(p) || is_symbol(p),
            };
            if baseless {
                spans.push(Span {
                    start: i,
                    end: i + c.len_utf8(),
                });
            }
        }
        prev = Some(c);
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Mixed script in token
// ─────────────────────────────────────────────────────────────────────

/// One token mixing two or more scripts (Latin+Cyrillic homoglyphs,
/// math-alphanumeric look-alikes). Common/Inherited characters carry no
/// script identity and never count. Catches paste/encoding errors that
/// render invisibly.
pub const MIXED_SCRIPT_IN_TOKEN: RuleId = RuleId::MixedScriptInToken;

pub struct MixedScriptInToken;

impl TokenRule for MixedScriptInToken {
    fn id(&self) -> RuleId {
        MIXED_SCRIPT_IN_TOKEN
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str, tokens: &[Token]) -> Vec<Span> {
        scan_mixed_script_in_token(text, tokens)
    }
}

pub fn scan_mixed_script_in_token(text: &str, tokens: &[Token]) -> Vec<Span> {
    let mut spans = Vec::new();
    for token in tokens {
        let mut first: Option<ScriptTag> = None;
        let mut mixed = false;
        for c in token.span.slice(text).chars() {
            let Some(tag) = script_of(c) else { continue };
            match first {
                None => first = Some(tag),
                Some(f) if f != tag => {
                    mixed = true;
                    break;
                }
                Some(_) => {}
            }
        }
        if mixed {
            spans.push(token.span);
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Mixed numeral systems
// ─────────────────────────────────────────────────────────────────────

/// A verse mixing decimal digits from two numeral systems (ASCII `7`
/// next to Devanagari `७`, …). The minority-system digit runs are
/// flagged; the majority system is taken as the verse's convention.
pub const MIXED_NUMERAL_SYSTEMS: RuleId = RuleId::MixedNumeralSystems;

pub struct MixedNumeralSystems;

impl PerVerseRule for MixedNumeralSystems {
    fn id(&self) -> RuleId {
        MIXED_NUMERAL_SYSTEMS
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_mixed_numeral_systems(text)
    }
}

/// Numeral-system identity of a decimal digit: the zero codepoint of its
/// contiguous Nd block (every Unicode decimal-digit block is a run of
/// ten starting at its zero).
fn numeral_system(c: char) -> Option<u32> {
    if !is_decimal_digit(c) {
        return None;
    }
    let v = c.to_digit(10).unwrap_or_else(|| {
        // Non-ASCII Nd: derive the digit value from the block offset is
        // impossible without the zero — but Rust's to_digit handles only
        // ASCII. Walk back to the block zero instead: Nd blocks are
        // aligned runs of ten, so the zero is the largest codepoint
        // `z <= c` where `(c as u32 - z) < 10` and `z` is Nd with the
        // nine following codepoints Nd. Simpler: scan back up to 9.
        let cu = c as u32;
        for back in 1..=9 {
            if let Some(z) = char::from_u32(cu - back)
                && !is_decimal_digit(z)
            {
                return back - 1;
            }
        }
        9
    });
    Some(c as u32 - v)
}

pub fn scan_mixed_numeral_systems(text: &str) -> Vec<Span> {
    use std::collections::HashMap;

    let mut counts: HashMap<u32, usize> = HashMap::new();
    for c in text.chars() {
        if let Some(sys) = numeral_system(c) {
            *counts.entry(sys).or_default() += 1;
        }
    }
    if counts.len() < 2 {
        return Vec::new();
    }
    // Majority system; deterministic tie-break on the lower zero point.
    let majority = counts
        .iter()
        .max_by_key(|&(&sys, &n)| (n, std::cmp::Reverse(sys)))
        .map(|(&sys, _)| sys)
        .unwrap();

    // Flag maximal runs of minority-system digits.
    let mut spans = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_end = 0usize;
    for (i, c) in text.char_indices() {
        let minority = numeral_system(c).is_some_and(|sys| sys != majority);
        if minority {
            if run_start.is_none() {
                run_start = Some(i);
            }
            run_end = i + c.len_utf8();
        } else if let Some(start) = run_start.take() {
            spans.push(Span { start, end: run_end });
        }
    }
    if let Some(start) = run_start {
        spans.push(Span { start, end: run_end });
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_flags_each_tab() {
        let f = scan_tab_in_body("foo\tbar\tbaz");
        assert_eq!(f, vec![Span { start: 3, end: 4 }, Span { start: 7, end: 8 }]);
    }

    #[test]
    fn tab_clean_verse_no_findings() {
        assert!(scan_tab_in_body("foo bar baz").is_empty());
    }

    #[test]
    fn control_chars_flags_c0_and_c1() {
        // U+0007 (BEL, C0), U+0085 (NEL, C1)
        let f = scan_control_chars("foo\u{0007}bar\u{0085}baz");
        assert_eq!(f.len(), 2);
        assert_eq!("foo\u{0007}bar\u{0085}baz"[f[0].start..f[0].end].chars().next(), Some('\u{0007}'));
    }

    #[test]
    fn control_chars_excludes_tab_and_newline() {
        assert!(scan_control_chars("foo\tbar\nbaz").is_empty());
    }

    #[test]
    fn zero_width_flags_bom_in_latin() {
        let f = scan_zero_width_misuse("foo\u{FEFF}bar");
        assert_eq!(f.len(), 1);
        assert_eq!("foo\u{FEFF}bar"[f[0].start..f[0].end].chars().next(), Some('\u{FEFF}'));
    }

    #[test]
    fn zero_width_no_longer_flags_joiners() {
        // ZWNJ (U+200C) and ZWJ (U+200D) are orthography-dependent — legitimate
        // in Indic/Arabic-family shaping and in emoji sequences, a slip in Latin.
        // Deterministic hygiene no longer judges them at all (the old Latin-centric
        // script allow-list is gone); a corpus-relative successor is future work.
        assert!(scan_zero_width_misuse("एक\u{200C}क").is_empty()); // Devanagari ZWNJ
        assert!(scan_zero_width_misuse("fo\u{200C}o").is_empty()); // Latin ZWNJ (was flagged)
        assert!(scan_zero_width_misuse("a\u{200D}b").is_empty()); // ZWJ
    }

    #[test]
    fn zero_width_no_longer_flags_zwsp() {
        // U+200B is orthography-dependent (Khmer/Lao/…), scored corpus-relative
        // by uni.zero-width-space-anomaly — deterministic hygiene stays silent
        // regardless of surrounding script.
        assert!(scan_zero_width_misuse("a\u{200B}b").is_empty());
        assert!(scan_zero_width_misuse("ក\u{200B}ខ").is_empty()); // Khmer
        assert!(scan_zero_width_misuse("\u{200B}").is_empty());
    }

    #[test]
    fn zero_width_still_flags_other_controls_beside_zwsp() {
        // A verse carrying ZWSP *and* a BOM, word joiner, and bidi override:
        // only the three genuine controls are flagged; the ZWSP is skipped.
        let f = scan_zero_width_misuse("a\u{200B}b\u{FEFF}c\u{2060}d\u{202E}e");
        assert_eq!(f.len(), 3);
        let text = "a\u{200B}b\u{FEFF}c\u{2060}d\u{202E}e";
        let flagged: Vec<char> = f.iter().map(|s| text[s.start..s.end].chars().next().unwrap()).collect();
        assert_eq!(flagged, vec!['\u{FEFF}', '\u{2060}', '\u{202E}']);
    }

    #[test]
    fn empty_verse_fires_on_empty() {
        assert_eq!(scan_empty_verse(""), vec![Span { start: 0, end: 0 }]);
    }

    #[test]
    fn empty_verse_fires_on_whitespace_only() {
        assert_eq!(scan_empty_verse("   \t  ").len(), 1);
    }

    #[test]
    fn empty_verse_quiet_on_real_content() {
        assert!(scan_empty_verse("hello").is_empty());
    }

    #[test]
    fn invalid_codepoint_flags_replacement_char() {
        let f = scan_invalid_codepoint("god\u{FFFD}created");
        assert_eq!(f.len(), 1);
        assert_eq!("god\u{FFFD}created"[f[0].start..f[0].end].chars().next(), Some('\u{FFFD}'));
    }

    #[test]
    fn invalid_codepoint_flags_noncharacters() {
        // U+FDD0 (Arabic-block noncharacter) and U+FFFE (plane-end pair).
        assert_eq!(scan_invalid_codepoint("a\u{FDD0}b").len(), 1);
        assert_eq!(scan_invalid_codepoint("a\u{FFFE}b").len(), 1);
        assert_eq!(scan_invalid_codepoint("a\u{FFFF}b").len(), 1);
        // Plane-end noncharacters in a higher plane (U+1FFFF).
        assert_eq!(scan_invalid_codepoint("a\u{1FFFF}b").len(), 1);
    }

    #[test]
    fn invalid_codepoint_flags_special_format_leftovers() {
        // U+FFFC object replacement, U+FFF9 interlinear-annotation anchor.
        assert_eq!(scan_invalid_codepoint("a\u{FFFC}b").len(), 1);
        assert_eq!(scan_invalid_codepoint("a\u{FFF9}b").len(), 1);
    }

    #[test]
    fn invalid_codepoint_clean_text_quiet() {
        assert!(scan_invalid_codepoint("In the beginning God created").is_empty());
        assert!(scan_invalid_codepoint("परमेश्वर ने कहा").is_empty());
    }

    #[test]
    fn invalid_codepoint_respects_range_edges() {
        // U+FDEF is the last noncharacter; U+FDF0 just past it is valid.
        let f = scan_invalid_codepoint("\u{FDEF}\u{FDF0}");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0], Span { start: 0, end: 3 });
    }

    #[test]
    fn combining_mark_after_space_flagged() {
        // "a ́b" — acute with only a space to attach to.
        let text = "a \u{0301}b";
        let f = scan_combining_mark_without_base(text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].slice(text), "\u{0301}");
    }

    #[test]
    fn combining_mark_at_start_and_after_punct_flagged() {
        assert_eq!(scan_combining_mark_without_base("\u{0301}abc").len(), 1);
        assert_eq!(scan_combining_mark_without_base("word.\u{0301} x").len(), 1);
    }

    #[test]
    fn combining_mark_on_base_is_clean() {
        assert!(scan_combining_mark_without_base("ne\u{0301}e").is_empty());
        // Devanagari matras on consonants.
        assert!(scan_combining_mark_without_base("परमेश्वर").is_empty());
    }

    /// Tokenize then scan — the runner now hands `scan_mixed_script_in_token`
    /// its tokens, so the tests share one tokenization too.
    fn mixed(text: &str) -> Vec<Span> {
        scan_mixed_script_in_token(text, &crate::token::tokenize(text))
    }

    #[test]
    fn mixed_script_homoglyph_flagged() {
        // Latin word with a Cyrillic 'а' in the middle.
        let text = "p\u{0430}ul said";
        let f = mixed(text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].slice(text), "p\u{0430}ul");
    }

    #[test]
    fn mixed_script_math_bold_flagged() {
        // U+1D400 MATHEMATICAL BOLD CAPITAL A inside a Latin token.
        let f = mixed("\u{1D400}men");
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn single_script_tokens_clean() {
        assert!(mixed("an ordinary verse").is_empty());
        assert!(mixed("परमेश्वर ने कहा").is_empty());
        // Digits/punct are Common — never count as a second script.
        assert!(mixed("40days a.m.").is_empty());
        // Two scripts in two separate tokens is fine (quotation, gloss).
        assert!(mixed("word शब्द").is_empty());
    }

    #[test]
    fn mixed_numerals_flag_minority_run() {
        // Two ASCII digits (majority), one Devanagari run (minority).
        let text = "12 men and ४५ women";
        let f = scan_mixed_numeral_systems(text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].slice(text), "४५");
    }

    #[test]
    fn single_numeral_system_clean() {
        assert!(scan_mixed_numeral_systems("12 men and 45 women").is_empty());
        assert!(scan_mixed_numeral_systems("१२ and ४५").is_empty());
        assert!(scan_mixed_numeral_systems("no digits at all").is_empty());
    }
}
