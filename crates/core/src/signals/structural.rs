//! Structural signals — markup that should never survive ingest into
//! plain verse text. onion strips USFM and the editor strips HTML; any
//! remnant here is an ingest bug upstream, not a translation issue, so
//! these are pure scans with no language sensitivity at all.

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;

/// Source-markup leftovers in verse text: USFM backslash markers
/// (`\v`, `\p`, `\f`, `\w` …), USFM attribute remnants (`|` pipes,
/// `^` carets), and raw `<…>` HTML/XML tags. The highest-value scan in
/// the deterministic batch — it catches ingest bugs.
pub const SOURCE_MARKER_LEFTOVER: RuleId = RuleId::SourceMarkerLeftover;

pub struct SourceMarkerLeftover;

impl PerVerseRule for SourceMarkerLeftover {
    fn id(&self) -> RuleId {
        SOURCE_MARKER_LEFTOVER
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_source_marker_leftover(text)
    }
}

pub fn scan_source_marker_leftover(text: &str) -> Vec<Span> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            // `\marker` / `\marker*` / `\+marker`: backslash + ASCII
            // letters (+ optional digits, closing `*`). A lone backslash
            // is flagged too — it has no place in scripture body.
            b'\\' => {
                let start = i;
                i += 1;
                if i < bytes.len() && bytes[i] == b'+' {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'*' {
                    i += 1;
                }
                spans.push(Span { start, end: i });
            }
            // Raw HTML/XML tag: `<` + tag-ish start + no spaces-only,
            // closed by `>` on the same verse. Requires the first char
            // after `<` (or `</`) to be an ASCII letter so prose like
            // "a < b" doesn't flag.
            b'<' => {
                let start = i;
                let mut j = i + 1;
                if j < bytes.len() && bytes[j] == b'/' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                    while j < bytes.len() && bytes[j] != b'>' {
                        j += 1;
                    }
                    if j < bytes.len() {
                        spans.push(Span { start, end: j + 1 });
                        i = j + 1;
                        continue;
                    }
                }
                i += 1;
            }
            // USFM attribute / special-text remnants.
            b'|' | b'^' => {
                spans.push(Span { start: i, end: i + 1 });
                i += 1;
            }
            _ => i += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slices<'a>(text: &'a str) -> Vec<&'a str> {
        scan_source_marker_leftover(text)
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn flags_usfm_markers() {
        assert_eq!(slices(r"In the \v 2 beginning"), vec![r"\v"]);
        assert_eq!(slices(r"word \f + \ft note \f* more"), vec![r"\f", r"\ft", r"\f*"]);
        assert_eq!(slices(r"a \+nd Lord\+nd* b"), vec![r"\+nd", r"\+nd*"]);
    }

    #[test]
    fn flags_lone_backslash() {
        assert_eq!(slices(r"a \ b"), vec![r"\"]);
    }

    #[test]
    fn flags_html_tags() {
        assert_eq!(slices("a <b>bold</b> word"), vec!["<b>", "</b>"]);
        assert_eq!(slices("line<br/>break"), vec!["<br/>"]);
    }

    #[test]
    fn prose_angle_brackets_are_clean() {
        assert!(slices("5 < 7 and 7 > 5").is_empty());
        // Unclosed tag-like start doesn't flag (no `>` in the verse).
        assert!(slices("a <unclosed forever").is_empty());
    }

    #[test]
    fn flags_attribute_remnants() {
        assert_eq!(slices("grace|strong=\"G5485\""), vec!["|"]);
        assert_eq!(slices("foo ^ bar"), vec!["^"]);
    }

    #[test]
    fn clean_verse_is_clean() {
        assert!(slices("In the beginning God created the heavens.").is_empty());
    }

    #[test]
    fn span_slices_the_marker() {
        let text = r"word \add said\add* here";
        let spans = scan_source_marker_leftover(text);
        assert_eq!(spans[0].slice(text), r"\add");
        assert_eq!(spans[1].slice(text), r"\add*");
    }
}
