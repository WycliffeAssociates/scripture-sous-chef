//! Hygiene signals — things that are *never legitimate*, regardless of
//! corpus convention or language. No corpus statistics, no config knobs
//! beyond on/off. The bar is high: if there's any plausible language or
//! style where a pattern is fine, it belongs in a statistical signal (on
//! `labs`) that learns the corpus convention, not here.
//!
//! Each scan takes the verse `text` (onion's lossless projection — NOT a
//! normalised copy) and returns byte `Span`s into it. The runner stamps
//! `sid` + `code` + `severity`.

use std::collections::HashMap;

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::script::script_of;
use crate::span::Span;
use crate::unicode::{ZWJ, ZWNJ, is_c0_control, is_c1_control, is_zero_width_or_format};

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

/// Zero-width characters in scripts that don't legitimately use them.
/// ZWNJ/ZWJ are meaningful in many Indic and Arabic-family scripts and
/// are not flagged when the verse's majority script is one of those.
/// Other zero-width chars (BOM, RLM, LRM, the formatting-control range)
/// are flagged unconditionally.
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
    let allows_joiners = script_allows_joiners(majority_script(text));
    for (i, c) in text.char_indices() {
        if !is_zero_width_or_format(c) {
            continue;
        }
        if allows_joiners && (c == ZWNJ || c == ZWJ) {
            continue;
        }
        spans.push(Span {
            start: i,
            end: i + c.len_utf8(),
        });
    }
    spans
}

fn majority_script(s: &str) -> Option<&'static str> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for c in s.chars() {
        if let Some(name) = script_of(c) {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(n, _)| n)
}

fn script_allows_joiners(majority: Option<&'static str>) -> bool {
    matches!(
        majority,
        Some(
            "Devanagari"
                | "Bengali"
                | "Gurmukhi"
                | "Gujarati"
                | "Oriya"
                | "Tamil"
                | "Telugu"
                | "Kannada"
                | "Malayalam"
                | "Sinhala"
                | "Arabic"
                | "Myanmar"
                | "Thaana"
        )
    )
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
    fn zero_width_allows_zwnj_in_devanagari() {
        // एक (Devanagari) + ZWNJ + क
        assert!(scan_zero_width_misuse("एक\u{200C}क").is_empty());
    }

    #[test]
    fn zero_width_flags_zwnj_in_latin() {
        let f = scan_zero_width_misuse("fo\u{200C}o");
        assert_eq!(f.len(), 1);
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
}
