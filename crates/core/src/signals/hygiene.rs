//! Hygiene signals — things that are *never legitimate*, regardless of
//! corpus convention or language. No corpus statistics, no config knobs
//! beyond on/off. The bar is high: if there's any plausible language or
//! style where a pattern is fine, it belongs in a statistical signal (on
//! `labs`) that learns the corpus convention, not here.
//!
//! Each scan takes the verse `text` (onion's lossless projection — NOT a
//! normalised copy) and returns byte `Span`s into it. The runner stamps
//! `sid` + `code` + `severity`.

use crate::charclass::Class;
use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::tape::{Mask, TapeEntry};
use crate::unicode::{
    ZWJ, ZWNJ, ZWSP, is_c0_control, is_c1_control, is_invalid_text_codepoint,
    is_zero_width_or_format, numeral_system,
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
    fn check(&self, text: &str, _tape: &[TapeEntry]) -> Vec<Span> {
        scan_tab_in_body(text)
    }
    fn gate(&self) -> Mask {
        Mask::TAB
    }
}

pub fn scan_tab_in_body(text: &str) -> Vec<Span> {
    text.match_indices('\t')
        .map(|(i, _)| Span {
            start: i as u32,
            end: i as u32 + 1,
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
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_control_chars(tape)
    }
    fn gate(&self) -> Mask {
        Mask::CONTROL
    }
}

pub(crate) fn scan_control_chars(tape: &[TapeEntry]) -> Vec<Span> {
    // One finding per maximal run of the *same* control char: damaged files
    // carry padding runs (NUL×40 at a verse end), and per-char findings turn
    // one damaged verse into dozens of rows without adding information.
    let mut spans: Vec<Span> = Vec::new();
    let mut run: Option<(char, Span)> = None;
    for e in tape {
        let (i, c) = (e.off, e.ch);
        let flagged = (is_c0_control(c) && c != '\t' && c != '\n') || is_c1_control(c);
        if flagged {
            match &mut run {
                Some((rc, span)) if *rc == c && span.end == i => span.end = i + c.len_utf8() as u32,
                _ => {
                    if let Some((_, span)) = run.take() {
                        spans.push(span);
                    }
                    run = Some((
                        c,
                        Span {
                            start: i,
                            end: i + c.len_utf8() as u32,
                        },
                    ));
                }
            }
        } else if let Some((_, span)) = run.take() {
            spans.push(span);
        }
    }
    if let Some((_, span)) = run {
        spans.push(span);
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
/// tell a convention from an error. A *doubled* U+200B run (line-break redundant)
/// is flagged separately by
/// [`uni.redundant-zero-width-space`](crate::signals::zero_width_space) at Info;
/// the joiners are simply skipped for now, awaiting their own corpus-relative
/// rule. (They were previously flagged via a Latin-centric script allow-list,
/// which produced false-positive storms on legitimate Khmer/Indic joiner use —
/// worse than flagging nothing. A property-driven successor is future work.)
pub const ZERO_WIDTH_MISUSE: RuleId = RuleId::ZeroWidthMisuse;

pub struct ZeroWidthMisuse;

impl PerVerseRule for ZeroWidthMisuse {
    fn id(&self) -> RuleId {
        ZERO_WIDTH_MISUSE
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_zero_width_misuse(tape)
    }
    fn gate(&self) -> Mask {
        Mask::ZW_FORMAT
    }
}

pub(crate) fn scan_zero_width_misuse(tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();
    for e in tape {
        let (i, c) = (e.off, e.ch);
        if !is_zero_width_or_format(c) {
            continue;
        }
        // The orthography-dependent zero-width characters are never a
        // deterministic error here: U+200B's redundant placements are flagged by
        // `uni.redundant-zero-width-space` instead, and the joiners U+200C/U+200D
        // are skipped entirely pending their own corpus-relative rule. Everything
        // else in the format range (BOM, bidi, word joiner, …) is flagged.
        if c == ZWSP || c == ZWNJ || c == ZWJ {
            continue;
        }
        spans.push(Span {
            start: i,
            end: i + c.len_utf8() as u32,
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
    fn check(&self, text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_empty_verse(text, tape)
    }
    fn gate(&self) -> Mask {
        // Fires on the *absence* of content; the mask sets NO_CONTENT when no
        // non-whitespace scalar was seen (empty or whitespace-only verse).
        Mask::NO_CONTENT
    }
}

pub(crate) fn scan_empty_verse(text: &str, tape: &[TapeEntry]) -> Vec<Span> {
    if tape.iter().all(|e| e.cl.is_whitespace()) {
        // Span the whole (whitespace-only or empty) text.
        vec![Span {
            start: 0,
            end: text.len() as u32,
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
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_invalid_codepoint(tape)
    }
    fn gate(&self) -> Mask {
        Mask::INVALID
    }
}

pub(crate) fn scan_invalid_codepoint(tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();
    for e in tape {
        if is_invalid_text_codepoint(e.ch) {
            spans.push(Span {
                start: e.off,
                end: e.off + e.ch.len_utf8() as u32,
            });
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Replacement run
// ─────────────────────────────────────────────────────────────────────

/// A run of three or more ASCII `?` — the classic substitution glyph a lossy
/// legacy-encoding conversion leaves where whole words died. U+FFFD is the
/// modern equivalent and belongs to `hyg.invalid-codepoint`; a `?`-run is
/// valid Unicode, so only its shape gives it away. Corpus recurrence must
/// never excuse it (destroyed text recurs like a convention — my_juds carries
/// ~1,000 such chunks), which is why this is deterministic hygiene and the
/// corpus-relative punctuation rules exclude the pattern from candidacy
/// rather than each half-owning it. Real `??`/`???` rhetoric exists at run
/// length 2 and is left to `punct.adjacency-anomaly`'s statistics; genuine
/// triple question marks in scripture body text are not an attested
/// convention in any surveyed corpus.
pub const REPLACEMENT_RUN: RuleId = RuleId::ReplacementRun;

pub struct ReplacementRun;

impl PerVerseRule for ReplacementRun {
    fn id(&self) -> RuleId {
        REPLACEMENT_RUN
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str, _tape: &[TapeEntry]) -> Vec<Span> {
        scan_replacement_run(text)
    }
    fn gate(&self) -> Mask {
        Mask::QRUN
    }
}

pub fn scan_replacement_run(text: &str) -> Vec<Span> {
    const MIN_RUN: usize = 3;
    let mut spans = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'?' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'?' {
                i += 1;
            }
            if i - start >= MIN_RUN {
                spans.push(Span {
                    start: start as u32,
                    end: i as u32,
                });
            }
        } else {
            i += 1;
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
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_combining_mark_without_base(tape)
    }
    fn gate(&self) -> Mask {
        Mask::MARK_BASELESS
    }
}

pub(crate) fn scan_combining_mark_without_base(tape: &[TapeEntry]) -> Vec<Span> {
    let mut spans = Vec::new();
    // The previous scalar's class — a mark is baseless at verse start, or when
    // the scalar before it is whitespace, punctuation, or a symbol (the fused
    // bits equal the std/GC predicates used before; ADR 0045).
    let mut prev: Option<Class> = None;
    for e in tape {
        if e.cl.is_mark() {
            let baseless = match prev {
                None => true,
                Some(p) => p.is_whitespace() || p.is_punctuation() || p.is_symbol(),
            };
            if baseless {
                spans.push(Span {
                    start: e.off,
                    end: e.off + e.ch.len_utf8() as u32,
                });
            }
        }
        prev = Some(e.cl);
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Mixed script in token
// ─────────────────────────────────────────────────────────────────────

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
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_mixed_numeral_systems(tape)
    }
    fn gate(&self) -> Mask {
        Mask::MULTI_NUMSYS
    }
}

// Iteration-order audit (FxHashMap swap, 2026-07-22): `counts` is read only
// via `.len()` (order-independent) and `.iter().max_by_key(...)`, whose key
// `(n, Reverse(sys))` is a strict total order — `sys` is the map key so no
// two entries can tie on the full key, meaning `max_by_key` always returns
// the same unique winner regardless of iteration order. The span-emitting
// loop below walks `tape` directly, not `counts`, so hasher choice never
// reaches the emitted `Vec<Span>`.
pub(crate) fn scan_mixed_numeral_systems(tape: &[TapeEntry]) -> Vec<Span> {
    use rustc_hash::FxHashMap;

    let mut counts: FxHashMap<u32, usize> = FxHashMap::default();
    for e in tape {
        // The tape's decimal-digit bit gates the block-zero derivation, so
        // `numeral_system` runs only on actual digits.
        if e.cl.is_decimal_digit()
            && let Some(sys) = numeral_system(e.ch)
        {
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
    let mut run_start: Option<u32> = None;
    let mut run_end = 0u32;
    for e in tape {
        let minority =
            e.cl.is_decimal_digit() && numeral_system(e.ch).is_some_and(|sys| sys != majority);
        if minority {
            if run_start.is_none() {
                run_start = Some(e.off);
            }
            run_end = e.off + e.ch.len_utf8() as u32;
        } else if let Some(start) = run_start.take() {
            spans.push(Span {
                start,
                end: run_end,
            });
        }
    }
    if let Some(start) = run_start {
        spans.push(Span {
            start,
            end: run_end,
        });
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The verse tape the runner would hand a per-verse scan.
    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }

    #[test]
    fn tab_flags_each_tab() {
        let f = scan_tab_in_body("foo\tbar\tbaz");
        assert_eq!(
            f,
            vec![Span { start: 3, end: 4 }, Span { start: 7, end: 8 }]
        );
    }

    #[test]
    fn tab_clean_verse_no_findings() {
        assert!(scan_tab_in_body("foo bar baz").is_empty());
    }

    #[test]
    fn control_chars_flags_c0_and_c1() {
        // U+0007 (BEL, C0), U+0085 (NEL, C1)
        let f = scan_control_chars(&tp("foo\u{0007}bar\u{0085}baz"));
        assert_eq!(f.len(), 2);
        assert_eq!(
            "foo\u{0007}bar\u{0085}baz"[f[0].start as usize..f[0].end as usize]
                .chars()
                .next(),
            Some('\u{0007}')
        );
    }

    #[test]
    fn control_chars_excludes_tab_and_newline() {
        assert!(scan_control_chars(&tp("foo\tbar\nbaz")).is_empty());
    }

    #[test]
    fn control_char_run_is_one_finding() {
        // NUL padding at a damaged verse end: one finding for the run, not
        // one per char. A different control char breaks the run; a gap does
        // too.
        let text = "word\u{0}\u{0}\u{0}\u{0}";
        let f = scan_control_chars(&tp(text));
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].start, f[0].end), (4, 8));
        let f = scan_control_chars(&tp("a\u{0}\u{0}\u{7}\u{7}b\u{0}c"));
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn replacement_run_flags_three_plus_question_marks() {
        let f = scan_replacement_run("word ????? word ??? end");
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].start, f[0].end), (5, 10));
        // Mid-word and punctuation-adjacent runs flag too — damage doesn't
        // respect token boundaries.
        assert_eq!(scan_replacement_run("wo???rd").len(), 1);
    }

    #[test]
    fn replacement_run_leaves_short_and_real_questions_alone() {
        // `?` and `??` are real interrogatives (or adjacency's business).
        assert!(scan_replacement_run("what? really?? sure").is_empty());
        // Non-ASCII question marks are not the lossy-conversion glyph.
        assert!(scan_replacement_run("؟؟؟").is_empty());
    }

    #[test]
    fn zero_width_flags_bom_in_latin() {
        let f = scan_zero_width_misuse(&tp("foo\u{FEFF}bar"));
        assert_eq!(f.len(), 1);
        assert_eq!(
            "foo\u{FEFF}bar"[f[0].start as usize..f[0].end as usize]
                .chars()
                .next(),
            Some('\u{FEFF}')
        );
    }

    #[test]
    fn zero_width_no_longer_flags_joiners() {
        // ZWNJ (U+200C) and ZWJ (U+200D) are orthography-dependent — legitimate
        // in Indic/Arabic-family shaping and in emoji sequences, a slip in Latin.
        // Deterministic hygiene no longer judges them at all (the old Latin-centric
        // script allow-list is gone); a corpus-relative successor is future work.
        assert!(scan_zero_width_misuse(&tp("एक\u{200C}क")).is_empty()); // Devanagari ZWNJ
        assert!(scan_zero_width_misuse(&tp("fo\u{200C}o")).is_empty()); // Latin ZWNJ (was flagged)
        assert!(scan_zero_width_misuse(&tp("a\u{200D}b")).is_empty()); // ZWJ
    }

    #[test]
    fn zero_width_no_longer_flags_zwsp() {
        // U+200B is orthography-dependent (Khmer/Lao/…); deterministic hygiene
        // stays silent on it regardless of surrounding script (its redundant
        // placements are handled by uni.redundant-zero-width-space instead).
        assert!(scan_zero_width_misuse(&tp("a\u{200B}b")).is_empty());
        assert!(scan_zero_width_misuse(&tp("ក\u{200B}ខ")).is_empty()); // Khmer
        assert!(scan_zero_width_misuse(&tp("\u{200B}")).is_empty());
    }

    #[test]
    fn zero_width_still_flags_other_controls_beside_zwsp() {
        // A verse carrying ZWSP *and* a BOM, word joiner, and bidi override:
        // only the three genuine controls are flagged; the ZWSP is skipped.
        let f = scan_zero_width_misuse(&tp("a\u{200B}b\u{FEFF}c\u{2060}d\u{202E}e"));
        assert_eq!(f.len(), 3);
        let text = "a\u{200B}b\u{FEFF}c\u{2060}d\u{202E}e";
        let flagged: Vec<char> = f
            .iter()
            .map(|s| {
                text[s.start as usize..s.end as usize]
                    .chars()
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(flagged, vec!['\u{FEFF}', '\u{2060}', '\u{202E}']);
    }

    #[test]
    fn empty_verse_fires_on_empty() {
        assert_eq!(
            scan_empty_verse("", &tp("")),
            vec![Span { start: 0, end: 0 }]
        );
    }

    #[test]
    fn empty_verse_fires_on_whitespace_only() {
        assert_eq!(scan_empty_verse("   \t  ", &tp("   \t  ")).len(), 1);
    }

    #[test]
    fn empty_verse_quiet_on_real_content() {
        assert!(scan_empty_verse("hello", &tp("hello")).is_empty());
    }

    #[test]
    fn invalid_codepoint_flags_replacement_char() {
        let f = scan_invalid_codepoint(&tp("god\u{FFFD}created"));
        assert_eq!(f.len(), 1);
        assert_eq!(
            "god\u{FFFD}created"[f[0].start as usize..f[0].end as usize]
                .chars()
                .next(),
            Some('\u{FFFD}')
        );
    }

    #[test]
    fn invalid_codepoint_flags_noncharacters() {
        // U+FDD0 (Arabic-block noncharacter) and U+FFFE (plane-end pair).
        assert_eq!(scan_invalid_codepoint(&tp("a\u{FDD0}b")).len(), 1);
        assert_eq!(scan_invalid_codepoint(&tp("a\u{FFFE}b")).len(), 1);
        assert_eq!(scan_invalid_codepoint(&tp("a\u{FFFF}b")).len(), 1);
        // Plane-end noncharacters in a higher plane (U+1FFFF).
        assert_eq!(scan_invalid_codepoint(&tp("a\u{1FFFF}b")).len(), 1);
    }

    #[test]
    fn invalid_codepoint_flags_special_format_leftovers() {
        // U+FFFC object replacement, U+FFF9 interlinear-annotation anchor.
        assert_eq!(scan_invalid_codepoint(&tp("a\u{FFFC}b")).len(), 1);
        assert_eq!(scan_invalid_codepoint(&tp("a\u{FFF9}b")).len(), 1);
    }

    #[test]
    fn invalid_codepoint_clean_text_quiet() {
        assert!(scan_invalid_codepoint(&tp("In the beginning God created")).is_empty());
        assert!(scan_invalid_codepoint(&tp("परमेश्वर ने कहा")).is_empty());
    }

    #[test]
    fn invalid_codepoint_respects_range_edges() {
        // U+FDEF is the last noncharacter; U+FDF0 just past it is valid.
        let f = scan_invalid_codepoint(&tp("\u{FDEF}\u{FDF0}"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0], Span { start: 0, end: 3 });
    }

    #[test]
    fn combining_mark_after_space_flagged() {
        // "a ́b" — acute with only a space to attach to.
        let text = "a \u{0301}b";
        let f = scan_combining_mark_without_base(&tp(text));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].slice(text), "\u{0301}");
    }

    #[test]
    fn combining_mark_at_start_and_after_punct_flagged() {
        assert_eq!(
            scan_combining_mark_without_base(&tp("\u{0301}abc")).len(),
            1
        );
        assert_eq!(
            scan_combining_mark_without_base(&tp("word.\u{0301} x")).len(),
            1
        );
    }

    #[test]
    fn combining_mark_on_base_is_clean() {
        assert!(scan_combining_mark_without_base(&tp("ne\u{0301}e")).is_empty());
        // Devanagari matras on consonants.
        assert!(scan_combining_mark_without_base(&tp("परमेश्वर")).is_empty());
    }

    #[test]
    fn mixed_numerals_flag_minority_run() {
        // Two ASCII digits (majority), one Devanagari run (minority).
        let text = "12 men and ४५ women";
        let f = scan_mixed_numeral_systems(&tp(text));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].slice(text), "४५");
    }

    #[test]
    fn single_numeral_system_clean() {
        assert!(scan_mixed_numeral_systems(&tp("12 men and 45 women")).is_empty());
        assert!(scan_mixed_numeral_systems(&tp("१२ and ४५")).is_empty());
        assert!(scan_mixed_numeral_systems(&tp("no digits at all")).is_empty());
    }
}
