//! Redundant zero-width line-break controls — a U+200B ZERO WIDTH SPACE that
//! adds no break opportunity the text doesn't already have, so it is almost
//! certainly an editing/paste/tooling artifact rather than intent.
//!
//! U+200B marks a *line/word-break opportunity* (Unicode Core Spec §23.2; UAX #14
//! Line_Break class `ZW`). Two placements are redundant *regardless of script or
//! language*, which is what this rule flags:
//!
//! 1. **A run of two or more consecutive U+200B.** UAX #14 (LB7/LB8) gives a
//!    break at `ZW`; a second adjacent `ZW` is idempotent — it creates no further
//!    break. No orthography doubles it on purpose.
//! 2. **A U+200B immediately adjacent to a U+0020 SPACE** (either side). The space
//!    already provides a break opportunity, so the U+200B does nothing.
//!
//! The finding spans the **whole run**; it means *this run contains redundant
//! copies*, **not** that the position is wrong. Retaining a single U+200B there
//! may still be meaningful (a word/line-break aid in a script that uses it), so
//! the fix is to collapse the redundancy, not necessarily to delete outright.
//!
//! **Deliberately *not* flagged** (each would over-reach the "redundant regardless
//! of language" bar):
//! - **Leading / trailing U+200B.** A [`VerseMap`](crate::verse::VerseMap) value is
//!   not contractually a complete layout unit — verses split mid-sentence and get
//!   concatenated, so a verse-edge U+200B can become a real inter-verse break. Its
//!   redundancy is not universal.
//! - **U+200B adjacent to a *non*-U+200B zero-width/format char** (NBSP, ZWJ, ZWNJ,
//!   WJ, bidi controls). Those are nonbreaking or have distinct behavior — U+200B
//!   beside them may be informative, so only an adjacent *U+200B* is the safe
//!   duplicate case.
//! - **U+200B in-token** (letter↔letter, digit-adjacent) and **adjacent to
//!   punctuation** (slash/hyphen/period/quote). These are exactly the positions
//!   UAX #14 permits a meaningful break (e.g. LB13), so placement there is
//!   spec-sanctioned, not redundant.
//!
//! Severity is **Info** and the rule lives in `uni.*`, not `hyg.*`: redundancy is
//! not *universal invalidity* (UAX #29 segmentation can even shift on an added
//! U+200B), so the stronger hygiene claim is not defensible. Default-on, no knobs.
//!
//! This supersedes the corpus-relative `uni.zero-width-space-anomaly` scorer
//! (ADR 0023): a cross-corpus ablation showed the deterministic redundancy check
//! owns every demonstrated artifact, while the statistical residue was entirely
//! spec-permitted placement or sparse-use false positives (ADR 0027).

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::unicode::ZWSP;

/// U+0020 SPACE — compared as a scalar (not a raw byte) so the adjacency
/// contract reads as the Unicode character it is.
const SPACE: char = '\u{0020}';

pub const REDUNDANT_ZERO_WIDTH_SPACE: RuleId = RuleId::RedundantZeroWidthSpace;

pub struct RedundantZeroWidthSpace;

