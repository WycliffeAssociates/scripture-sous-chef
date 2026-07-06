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

use crate::charclass::class_of;
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

/// Horizontal whitespace: tab plus any `White_Space` char that isn't a line
/// break. `is_whitespace` comes from the fused table; the line-break scalars
/// (LF, VT, FF, CR, NEL, LS, PS) are carved out so a stray embedded newline
/// stays a boundary, not a flaggable run member.
fn is_h_whitespace(c: char) -> bool {
    if matches!(
        c,
        '\n' | '\u{000B}' | '\u{000C}' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    ) {
        return false;
    }
    c == '\t' || class_of(c).is_whitespace()
}

/// Per-verse scan, char-based: predicates are Unicode classes, and a run may
/// mix members (space + NBSP is still one doubled run).
pub fn scan_excess_h_whitespace(text: &str) -> Vec<Span> {
    let mut runs = Vec::new();
    let mut saw_text = false;
    let mut last_nonws: Option<char> = None;

    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if is_h_whitespace(c) {
            let run_start = i;
            let mut count = 1usize;
            let mut end = i + c.len_utf8();
            while let Some(&(j, next)) = iter.peek() {
                if !is_h_whitespace(next) {
                    break;
                }
                iter.next();
                count += 1;
                end = j + next.len_utf8();
            }
            // Only flag runs that follow real content (leading runs are
            // not content whitespace) and are not the legitimate spacing
            // that follows a sentence terminal.
            let protected = last_nonws.is_some_and(|p| class_of(p).is_sentence_terminal());
            if count >= 2 && saw_text && !protected {
                runs.push(Span { start: run_start, end });
            }
        } else if matches!(c, '\n' | '\r') {
            // Newlines are absent from the slice-1 projection; if one
            // appears, treat it as a boundary but do not flag (deferred).
        } else {
            saw_text = true;
            last_nonws = Some(c);
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
    fn protection_follows_sentence_terminal_not_ascii() {
        // The same double-space courtesy after danda, Ethiopic full stop,
        // Arabic full stop, and Burmese section mark.
        for terminal in ["।", "።", "۔", "။", "؟"] {
            let text = format!("word{terminal}  Next");
            assert!(
                scan_excess_h_whitespace(&text).is_empty(),
                "double space after {terminal} is the sentence-spacing convention"
            );
        }
        // A comma is not a sentence terminal — anywhere.
        assert_eq!(scan_excess_h_whitespace("a,  b").len(), 1);
        assert_eq!(scan_excess_h_whitespace("क,  ख").len(), 1);
    }

    #[test]
    fn non_ascii_whitespace_runs_flag() {
        // Doubled NBSP, and a space+NBSP mix, are excess whitespace the old
        // byte scan couldn't see.
        assert_eq!(scan_excess_h_whitespace("a\u{00A0}\u{00A0}b").len(), 1);
        let text = "a \u{00A0}b";
        let runs = scan_excess_h_whitespace(text);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].slice(text), " \u{00A0}");
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
