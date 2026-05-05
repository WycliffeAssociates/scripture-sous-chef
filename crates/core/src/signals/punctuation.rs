//! Punctuation and spacing conventions (METHODS.md §3.5). All rules in
//! this module operate at the **discourse level** (see
//! `crate::discourse`) — verse-level whitespace/punctuation can't be
//! evaluated in isolation because sentences cross verse boundaries.
//! Findings are mapped back to a Sid (or Sid range) at emit time.
//!
//! Statistical-first, config-as-override: each rule observes the
//! corpus's own convention via Dunning LLR and flags deviations.
//! Config can pin a convention (e.g. "this project uses French
//! spacing") to short-circuit the observation step.

use crate::context::AnalysisContext;
use crate::diagnostics::{AnalyzeStats, Finding, RuleId, Severity};
use crate::discourse::{PairAnomaly, PairAnomalyKind};
use crate::project::Project;
use crate::rule::Rule;

/// Whitespace-spacing convention. Observe the corpus's dominant
/// inter-word spacing pattern (single ASCII space, NBSP after numerals,
/// French-style space-before-`!?:;`, etc.) and flag deviations.
///
/// Subsumes the old `hyg.multiple-whitespace`: in a corpus where
/// double-space is rare, a double-space deviates from convention; in a
/// (hypothetical) corpus where double-space is the norm, single-space
/// is the deviation.
///
/// TODO:
/// - [ ] Build per-corpus distribution of (left-context, whitespace-run,
///       right-context) triples over the discourse stream.
/// - [ ] Dunning LLR per pattern; treat the most-common as canonical
///       when LLR ≥ 10.83.
/// - [ ] Flag deviations; span = the offending whitespace run.
/// - [ ] Config override: `whitespace_convention: { single | french | … }`.
pub const SPACING_CONVENTION: RuleId = RuleId("punct.spacing-convention");

/// Terminal-punctuation convention. Observe whether the corpus uses
/// `…` or `...` for ellipsis, single or doubled `!`/`?` for emphasis,
/// em-dash `—` or double-hyphen `--`, etc. Flag deviations.
///
/// Subsumes the old `hyg.double-punct`: `..` is "deviation from `…`
/// convention" in most corpora; in a corpus that genuinely doesn't use
/// `…`, `..` is just a typo of `.`.
///
/// TODO:
/// - [ ] Catalog of multi-char terminator candidates from the discourse
///       stream.
/// - [ ] Dunning LLR per pattern; pick canonical form per character.
/// - [ ] Flag deviations.
pub const TERMINATOR_CONVENTION: RuleId = RuleId("punct.terminator-convention");

/// Intermedial punctuation: `word,word` when corpus convention is
/// `word, word`. Detects mistyped commas/periods stuck inside a word.
/// (Subset of `SPACING_CONVENTION` — kept distinct because the action
/// is different: this one fires inside-token, the other between-token.)
///
/// TODO:
/// - [ ] Per-corpus distribution of (word, punct, word) triples
///       specifically without surrounding whitespace.
/// - [ ] Dunning LLR per punct character; canonical pattern is "always
///       has whitespace" for almost all punctuation in almost all
///       languages.
pub const INTERMEDIAL_PUNCT: RuleId = RuleId("punct.intermedial");

/// Paired punctuation balance: parens, quotes, brackets opened in the
/// discourse stream without a closing partner within a configurable
/// window. Discourse-level (sentences cross verse boundaries) but the
/// finding is anchored at the Sid where the imbalance becomes visible.
///
/// TODO:
/// - [ ] Configurable pair set (default ASCII + curly quotes + guillemets).
/// - [ ] Track open/close counts across the discourse stream; flag at
///       the end of the configured window (default: chapter).
/// - [ ] Honour script-direction for RTL pairs.
pub const PAIRED_PUNCT_BALANCE: RuleId = RuleId("punct.paired-balance");

pub struct PairedPunctBalance;

impl Rule for PairedPunctBalance {
    fn id(&self) -> RuleId {
        PAIRED_PUNCT_BALANCE
    }

    fn check<'src>(
        &self,
        project: &'src Project<'src>,
        context: &AnalysisContext,
        _stats: &mut AnalyzeStats,
    ) -> Vec<Finding<'src>> {
        scan_paired_punct_balance(project, context)
    }
}

pub fn scan_paired_punct_balance<'src>(
    project: &'src Project<'src>,
    context: &AnalysisContext,
) -> Vec<Finding<'src>> {
    let mut findings = Vec::new();
    for anomaly in &context.span_index.anomalies {
        let Some((sid, verse_off)) = context.discourse.locate(anomaly.byte_offset) else {
            continue;
        };
        let Some(verse) = project.target.verses.get(&sid) else {
            continue;
        };
        let len = anomaly.punct.len_utf8();
        let span = if verse_off + len <= verse.nfc.len() {
            &verse.nfc[verse_off..verse_off + len]
        } else {
            &verse.nfc[verse_off..verse_off]
        };
        let context_snippet = snippet_around(&context.discourse.text, anomaly.byte_offset, len);
        findings.push(Finding {
            rule_id: PAIRED_PUNCT_BALANCE,
            sid,
            severity: Severity::Warn,
            span,
            message: paired_message(anomaly, &context_snippet),
            evidence: 1.0,
        });
    }
    findings
}

