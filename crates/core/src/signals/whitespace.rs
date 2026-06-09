//! Excess horizontal whitespace in content text.
//!
//! A run of 2+ horizontal whitespace characters (space/tab) inside verse
//! content. Ports the *semantics* of onion's
//! `scan_excess_content_whitespace` — including the sentence-boundary
//! protection (a double space after sentence-ending punctuation is a
//! legitimate spacing convention, not an error) — but returns the byte
//! `Span` of each offending run instead of a bool.
//!
//! Embedded-newline detection is deferred: newlines are absent from the
//! slice-1 vref projection and a line break isn't cleanly highlightable
//! (ADR 0010).

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;

pub const EXCESS_H_WHITESPACE: RuleId = RuleId::ExcessHWhitespace;

pub struct ExcessHWhitespace;

impl PerVerseRule for ExcessHWhitespace {
    fn id(&self) -> RuleId {
        EXCESS_H_WHITESPACE
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_excess_h_whitespace(text)
    }
}

/// Per-verse scan. ASCII-byte scan: all predicates (horizontal WS,
/// sentence-ending punctuation) are ASCII, and non-ASCII bytes are text
/// content, so byte offsets land on char boundaries.
pub fn scan_excess_h_whitespace(text: &str) -> Vec<Span> {
    let bytes = text.as_bytes();
    let is_hs = |b: u8| b == b' ' || b == b'\t';
    let is_sentence_end = |b: u8| matches!(b, b'.' | b'!' | b'?' | b':' | b';');

    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut saw_text = false;
    let mut last_nonws: Option<u8> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if is_hs(b) {
            let run_start = i;
            while i < bytes.len() && is_hs(bytes[i]) {
                i += 1;
            }
            // Only flag runs that follow real content (leading runs are
            // not content whitespace) and are not the legitimate spacing
            // that follows sentence-ending punctuation.
            if i - run_start >= 2 && saw_text && !last_nonws.is_some_and(is_sentence_end) {
                runs.push(Span {
                    start: run_start,
                    end: i,
                });
            }
        } else if b == b'\n' || b == b'\r' {
            // Newlines are absent from the slice-1 projection; if one
            // appears, treat it as a boundary but do not flag (deferred).
            i += 1;
        } else {
            saw_text = true;
            last_nonws = Some(b);
            i += 1;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_double_space_and_slices_the_run() {
        let text = "a  b";
        let runs = scan_excess_h_whitespace(text);
        assert_eq!(runs, vec![Span { start: 1, end: 3 }]);
        // The returned range slices exactly the whitespace run.
        assert_eq!(runs[0].slice(text), "  ");
    }

    #[test]
    fn single_space_is_clean() {
        assert!(scan_excess_h_whitespace("a b").is_empty());
    }

    #[test]
    fn protects_sentence_boundary_spacing() {
        // Double space after a period is a spacing convention, not error.
        assert!(scan_excess_h_whitespace("End.  Next").is_empty());
        // ... but a double space mid-clause is flagged.
        assert_eq!(
            scan_excess_h_whitespace("mid  clause"),
            vec![Span { start: 3, end: 5 }]
        );
    }

    #[test]
    fn leading_run_not_flagged() {
        assert!(scan_excess_h_whitespace("   a").is_empty());
    }

    #[test]
    fn tab_run_flagged() {
        let runs = scan_excess_h_whitespace("a\t\tb");
        assert_eq!(runs, vec![Span { start: 1, end: 3 }]);
    }

    #[test]
    fn multiple_runs_in_one_verse() {
        let runs = scan_excess_h_whitespace("a  b   c");
        assert_eq!(
            runs,
            vec![Span { start: 1, end: 3 }, Span { start: 4, end: 7 }]
        );
    }
}
