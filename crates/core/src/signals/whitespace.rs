//! Excess horizontal whitespace in content text.
//!
//! A run of 2+ horizontal whitespace characters inside verse content. Ports
//! the *semantics* of onion's `scan_excess_content_whitespace` — including
//! the sentence-boundary protection (a double space after sentence-ending
//! punctuation is a legitimate spacing convention, not an error) — but
//! returns the byte `Span` of each offending run instead of a bool.
//!
//! Both predicates are Unicode classes read from the fused table, not ASCII
//! lists: horizontal whitespace is `Zs` + tab (so a doubled NBSP or an
//! NBSP+space pair — common paste artifacts — flag like doubled spaces), and
//! the protection set is UCD `Sentence_Terminal`, so a corpus double-spacing
//! after danda `।`, Ethiopic `።`, Arabic `۔`, or Burmese `။` gets the same
//! courtesy English gets after `.` — the protection follows the property,
//! not the Latin keyboard.
//!
//! Embedded-newline detection is deferred: newlines are absent from the
//! slice-1 vref projection and a line break isn't cleanly highlightable
//! (ADR 0010).

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::tape::{Mask, TapeEntry};

pub const EXCESS_H_WHITESPACE: RuleId = RuleId::ExcessHWhitespace;

pub struct ExcessHWhitespace;

impl PerVerseRule for ExcessHWhitespace {
    fn id(&self) -> RuleId {
        EXCESS_H_WHITESPACE
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, _text: &str, tape: &[TapeEntry]) -> Vec<Span> {
        scan_excess_h_whitespace(tape)
    }
    fn gate(&self) -> Mask {
        Mask::EXCESS_WS
    }
}

/// Horizontal whitespace: tab plus any `White_Space` char that isn't a line
/// break. `is_whitespace` comes from the fused table (via the tape entry's
/// class); the line-break scalars (LF, VT, FF, CR, NEL, LS, PS) are carved out
/// so a stray embedded newline stays a boundary, not a flaggable run member.
fn is_h_whitespace(e: &TapeEntry) -> bool {
    if matches!(
        e.ch,
        '\n' | '\u{000B}' | '\u{000C}' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    ) {
        return false;
    }
    e.ch == '\t' || e.cl.is_whitespace()
}

/// Per-verse scan over the scalar tape: predicates are Unicode classes, and a
/// run may mix members (space + NBSP is still one doubled run).
pub(crate) fn scan_excess_h_whitespace(tape: &[TapeEntry]) -> Vec<Span> {
    let mut runs = Vec::new();
    let mut saw_text = false;
    // Whether the last non-whitespace scalar was a sentence terminal (the
    // spacing after which is a legitimate convention, not an error).
    let mut last_was_terminal = false;

    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        if is_h_whitespace(&e) {
            let run_start = e.off as usize;
            let mut count = 1usize;
            let mut end = e.off as usize + e.ch.len_utf8();
            let mut j = i + 1;
            while j < tape.len() && is_h_whitespace(&tape[j]) {
                count += 1;
                end = tape[j].off as usize + tape[j].ch.len_utf8();
                j += 1;
            }
            // Only flag runs that follow real content (leading runs are
            // not content whitespace) and are not the legitimate spacing
            // that follows a sentence terminal.
            if count >= 2 && saw_text && !last_was_terminal {
                runs.push(Span { start: run_start, end });
            }
            i = j;
        } else if matches!(e.ch, '\n' | '\r') {
            // Newlines are absent from the slice-1 projection; if one
            // appears, treat it as a boundary but do not flag (deferred).
            i += 1;
        } else {
            saw_text = true;
            last_was_terminal = e.cl.is_sentence_terminal();
            i += 1;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the verse tape the runner would hand the scan.
    fn scan(text: &str) -> Vec<Span> {
        let mut tape = Vec::new();
        crate::tape::build(text, &mut tape);
        scan_excess_h_whitespace(&tape)
    }

    #[test]
    fn flags_double_space_and_slices_the_run() {
        let text = "a  b";
        let runs = scan(text);
        assert_eq!(runs, vec![Span { start: 1, end: 3 }]);
        // The returned range slices exactly the whitespace run.
        assert_eq!(runs[0].slice(text), "  ");
    }

    #[test]
    fn single_space_is_clean() {
        assert!(scan("a b").is_empty());
    }

    #[test]
    fn protects_sentence_boundary_spacing() {
        // Double space after a period is a spacing convention, not error.
        assert!(scan("End.  Next").is_empty());
        // ... but a double space mid-clause is flagged.
        assert_eq!(
            scan("mid  clause"),
            vec![Span { start: 3, end: 5 }]
        );
    }

    #[test]
    fn protection_follows_sentence_terminal_not_ascii() {
        // The same double-space courtesy after danda, Ethiopic full stop,
        // Arabic full stop, and Burmese section mark.
        for terminal in ["।", "።", "۔", "။", "؟"] {
            let text = format!("word{terminal}  Next");
            assert!(
                scan(&text).is_empty(),
                "double space after {terminal} is the sentence-spacing convention"
            );
        }
        // A comma is not a sentence terminal — anywhere.
        assert_eq!(scan("a,  b").len(), 1);
        assert_eq!(scan("क,  ख").len(), 1);
    }

    #[test]
    fn non_ascii_whitespace_runs_flag() {
        // Doubled NBSP, and a space+NBSP mix, are excess whitespace the old
        // byte scan couldn't see.
        assert_eq!(scan("a\u{00A0}\u{00A0}b").len(), 1);
        let text = "a \u{00A0}b";
        let runs = scan(text);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slice(text), " \u{00A0}");
    }

    #[test]
    fn leading_run_not_flagged() {
        assert!(scan("   a").is_empty());
    }

    #[test]
    fn tab_run_flagged() {
        let runs = scan("a\t\tb");
        assert_eq!(runs, vec![Span { start: 1, end: 3 }]);
    }

    #[test]
    fn multiple_runs_in_one_verse() {
        let runs = scan("a  b   c");
        assert_eq!(
            runs,
            vec![Span { start: 1, end: 3 }, Span { start: 4, end: 7 }]
        );
    }
}