/// Extract one word of text on each side of the punctuation
/// character at `byte_offset` so the message is legible inside
/// nested-quote runs (`die.'"'"` is otherwise indistinguishable
/// from any other ambiguous quote in the verse).
///
/// Snaps to the nearest whitespace boundaries up to a small char
/// budget, so `says, 'The` produces `says,<'>The` rather than a
/// raw fixed-width window that might cut a word.
fn snippet_around(text: &str, byte_offset: usize, punct_len: usize) -> String {
    const MAX_CHARS_PER_SIDE: usize = 14;

    // Walk left from `byte_offset` collecting up to MAX_CHARS_PER_SIDE
    // chars, stopping at the first whitespace seen *after* at least
    // one non-whitespace char (so we anchor on a word boundary).
    let before_end = byte_offset.min(text.len());
    let mut before_start = before_end;
    let mut chars = 0usize;
    let mut hit_word = false;
    for (i, c) in text[..before_end].char_indices().rev() {
        if c.is_whitespace() {
            if hit_word {
                before_start = i + c.len_utf8();
                break;
            }
        } else {
            hit_word = true;
        }
        chars += 1;
        before_start = i;
        if chars >= MAX_CHARS_PER_SIDE {
            break;
        }
    }

    let after_start = (byte_offset + punct_len).min(text.len());
    let mut after_end = after_start;
    let mut chars = 0usize;
    let mut hit_word = false;
    for (i, c) in text[after_start..].char_indices() {
        let abs = after_start + i;
        if c.is_whitespace() {
            if hit_word {
                after_end = abs;
                break;
            }
        } else {
            hit_word = true;
        }
        chars += 1;
        after_end = abs + c.len_utf8();
        if chars >= MAX_CHARS_PER_SIDE {
            break;
        }
    }

    let before = text[before_start..before_end].trim_start();
    let punct = &text[before_end..(before_end + punct_len).min(text.len())];
    let after = text[after_start..after_end].trim_end();
    let prefix = if before_start > 0 && !before.is_empty() { "…" } else { "" };
    let suffix = if after_end < text.len() && !after.is_empty() { "…" } else { "" };
    format!("{prefix}{before}{punct}{after}{suffix}")
}

fn paired_message(anomaly: &PairAnomaly, ctx: &str) -> String {
    match anomaly.kind {
        PairAnomalyKind::UnexpectedClose => {
            format!("unexpected closing punctuation '{}' near {ctx}", anomaly.punct)
        }
        PairAnomalyKind::MismatchedClose => match (anomaly.expected_open, anomaly.start_sid) {
            (Some(open), Some(open_sid)) => format!(
                "closing punctuation '{}' does not match open '{}' (opened in {}); near {ctx}",
                anomaly.punct, open, open_sid
            ),
            (Some(open), None) => format!(
                "closing punctuation '{}' does not match open '{}' near {ctx}",
                anomaly.punct, open
            ),
            (None, _) => format!(
                "mismatched closing punctuation '{}' near {ctx}",
                anomaly.punct
            ),
        },
        PairAnomalyKind::UnclosedOpen => match (anomaly.start_sid, anomaly.end_sid) {
            (Some(start), Some(end)) if start == end => format!(
                "unclosed punctuation '{}' in {} near {ctx}",
                anomaly.punct, start
            ),
            (Some(start), Some(end)) if start.book == end.book => format!(
                "unclosed punctuation '{}' opened in {start} near {ctx}, not closed by {end}",
                anomaly.punct,
            ),
            (Some(start), Some(_end)) => format!(
                // Cross-book end_sid only happens when book-boundary
                // flushing didn't land cleanly (shouldn't normally
                // occur). Anchor to start.
                "unclosed punctuation '{}' opened in {} near {ctx} and never closed in book",
                anomaly.punct, start
            ),
            _ => format!("unclosed punctuation '{}' near {ctx}", anomaly.punct),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::marker::PhantomData;

    use crate::config::{Config, ExceptionSet};
    use crate::context::AnalysisContext;
    use crate::project::NamedCorpus;
    use crate::sid::{BookId, Sid};
    use crate::verse::{Verse, build_verse};

    fn sid(book: &str, ch: u16, vs: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), ch, vs)
    }

    fn corpus<'a>(verses: Vec<(Sid, &str)>) -> NamedCorpus<'a> {
        let mut map: BTreeMap<Sid, Verse> = BTreeMap::new();
        for (s, t) in verses {
            map.insert(s, build_verse(s, t.to_string()));
        }
        NamedCorpus {
            name: "t".into(),
            verses: map,
            _src: PhantomData,
        }
    }

    #[test]
    fn flags_unclosed_open_from_span_index() {
        let target = corpus(vec![(sid("GEN", 1, 1), "He said (to them.")]);
        let project = Project {
            target,
            source: None,
            config: Config::default(),
            exceptions: ExceptionSet::default(),
        };
        let context = AnalysisContext::build(&project);
        let findings = scan_paired_punct_balance(&project, &context);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, PAIRED_PUNCT_BALANCE);
        assert_eq!(findings[0].span, "(");
        assert!(findings[0].message.contains("unclosed"));
    }
}