impl PerVerseRule for RedundantZeroWidthSpace {
    fn id(&self) -> RuleId {
        REDUNDANT_ZERO_WIDTH_SPACE
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn check(&self, text: &str) -> Vec<Span> {
        scan_redundant_zwsp(text)
    }
}

/// Flag each maximal run of consecutive U+200B that is redundant — length ≥ 2,
/// or the scalar immediately before/after the run is U+0020 SPACE. One [`Span`]
/// per redundant run, covering the whole run; runs at a verse edge or beside a
/// non-space, non-U+200B neighbour are left alone.
pub fn scan_redundant_zwsp(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut prev: Option<char> = None;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c != ZWSP {
            prev = Some(c);
            continue;
        }
        // Consume the maximal U+200B run starting here.
        let start = i;
        let mut end = i + ZWSP.len_utf8();
        let mut len = 1usize;
        while let Some(&(j, nc)) = it.peek() {
            if nc != ZWSP {
                break;
            }
            it.next();
            end = j + ZWSP.len_utf8();
            len += 1;
        }
        let after = it.peek().map(|&(_, nc)| nc);
        if len >= 2 || prev == Some(SPACE) || after == Some(SPACE) {
            spans.push(Span { start, end });
        }
        // The scalar preceding whatever comes next is a U+200B (never U+0020),
        // so a following run can't be spuriously called space-adjacent via us.
        prev = Some(ZWSP);
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZW: &str = "\u{200B}";

    fn slices(text: &str) -> Vec<&str> {
        scan_redundant_zwsp(text).iter().map(|s| &text[s.start..s.end]).collect()
    }

    #[test]
    fn space_adjacent_on_either_side_flags() {
        // Space before, and space after: each is one redundant ZWSP.
        assert_eq!(scan_redundant_zwsp(&format!("a {ZW}b")), vec![Span { start: 2, end: 5 }]);
        assert_eq!(scan_redundant_zwsp(&format!("a{ZW} b")), vec![Span { start: 1, end: 4 }]);
    }

    #[test]
    fn edges_are_not_flagged() {
        // A verse is not a guaranteed layout unit, so leading/trailing ZWSP that
        // is neither doubled nor space-adjacent is left alone.
        assert!(scan_redundant_zwsp(&format!("{ZW}a")).is_empty());
        assert!(scan_redundant_zwsp(&format!("a{ZW}")).is_empty());
    }

    #[test]
    fn a_run_is_one_finding_spanning_the_whole_run() {
        // Two adjacent ZWSP → one finding covering both (byte 1..7).
        let f = scan_redundant_zwsp(&format!("a{ZW}{ZW}b"));
        assert_eq!(f, vec![Span { start: 1, end: 7 }]);
        assert_eq!(slices(&format!("a{ZW}{ZW}b")), vec![[ZW, ZW].concat()]);
        // Three in a row is still one finding.
        assert_eq!(scan_redundant_zwsp(&format!("a{ZW}{ZW}{ZW}b")).len(), 1);
    }

    #[test]
    fn non_space_non_zwsp_neighbours_do_not_flag() {
        // NBSP and other controls are nonbreaking / behave differently, so only
        // an adjacent U+200B is the safe duplicate case.
        assert!(scan_redundant_zwsp(&format!("a\u{00A0}{ZW}b")).is_empty()); // NBSP
        assert!(scan_redundant_zwsp(&format!("a\u{200D}{ZW}b")).is_empty()); // ZWJ
        assert!(scan_redundant_zwsp(&format!("a{ZW}\u{200C}b")).is_empty()); // ZWNJ
    }

    #[test]
    fn spec_permitted_placements_stay_silent() {
        // Punctuation- and digit-adjacent and in-token placements are exactly the
        // breaks UAX #14 permits — not redundant, so not flagged.
        for t in [
            format!("a/{ZW}b"),
            format!("a-{ZW}b"),
            format!("a.{ZW}b"),
            format!("1{ZW}2"),
            format!("\u{1780}{ZW}\u{1781}"), // Khmer letter↔letter word break
        ] {
            assert!(scan_redundant_zwsp(&t).is_empty(), "should be silent: {t:?}");
        }
    }

    #[test]
    fn multiple_independent_runs_are_ordered() {
        // A doubled run then a space-adjacent single → two findings, in order.
        let text = format!("a{ZW}{ZW}b {ZW}c");
        let f = scan_redundant_zwsp(&text);
        assert_eq!(f.len(), 2);
        assert!(f[0].start < f[1].start, "spans stay in text order");
        assert_eq!(&text[f[0].start..f[0].end], [ZW, ZW].concat());
        assert_eq!(&text[f[1].start..f[1].end], ZW);
    }

    #[test]
    fn rule_reports_info() {
        assert_eq!(RedundantZeroWidthSpace.severity(), Severity::Info);
        assert_eq!(RedundantZeroWidthSpace.id(), REDUNDANT_ZERO_WIDTH_SPACE);
    }
}
