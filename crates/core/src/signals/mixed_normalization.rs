//! Mixed normalization — detects a corpus writing canonically equivalent
//! grapheme clusters in more than one raw Unicode form (ADR 0063).
//!
//! The unit of comparison is one extended grapheme cluster, keyed by its NFC
//! form. A corpus that consistently writes precomposed `é` is silent, and one
//! that consistently writes `e` + COMBINING ACUTE is equally silent — this
//! rule fires only when *both* raw forms coexist under the same NFC key.
//! Deliberately deterministic and corpus-scoped: there is no threshold, no
//! calibrated convention, and at most one finding for the whole corpus.
//!
//! `unicode-normalization` does the actual NFC work (canonical ordering,
//! recursive decomposition, singleton mappings, composition exclusions);
//! reimplementing a partial table would disagree with JS
//! `String.prototype.normalize` at the wasm boundary.

use std::borrow::Cow;

use rustc_hash::FxHashMap;
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::corpus::{BookGroup, Books, Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::rule::{self, ProjectRule};
use crate::span::Span;
use crate::stream;

pub const MIXED_NORMALIZATION: RuleId = RuleId::MixedNormalization;

/// One raw form's corpus-local evidence: how many times it occurred and
/// where it was first seen. `first` is set once, at the raw form's first
/// occurrence in book order — later occurrences only bump `count`.
#[derive(Clone)]
struct FormSummary {
    count: u64,
    first: FirstSite,
}

/// A book-local site: unpacked `LocalKeyIdx` + `Span`, not the packed
/// `SiteAddr` other retained products use. `SiteAddr` narrows verse-relative
/// byte offsets to `u16` for high-volume site vectors; this rule retains at
/// most one first-site per distinct raw form (sparse by construction), so
/// the wider `u32` `Span` is the right safety/size tradeoff — a legitimately
/// long verse must not panic narrowing this rule's only deviant occurrence.
#[derive(Clone, Copy)]
struct FirstSite {
    local: LocalKeyIdx,
    span: Span,
}

/// One book's grapheme-cluster forms, keyed first by NFC key then by raw
/// byte form. Retained by the analysis cache exactly like bracket-balance's
/// `BookMatch` — a pure function of the book's text, reused unchanged while
/// the book's content hash doesn't move.
///
/// `FxHashMap`, not `BTreeMap` (ADR 0057's internal-hot-path-map pattern):
/// this map takes two lookups per grapheme cluster in the *entire* verse
/// text — every letter, digit, and mark, not just mixed ones — so its
/// lookup cost is corpus-wide, not proportional to the (tiny) mixed set.
/// The nested-`BTreeMap` first cut measured a ~2x `analyze` regression on
/// the criterion bench; iteration order is never observed (`emit` orders
/// everything explicitly by `KeyIdx`/`Span`), so there is no correctness
/// reason to pay for sorted iteration here.
#[derive(Clone, Default)]
pub(crate) struct BookNormalization {
    forms: FxHashMap<Box<str>, FxHashMap<Box<str>, FormSummary>>,
}

/// The mixed-normalization listener: every grapheme cluster in the book,
/// counted by its **raw** bytes alone — no unsafe skip predicate (ADR 0063):
/// every distinct raw form is counted, or mixing like plain ASCII `K` against
/// KELVIN SIGN `U+212A` (which normalizes to `K`), or the Bengali
/// composition-exclusion pair (`U+09AF U+09BC`, itself both NFC and NFD)
/// against composition-excluded `U+09DF` (which normalizes to it), would go
/// undetected.
///
/// Deliberately **flat and NFC-free on the hot path**: `verse()` does exactly
/// one `FxHashMap` lookup per grapheme cluster and never calls `is_nfc`/
/// `.nfc()`. An earlier two-level (NFC key → raw form) shape computed the
/// NFC key for every *occurrence* — i.e. corpus-wide, not proportional to
/// the (tiny) distinct-form set — and profiling confirmed that nested
/// lookup, not normalization itself, was the measurable `analyze` cost.
/// [`finish`] moves the NFC-key computation to run once per **distinct raw
/// form** instead (a small, book-local set), which is where it belongs.
pub(crate) struct NormalizationAcc {
    forms: FxHashMap<Box<str>, FormSummary>,
}

impl NormalizationAcc {
    pub(crate) fn new() -> Self {
        NormalizationAcc {
            forms: FxHashMap::default(),
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        for g in v.graphemes {
            let raw = g.slice(v.text);
            match self.forms.get_mut(raw) {
                Some(summary) => summary.count += 1,
                None => {
                    self.forms.insert(
                        Box::from(raw),
                        FormSummary {
                            count: 1,
                            first: FirstSite {
                                local: v.local_idx,
                                span: g.range(),
                            },
                        },
                    );
                }
            }
        }
    }

    /// Group the book's distinct raw forms by NFC key — the one place this
    /// listener computes normalization, over the small distinct-form set
    /// rather than every occurrence.
    pub(crate) fn finish(self) -> BookNormalization {
        let mut grouped: FxHashMap<Box<str>, FxHashMap<Box<str>, FormSummary>> =
            FxHashMap::default();
        for (raw, summary) in self.forms {
            // Fast borrow path: ASCII is trivially its own NFC form; a
            // non-ASCII form that is already NFC (which includes the
            // both-NFC-and-NFD composition-exclusion case) also borrows
            // `raw` unchanged. Only a form that actually needs normalizing
            // allocates — and only once per distinct form, not per
            // occurrence.
            let key: Cow<'_, str> = if raw.is_ascii() || is_nfc(&raw) {
                Cow::Borrowed(raw.as_ref())
            } else {
                Cow::Owned(raw.nfc().collect::<String>())
            };
            grouped
                .entry(Box::from(key.as_ref()))
                .or_default()
                .insert(raw, summary);
        }
        BookNormalization { forms: grouped }
    }
}

pub(crate) struct MixedNormalization;

impl ProjectRule for MixedNormalization {
    fn id(&self) -> RuleId {
        MIXED_NORMALIZATION
    }

    // The corpus writing its own text two ways is intrinsic to the target;
    // the reference is irrelevant.
    fn check(&self, books: &Books<'_>, _source: Option<&Corpus>) -> Vec<Finding> {
        let summaries: Vec<BookNormalization> = rule::map_books(books, summarize_book);
        emit(books, &summaries)
    }
}

/// One book's grapheme-cluster forms, gathered via the shared fused-walk
/// listener so the direct trait path and the production walk cannot drift.
fn summarize_book(group: &BookGroup<'_>) -> BookNormalization {
    stream::drive_book(
        group,
        stream::Needs {
            graphemes: true,
            ..Default::default()
        },
        NormalizationAcc::new(),
        |a, v| a.verse(v),
        NormalizationAcc::finish,
    )
}

/// One raw form's merged, corpus-wide evidence: total count plus the
/// earliest (already-rebased global) site it was first seen at, for the
/// majority/tie-break/anchor rules below.
struct MergedForm {
    count: u64,
    first_key_idx: crate::corpus::KeyIdx,
    first_span: Span,
}

impl MergedForm {
    /// Caller-presented corpus order: global position, then byte offset
    /// within the verse (ADR 0061). Total, so ties can only be genuine.
    fn order_key(&self) -> (crate::corpus::KeyIdx, u32) {
        (self.first_key_idx, self.first_span.start)
    }
}

/// Merge every book's per-form evidence into one corpus-wide view per NFC
/// key, then emit the single deterministic finding (ADR 0063): the earliest
/// deviant occurrence across every mixed key, with the total minority count
/// summed over all of them. Shared by [`ProjectRule::check`] and the fused
/// walk. `groups` and `summaries` must be index-aligned (`walk_fused`'s
/// output contract, like bracket-balance's `emit`).
pub(crate) fn emit(groups: &Books<'_>, summaries: &[BookNormalization]) -> Vec<Finding> {
    let mut merged: FxHashMap<Box<str>, FxHashMap<Box<str>, MergedForm>> = FxHashMap::default();
    for (group, summary) in groups.iter().zip(summaries) {
        for (nfc_key, raw_forms) in &summary.forms {
            let key_entry = merged.entry(nfc_key.clone()).or_default();
            for (raw, form) in raw_forms {
                let first_key_idx = rebase(group.base, form.first.local);
                let first_span = form.first.span;
                key_entry
                    .entry(raw.clone())
                    .and_modify(|m: &mut MergedForm| {
                        m.count += form.count;
                        if (first_key_idx, first_span.start) < m.order_key() {
                            m.first_key_idx = first_key_idx;
                            m.first_span = first_span;
                        }
                    })
                    .or_insert(MergedForm {
                        count: form.count,
                        first_key_idx,
                        first_span,
                    });
            }
        }
    }

    // Per mixed key: the majority form (§3.4's total order), the minority
    // count it leaves behind, and the earliest candidate anchor among every
    // non-majority form (§3.5).
    let mut affected: u64 = 0;
    let mut anchor: Option<(crate::corpus::KeyIdx, u32, Span, &str)> = None;
    for (nfc_key, raw_forms) in &merged {
        if raw_forms.len() < 2 {
            continue; // one raw form for this key — silent (non-goal §1.1)
        }
        let total: u64 = raw_forms.values().map(|m| m.count).sum();
        let majority_raw: &str = raw_forms
            .iter()
            .max_by(|(a_raw, a), (b_raw, b)| {
                a.count
                    .cmp(&b.count)
                    // Earlier occurrence wins the tie: reverse the order-key
                    // comparison so `max_by` picks the smaller (earlier) one.
                    .then_with(|| b.order_key().cmp(&a.order_key()))
                    .then_with(|| b_raw.cmp(a_raw))
            })
            .map(|(raw, _)| raw.as_ref())
            .expect("checked len >= 2 above");
        let majority_count = raw_forms[majority_raw].count;
        affected += total - majority_count;

        let key_anchor = raw_forms
            .iter()
            .filter(|(raw, _)| raw.as_ref() != majority_raw)
            .map(|(_, m)| (m.first_key_idx, m.first_span.start, m.first_span, nfc_key.as_ref()))
            .min_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)))
            .expect("a mixed key has at least one non-majority form");
        if anchor.is_none_or(|(k, s, ..)| (key_anchor.0, key_anchor.1) < (k, s)) {
            anchor = Some(key_anchor);
        }
    }

    let Some((key_idx, _, span, example)) = anchor else {
        return Vec::new();
    };
    vec![Finding {
        key_idx,
        code: MIXED_NORMALIZATION,
        severity: Severity::Warning,
        range: span,
        score: None,
        args: Some(FindingArgs::Normalization {
            affected: affected.min(u64::from(u32::MAX)) as u32,
            example: example.to_string(),
        }),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::by_book;

    // Composition-excluded Bengali YYA vs its decomposed (also both-NFC-and-
    // NFD) form — the exclusion-case pair the plan calls out by name.
    const YYA: &str = "\u{09DF}";
    const YA_NUKTA: &str = "\u{09AF}\u{09BC}";

    // Three raw byte orderings of the same base + three distinct-class
    // combining marks (ccc 202/220/230) — one is already in canonical
    // ccc order (borrows itself as the NFC key); the other two are not
    // and both normalize (by reordering, not composition) to it.
    const X_MARKS_CANON: &str = "x\u{0327}\u{0316}\u{0301}";
    const X_MARKS_B: &str = "x\u{0316}\u{0327}\u{0301}";
    const X_MARKS_C: &str = "x\u{0301}\u{0327}\u{0316}";

    fn rule() -> MixedNormalization {
        MixedNormalization
    }

    /// A one-chapter book from `(verse, text)` pairs.
    fn book(name: &str, verses: &[(u16, &str)]) -> Corpus {
        let keys = verses.iter().map(|&(v, _)| format!("{name} 1:{v}")).collect();
        let texts = verses.iter().map(|&(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Several books, in the given presented order — for caller-order tests
    /// (ADR 0061): canonical book order must never substitute for it.
    fn multi_book(parts: &[(&str, &[(u16, &str)])]) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for &(name, verses) in parts {
            for &(v, t) in verses {
                keys.push(format!("{name} 1:{v}"));
                texts.push(t.to_string());
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn run(c: &Corpus) -> Vec<Finding> {
        rule().check(&by_book(c), None)
    }

    #[test]
    fn basic_mix_emits_once() {
        let c = book("GEN", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].code, RuleId::MixedNormalization);
    }

    #[test]
    fn affected_count_sums_minority_occurrences() {
        let c = book(
            "GEN",
            &[
                (1, "caf\u{00E9}"),
                (2, "caf\u{00E9}"),
                (3, "caf\u{00E9}"),
                (4, "caf\u{00E9}"),
                (5, "cafe\u{0301}"),
                (6, "cafe\u{0301}"),
            ],
        );
        let f = run(&c);
        assert_eq!(f.len(), 1);
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, .. }) => assert_eq!(*affected, 2),
            other => panic!("expected Normalization args, got {other:?}"),
        }
    }

    #[test]
    fn anchor_is_first_non_majority_occurrence_in_corpus_order() {
        // The majority form brackets the single minority occurrence on both
        // sides — the anchor must be the minority occurrence itself, not
        // "whichever form differs from the previous verse".
        let c = book(
            "GEN",
            &[
                (1, "caf\u{00E9}"),
                (2, "cafe\u{0301}"),
                (3, "caf\u{00E9}"),
            ],
        );
        let f = run(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(c.key(f[0].key_idx), "GEN 1:2");
    }

    #[test]
    fn anchor_range_covers_the_complete_grapheme_cluster() {
        let c = book("GEN", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")]);
        let f = run(&c);
        assert_eq!(f.len(), 1);
        // The anchor is verse 2's "e" + COMBINING ACUTE cluster: bytes 3..6,
        // not just the base "e" (3..4) or just the mark (4..6).
        assert_eq!(f[0].range, Span { start: 3, end: 6 });
    }

    #[test]
    fn consistently_composed_is_silent() {
        let c = book("GEN", &[(1, "caf\u{00E9}"), (2, "r\u{00E9}sum\u{00E9}")]);
        assert!(run(&c).is_empty());
    }

    #[test]
    fn consistently_decomposed_is_silent() {
        let c = book("GEN", &[(1, "cafe\u{0301}"), (2, "re\u{0301}sume\u{0301}")]);
        assert!(run(&c).is_empty());
    }

    #[test]
    fn repeated_identical_raw_bytes_is_one_form_silent() {
        let c = book(
            "GEN",
            &[(1, "caf\u{00E9}"), (2, "caf\u{00E9}"), (3, "caf\u{00E9}")],
        );
        assert!(run(&c).is_empty());
    }

    #[test]
    fn composition_exclusion_consistent_is_silent() {
        let c = book("GEN", &[(1, YYA), (2, YYA)]);
        assert!(run(&c).is_empty());
    }

    #[test]
    fn composition_exclusion_mixing_fires() {
        let c = book("GEN", &[(1, YYA), (2, YA_NUKTA)]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn both_nfc_and_nfd_form_is_retained_not_skipped() {
        // If the fully-decomposed (also-NFC) form were skipped as "already
        // fine", this key would look unmixed and the corpus would be silent.
        let c = book("GEN", &[(1, YYA), (2, YYA), (3, YYA), (4, YA_NUKTA)]);
        let f = run(&c);
        assert_eq!(f.len(), 1);
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, .. }) => assert_eq!(*affected, 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn multi_scalar_example_carries_full_nfc_key() {
        let c = book("GEN", &[(1, YYA), (2, YA_NUKTA)]);
        let f = run(&c);
        assert_eq!(f.len(), 1);
        match &f[0].args {
            Some(FindingArgs::Normalization { example, .. }) => {
                assert_eq!(example, YA_NUKTA);
                assert_eq!(
                    example.chars().count(),
                    2,
                    "composition-exclusion key is multi-scalar"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn ascii_kelvin_singleton_equivalence_fires() {
        let c = book("GEN", &[(1, "5K"), (2, "5\u{212A}")]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn ascii_only_is_silent() {
        let c = book("GEN", &[(1, "5K"), (2, "10K")]);
        assert!(run(&c).is_empty());
    }

    #[test]
    fn canonical_mark_reordering_two_raw_orders_one_key_fires() {
        // Acute (ccc 230) then grave-below (ccc 220) violates canonical
        // order; grave-below then acute matches it. Both raw sequences carry
        // the same two marks, so they share one NFC key once reordered.
        let c = book(
            "GEN",
            &[(1, "a\u{0301}\u{0316}"), (2, "a\u{0316}\u{0301}")],
        );
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn three_raw_forms_one_majority_two_minority_both_count() {
        let c = book(
            "GEN",
            &[
                (1, X_MARKS_CANON),
                (2, X_MARKS_CANON),
                (3, X_MARKS_CANON),
                (4, X_MARKS_B),
                (5, X_MARKS_C),
            ],
        );
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, .. }) => assert_eq!(*affected, 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_distinct_mixed_keys_sum_affected_and_use_globally_earliest_anchor() {
        // Two independently-mixed NFC keys (é and K) in one corpus: still
        // exactly one finding, `affected` sums both keys' minority counts,
        // and the anchor/example come from whichever key's deviant occurs
        // earliest in corpus order — the cross-key accumulator/global-anchor
        // loop (`emit`'s outer loop over `merged`), which a single-key test
        // never exercises.
        let c = book(
            "GEN",
            &[
                (1, "caf\u{00E9}"),  // é key, majority
                (2, "cafe\u{0301}"), // é key, minority — globally first deviant
                (3, "5K"),           // K key, majority
                (4, "5\u{212A}"),    // K key, minority — later than verse 2
            ],
        );
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, example }) => {
                assert_eq!(*affected, 2, "one minority occurrence from each key");
                assert_eq!(example, "\u{00E9}", "anchor must come from the é key, not K");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            c.key(f[0].key_idx),
            "GEN 1:2",
            "the é key's deviant (verse 2) is globally earlier than K's (verse 4)"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_wire_shape_is_pinned() {
        // Multi-scalar `example` (the composition-exclusion NFC key) proves
        // `example` really serializes as a JSON string, not a bare char.
        let c = book("GEN", &[(1, YYA), (2, YYA), (3, YA_NUKTA)]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        let json = serde_json::to_value(&f[0].args).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "normalization",
                "affected": 1,
                "example": YA_NUKTA,
            })
        );
    }

    /// Exercises Latin, the Bengali composition exclusion, and canonical
    /// mark-order cases together — proves the direct `ProjectRule::check`
    /// path and `analyze_with_config`'s fused-walk path share one
    /// accumulator/emitter and cannot drift (plan §8.2).
    #[test]
    fn direct_path_and_fused_path_agree() {
        let c = book(
            "GEN",
            &[
                (1, "caf\u{00E9}"),
                (2, "cafe\u{0301}"),
                (3, YYA),
                (4, YA_NUKTA),
                (5, "a\u{0301}\u{0316}"),
                (6, "a\u{0316}\u{0301}"),
            ],
        );
        let direct = run(&c);
        let fused: Vec<Finding> =
            crate::analyze_with_config(&c, None, &crate::Config::v1_defaults())
                .into_iter()
                .filter(|f| f.code == RuleId::MixedNormalization)
                .collect();
        assert_eq!(direct, fused, "direct and fused paths must agree exactly");
        assert_eq!(direct.len(), 1, "{direct:?}");
    }

    #[test]
    fn fifty_fifty_tie_first_seen_wins_and_later_form_anchors() {
        // Decomposed appears first (1 occurrence), composed appears second
        // (1 occurrence) — a pure count tie. First-seen must win the
        // majority tie-break, so the LATER (composed) occurrence anchors.
        let c = book("GEN", &[(1, "cafe\u{0301}"), (2, "caf\u{00E9}")]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(
            c.key(f[0].key_idx),
            "GEN 1:2",
            "later form (composed) must be the anchored deviant"
        );
    }

    #[test]
    fn pure_ascii_with_no_alternate_form_is_silent() {
        let c = book(
            "GEN",
            &[(
                1,
                "In the beginning God created the heavens and the earth.",
            )],
        );
        assert!(run(&c).is_empty());
    }

    #[test]
    fn empty_corpus_is_silent() {
        let c = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        assert!(run(&c).is_empty());
    }

    #[test]
    fn empty_verse_is_silent() {
        let c = book("GEN", &[(1, "")]);
        assert!(run(&c).is_empty());
    }

    #[test]
    fn source_corpus_does_not_affect_the_result() {
        let target = book("GEN", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")]);
        let without_source = rule().check(&by_book(&target), None);
        let source = book("GEN", &[(1, "whatever"), (2, "different text entirely")]);
        let with_source = rule().check(&by_book(&target), Some(&source));
        assert_eq!(without_source, with_source);
    }

    #[test]
    fn severity_score_and_payload_shape() {
        let c = book("GEN", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")]);
        let f = run(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].score, None);
        assert_eq!(f[0].code, RuleId::MixedNormalization);
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, example }) => {
                assert_eq!(*affected, 1);
                assert_eq!(example, "\u{00E9}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn deviant_past_u16_span_bound_anchors_without_panic() {
        // The deviant occurrence's byte offset *within its own verse*
        // exceeds u16::MAX — proving the retained first-site `Span` (u32)
        // is required; the packed `SiteAddr` other rules use would panic
        // narrowing this (plan §3.3).
        let filler = "a".repeat(70_000);
        let text = format!("caf\u{00E9} {filler} cafe\u{0301}");
        let c = book("GEN", &[(1, text.as_str())]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].range.start as usize > 65_535,
            "deviant span should sit past the u16 bound: {:?}",
            f[0].range
        );
    }

    #[test]
    fn same_raw_form_across_books_is_summed_and_ordered_by_presented_book_order() {
        let c = multi_book(&[
            ("GEN", &[(1, "caf\u{00E9}")][..]),
            (
                "EXO",
                &[(1, "cafe\u{0301}"), (2, "caf\u{00E9}")][..],
            ),
        ]);
        let f = run(&c);
        assert_eq!(f.len(), 1, "{f:?}");
        match &f[0].args {
            Some(FindingArgs::Normalization { affected, .. }) => assert_eq!(*affected, 1),
            other => panic!("{other:?}"),
        }
        assert_eq!(c.key(f[0].key_idx), "EXO 1:1");
    }

    #[test]
    fn reordering_books_changes_the_anchor_per_caller_presented_order() {
        let forward = multi_book(&[
            ("GEN", &[(1, "cafe\u{0301}")][..]),
            ("EXO", &[(1, "caf\u{00E9}")][..]),
        ]);
        let reversed = multi_book(&[
            ("EXO", &[(1, "caf\u{00E9}")][..]),
            ("GEN", &[(1, "cafe\u{0301}")][..]),
        ]);
        let f1 = run(&forward);
        let f2 = run(&reversed);
        assert_eq!(f1.len(), 1, "{f1:?}");
        assert_eq!(f2.len(), 1, "{f2:?}");
        assert_eq!(forward.key(f1[0].key_idx), "EXO 1:1");
        assert_eq!(reversed.key(f2[0].key_idx), "GEN 1:1");
    }

    /// The `Config::all()`-driven generic cache tests in `lib.rs` use only
    /// ASCII/non-equivalent text, so this rule never fires there — they
    /// prove an `Option<BookNormalization>` exists/hits generically, but
    /// never observe a real cached `FirstSite` restored, rebased, and used
    /// in a cross-book verdict. This is the dedicated regression: warm a
    /// two-book corpus where the mix (and its anchor) lives in the *later*
    /// book, prove a no-edit rerun reuses the walk and returns the identical
    /// finding, then grow the *earlier* book — shifting the later book's
    /// global `KeyIdx` base — while the later book stays a cache hit, and
    /// prove the finding resolves to its new, shifted position exactly like
    /// a cache-less cold analyze of the same grown corpus (plan §8.3 #2/#3).
    #[test]
    fn cached_finding_rebases_when_an_earlier_book_grows() {
        let cfg = crate::Config::v1_defaults();
        let original = multi_book(&[
            ("GEN", &[(1, "clean text")][..]),
            ("EXO", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")][..]),
        ]);
        let mut cache = crate::PrepCache::new();
        let (cold_cached, cold_cached_stats) =
            crate::analyze_stateful(&original, None, &cfg, None, Some(&mut cache));
        let hit = cold_cached
            .iter()
            .find(|f| f.code == RuleId::MixedNormalization)
            .expect("the mix fires");
        assert_eq!(original.key(hit.key_idx), "EXO 1:2");

        // No-edit warm rerun: identical finding, walk actually reused.
        let before = cache.probe();
        let (warm, _) = crate::analyze_stateful(
            &original,
            None,
            &cfg,
            Some(cold_cached_stats.clone()),
            Some(&mut cache),
        );
        let after = cache.probe();
        assert_eq!(warm, cold_cached, "no-edit warm rerun matches the cold pass");
        assert_eq!(after.walk_hits - before.walk_hits, 2, "both books reuse their walk");

        // Grow GEN (earlier) by one verse — EXO's global KeyIdx base shifts
        // forward. EXO's content is unchanged, so it must stay a walk hit;
        // its cached FirstSite.local must rebase against the NEW base.
        let grown = multi_book(&[
            ("GEN", &[(1, "clean text"), (2, "extra  space")][..]),
            ("EXO", &[(1, "caf\u{00E9}"), (2, "cafe\u{0301}")][..]),
        ]);
        let before_grow = cache.probe();
        let (cached, cached_stats) =
            crate::analyze_stateful(&grown, None, &cfg, Some(cold_cached_stats), Some(&mut cache));
        let after_grow = cache.probe();
        // A book already known stale (GEN, by content-hash mismatch) walks
        // fresh without ever probing the cache — `walk_hits`/`walk_misses`
        // only observe `cloned_walk` calls, which are short-circuited for
        // it. Only EXO (clean) calls in, and hits.
        assert_eq!(after_grow.walk_hits - before_grow.walk_hits, 1, "EXO (unchanged) reuses its walk");

        let (cold, cold_stats) = crate::analyze_stateful(&grown, None, &cfg, None, None);
        assert_eq!(cached, cold, "cache-hit result must equal a cache-less cold analyze");
        assert_eq!(cached_stats, cold_stats);

        let rebased_hit = cached
            .iter()
            .find(|f| f.code == RuleId::MixedNormalization)
            .expect("the mix still fires");
        assert_eq!(
            grown.key(rebased_hit.key_idx),
            "EXO 1:2",
            "the finding resolves to EXO's shifted position, not a stale one"
        );
    }
}
