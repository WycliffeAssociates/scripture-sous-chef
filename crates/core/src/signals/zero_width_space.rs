//! Redundant zero-width line-break controls — a **run of two or more consecutive
//! U+200B ZERO WIDTH SPACE**, which is almost certainly an editing/paste/tooling
//! artifact rather than intent.
//!
//! U+200B marks a line/word-break *opportunity* (Unicode Core Spec §23.2; UAX #14
//! Line_Break class `ZW`, which breaks *after* the control — LB8). Repeating it is
//! **line-break redundant**: adjacent controls give break opportunities at the
//! same zero-width position, so all but one add nothing, and no orthography
//! doubles it on purpose. This rule flags each maximal run of ≥ 2 as **one**
//! finding spanning the run: the run holds redundant *copies*, so collapse it to a
//! single U+200B — which may still be a meaningful break aid.
//!
//! **Only exact-duplicate runs are flagged — nothing about a *single* U+200B's
//! placement.** In particular, a single U+200B beside a U+0020 SPACE is **not**
//! flagged, even though it often looks redundant: it is not *provably* so. LB8
//! breaks after `ZW` (absorbing following spaces) with precedence over LB13, so a
//! U+200B can add a break the space alone does not — e.g. in `word␠<ZWSP>/next`
//! LB8 permits the break before `/`, but removing the U+200B leaves `␠/`, which
//! LB13 *prohibits* breaking before even after a space. Proving space-adjacency
//! redundant would need to analyse the surrounding line-break classes; that
//! machinery is out of scope, so this deterministic rule keeps to the one
//! placement that is unconditionally redundant: exact duplicates.
//!
//! Also not flagged: a single in-token / punctuation- / digit-adjacent U+200B (all
//! UAX #14–relevant break positions); a verse-edge U+200B (a
//! [`VerseMap`](crate::verse::VerseMap) value is not a guaranteed layout unit —
//! verses split mid-sentence and get concatenated); and a U+200B beside a
//! *different* character — a no-break space (NBSP, which is neither zero-width nor
//! a format control), a joiner (ZWJ/ZWNJ), WJ, or a bidi control, each with its
//! own line-break behaviour.
//!
//! Severity is **Info** in `uni.*`, not a `hyg.*` Warning: a duplicate is
//! line-break redundant, not universally invalid (UAX #29 word segmentation can
//! even shift on an added U+200B). Default-on, no knobs.
//!
//! This supersedes the corpus-relative `uni.zero-width-space-anomaly` scorer
//! (ADR 0023): an ablation showed the deterministic duplicate check owns the
//! demonstrated *doubled* artifacts, and the statistical residue was spec-permitted
//! placement or sparse-use false positives (ADR 0027).

use crate::diagnostics::{RuleId, Severity};
use crate::rule::PerVerseRule;
use crate::span::Span;
use crate::unicode::ZWSP;

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

/// Flag each maximal run of two or more consecutive U+200B — the one
/// unconditionally line-break-redundant placement (UAX #14 LB8 makes repeats
/// idempotent). One [`Span`] per run, spanning the whole run. A *single* U+200B is
/// never flagged: its redundancy — even beside a space — depends on the
/// surrounding line-break classes, which this deterministic rule does not analyse.
pub fn scan_redundant_zwsp(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c != ZWSP {
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
        if len >= 2 {
            spans.push(Span { start, end });
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZW: &str = "\u{200B}";

    #[test]
    fn duplicate_run_is_one_finding_spanning_the_whole_run() {
        // Two adjacent ZWSP → one finding covering both (byte 1..7).
        let f = scan_redundant_zwsp(&format!("a{ZW}{ZW}b"));
        assert_eq!(f, vec![Span { start: 1, end: 7 }]);
        // Three in a row is still one finding covering the whole run.
        let t3 = format!("a{ZW}{ZW}{ZW}b");
        let f3 = scan_redundant_zwsp(&t3);
        assert_eq!(f3.len(), 1);
        assert_eq!(&t3[f3[0].start..f3[0].end], [ZW, ZW, ZW].concat());
    }

    #[test]
    fn single_zwsp_is_never_flagged() {
        // Not doubled ⇒ not flagged, whatever the neighbour — including the
        // space-adjacent cases (not *provably* redundant: LB8/LB13), the edge
        // cases (a verse is not a layout unit), and the LB13 counterexample.
        for t in [
            format!("a{ZW}b"),               // in-token word break
            format!("a {ZW}b"),              // space before
            format!("a{ZW} b"),              // space after
            format!("{ZW}a"),                // leading (edge)
            format!("a{ZW}"),                // trailing (edge)
            format!("a {ZW}/b"),             // LB13 case: the ZWSP adds a real break — keep silent
            format!("a\u{00A0}{ZW}b"),       // beside NBSP (no-break space)
            format!("a\u{200D}{ZW}b"),       // beside ZWJ
            format!("\u{1780}{ZW}\u{1781}"), // Khmer letter↔letter word break
        ] {
            assert!(scan_redundant_zwsp(&t).is_empty(), "single ZWSP must not flag: {t:?}");
        }
    }

    #[test]
    fn separated_runs_each_flag_in_order() {
        let text = format!("a{ZW}{ZW}b c{ZW}{ZW}d");
        let f = scan_redundant_zwsp(&text);
        assert_eq!(f.len(), 2);
        assert!(f[0].start < f[1].start, "spans stay in text order");
        assert_eq!(&text[f[0].start..f[0].end], [ZW, ZW].concat());
        assert_eq!(&text[f[1].start..f[1].end], [ZW, ZW].concat());
    }

    #[test]
    fn rule_reports_info() {
        assert_eq!(RedundantZeroWidthSpace.severity(), Severity::Info);
        assert_eq!(RedundantZeroWidthSpace.id(), REDUNDANT_ZERO_WIDTH_SPACE);
    }
}
