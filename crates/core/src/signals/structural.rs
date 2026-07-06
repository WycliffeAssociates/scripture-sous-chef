//! Structural signals — markup that should never survive ingest into
//! plain verse text. onion strips USFM and the editor strips HTML; any
//! remnant here is an ingest bug upstream, not a translation issue, so
//! these are pure scans with no language sensitivity at all.

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;

/// Source-markup leftovers in verse text: USFM backslash markers
/// (`\v`, `\p`, `\f`, `\w` …) and raw `<…>` HTML/XML tags. The
/// highest-value scan in the deterministic batch — it catches ingest bugs.
///
/// We deliberately do *not* flag bare `|` or `^`. USFM's text grammar
/// (`([^\\]|\\[/~\\|])+`, "simple text up to the next marker") treats every
/// non-backslash byte — pipes and carets included — as legitimate content;
/// only the backslash is special. A surviving `|`/`^` would at most signal a
/// buggy USFM *parser* upstream, which is that parser's job to fix, not a
/// property of the translation. A caret wedged mid-word (`b^bê`) is real, but
/// it's a punctuation-usage anomaly best surfaced statistically
/// ([`crate::signals::punctuation`]), not a deterministic markup scan.
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
            _ => i += 1,
        }
    }
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Merge conflict markers
// ─────────────────────────────────────────────────────────────────────

/// Git merge-conflict markers committed into verse text: a run of three
/// or more `<`, `=`, `>`, or `|` — the heads of `<<<<<<< HEAD`, `=======`,
/// `>>>>>>> branch`, and the diff3 base marker `||||||| merged common
/// ancestors`. A resolved merge never leaves these; their presence means a
/// conflict was saved unresolved. We deliberately *don't* match git's exact
/// seven-char, line-anchored form: a non-default `conflict-marker-size`, a
/// truncated paste, or a projection that collapsed the marker's newlines
/// would all slip past it. No scripture body legitimately repeats one of
/// these characters three times, so the low bar costs nothing in false
/// positives. The pipe run lives here rather than in
/// [`SOURCE_MARKER_LEFTOVER`] because a bare `|` is legitimate USFM text —
/// only a *run* of them is conflict evidence. Language-blind: the run is
/// ASCII punctuation, never script.
pub const MERGE_CONFLICT_MARKER: RuleId = RuleId::MergeConflictMarker;

pub struct MergeConflictMarker;

impl PerVerseRule for MergeConflictMarker {
    fn id(&self) -> RuleId {
        MERGE_CONFLICT_MARKER
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_merge_conflict_marker(text)
    }
}

pub fn scan_merge_conflict_marker(text: &str) -> Vec<Span> {
    /// Shortest run no legitimate scripture text would contain.
    const MIN_RUN: usize = 3;
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if matches!(c, b'<' | b'=' | b'>' | b'|') {
            let start = i;
            while i < bytes.len() && bytes[i] == c {
                i += 1;
            }
            if i - start >= MIN_RUN {
                spans.push(Span { start, end: i });
            }
        } else {
            i += 1;
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slices(text: &str) -> Vec<&str> {
        scan_source_marker_leftover(text)
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    fn conflict_slices(text: &str) -> Vec<&str> {
        scan_merge_conflict_marker(text)
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
    fn bare_pipe_and_caret_are_clean() {
        // USFM text grammar treats non-backslash bytes as legitimate content.
        // A surviving `|`/`^` is at most an upstream parser bug, not a
        // translation signal, so this rule stays out of it.
        assert!(slices("grace|strong=\"G5485\"").is_empty());
        assert!(slices("foo ^ bar").is_empty());
        assert!(slices("sô turu ané bêbê whã b^bê supitu").is_empty());
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

    #[test]
    fn flags_a_full_conflict_block() {
        // The marker run is flagged, not the label after it.
        let text = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature/x";
        assert_eq!(conflict_slices(text), vec!["<<<<<<<", "=======", ">>>>>>>"]);
    }

    #[test]
    fn flags_diff3_base_pipes() {
        // The diff3 base marker: a run of pipes is conflict evidence. A bare
        // `|` is legitimate USFM text and stays clean (see below).
        assert_eq!(conflict_slices("||||||| merged common ancestors"), vec!["|||||||"]);
    }

    #[test]
    fn bare_pipe_is_not_a_conflict() {
        assert!(conflict_slices("grace|strong=\"G5485\"").is_empty());
        assert!(conflict_slices("a | b || c").is_empty());
    }

    #[test]
    fn fires_below_git_default_size() {
        // Non-default conflict-marker-size / truncated paste: still caught.
        assert_eq!(conflict_slices("<<<"), vec!["<<<"]);
        assert_eq!(conflict_slices(">>>>"), vec![">>>>"]);
    }

    #[test]
    fn fires_when_newlines_were_collapsed() {
        // Projection dropped the marker's line breaks — no anchor, no space.
        assert_eq!(conflict_slices("ours=======theirs"), vec!["======="]);
    }

    #[test]
    fn requires_at_least_three() {
        // One or two are ordinary text (`<<` quotes, `==` rule fragment).
        assert!(conflict_slices("a < b").is_empty());
        assert!(conflict_slices("a << b == c").is_empty());
    }

    #[test]
    fn clean_verse_has_no_conflict_markers() {
        assert!(conflict_slices("In the beginning God created the heavens.").is_empty());
        assert!(conflict_slices("5 < 7 and 7 > 5").is_empty());
    }
}
