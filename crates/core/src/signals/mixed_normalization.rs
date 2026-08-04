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

use crate::corpus::{Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::span::Span;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const MIXED_NORMALIZATION: RuleId = RuleId::MixedNormalization;

/// One raw grapheme form's chapter-local evidence: how many times it occurred in
/// the chapter, and where it was first seen there.
#[derive(Clone, PartialEq, Eq)]
struct NormRow {
    raw: Box<str>,
    count: u64,
    first: FirstSite,
}

/// A chapter-local site: unpacked `LocalKeyIdx` + `Span`, not the packed
/// `SiteAddr` other retained products use. `SiteAddr` narrows verse-relative byte
/// offsets to `u16` for high-volume site vectors; this rule retains at most one
/// first-site per distinct raw form per chapter (sparse by construction), so the
/// wider `u32` `Span` is the right safety/size tradeoff — a legitimately long
/// verse must not panic narrowing this rule's only deviant occurrence.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FirstSite {
    local: LocalKeyIdx,
    span: Span,
}

/// One chapter's normalization observation: its distinct raw grapheme forms.
///
/// **Boundary state is `()`.** The retired `NormalizationAcc::verse` read only the
/// current verse's `graphemes`, `tape`, `text` and `local_idx`, and its one piece
/// of accumulated state — the per-form `first` site — is a MINIMUM over sites.
/// A minimum folds associatively, so the corpus's first-deviant occurrence is
/// recoverable from independent chapter observations in layout order without any
/// carry: this is what "deterministic first-deviant summary" reduces to once the
/// observation is made self-contained.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct NormChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book fold rather than deep-copied.
    forms: Arc<NormChapterForms>,
}

/// One chapter's distinct raw forms, key-ordered by `raw`.
#[derive(Default, PartialEq, Eq)]
pub(crate) struct NormChapterForms {
    rows: Box<[NormRow]>,
}

/// One chapter's reduced normalization result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct NormReduced {
    token: Box<str>,
    forms: Arc<NormChapterForms>,
}

/// A book's folded normalization contribution: its `((NFC key, raw), count)`
/// addend for the corpus aggregate, plus its chapters' reduced results — which own
/// the chapter-local first-sites the ordered anchor resolution walks.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct NormBookContribution {
    counts: Arc<Vec<NormCountRow>>,
    chapters: Vec<NormReduced>,
}

/// One `((NFC key, raw), count)` row of a book's or the corpus's form counts.
type NormCountRow = ((Box<str>, Box<str>), u64);

/// The normalization corpus aggregate: per-book addends plus the corpus-wide count
/// per `(NFC key, raw)`. **Counts only** — every address lives in the reduced
/// chapters, because an address's corpus ORDER depends on the current layout and a
/// cross-call product may never retain a global `KeyIdx`.
#[derive(Default)]
pub(crate) struct NormCorpusStats {
    per_book: BTreeMap<Box<str>, Arc<Vec<NormCountRow>>>,
    counts: BTreeMap<(Box<str>, Box<str>), u64>,
}

/// The judge key. ONE key for the whole corpus: this rule emits at most one
/// finding, so there is one verdict and no key set to discover (duplicate-word's
/// precedent).
pub(crate) type NormKey = ();

/// The corpus verdict's ORDER-FREE half: whether any NFC key is written two ways
/// at all, and the total minority count summed over every mixed key.
///
/// `affected` is order-free even though the per-key majority's TIE-BREAK is not:
/// a tie means two raw forms have equal counts, so `total - majority_count` is the
/// same whichever of them wins. Which form wins decides the ANCHOR, and the anchor
/// is resolved during materialization, where the current layout gives corpus order.
#[derive(Clone, Copy, Default)]
pub(crate) struct NormOutcome {
    mixed: bool,
    affected: u32,
}

/// The `uni.mixed-normalization` observation substrate. Sole consumer: the rule of
/// the same name.
pub(crate) struct NormalizationSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <NormalizationSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's normalization map: every grapheme cluster carrying the tape's
/// `NORM_RELEVANT` bit, counted by its RAW bytes alone (ADR 0063 — no unsafe skip
/// predicate), with its first chapter-local site.
///
/// Deliberately NFC-free: the NFC key is computed once per distinct raw form at
/// the BOOK fold, not per occurrence, exactly as the retired listener's `finish`
/// did — profiling showed the per-occurrence nested lookup, not normalization
/// itself, was the measurable cost.
fn map_norm_chapter(chapter: &crate::substrate::ChapterView<'_>) -> NormChapterObs {
    let mut forms: FxHashMap<Box<str>, (u64, FirstSite)> = FxHashMap::default();
    let mut tape = Vec::new();
    let mut graphemes = Vec::new();
    for (vi, text) in chapter.texts.iter().enumerate() {
        let local = LocalKeyIdx::from_usize(vi);
        crate::tape::build(text, &mut tape);
        crate::grapheme::segment_tape(text, &tape, &mut graphemes);
        // `ti` advances monotonically in lockstep with `graphemes`: both are built
        // from the same tape in text order, so each tape entry belongs to exactly
        // one grapheme cluster and is visited once.
        let mut ti = 0usize;
        for g in graphemes.iter() {
            let end = g.start + g.len;
            let mut relevant = false;
            while ti < tape.len() && tape[ti].off < end {
                relevant |= tape[ti].cl.is_norm_relevant();
                ti += 1;
            }
            if !relevant {
                continue;
            }
            let raw = g.slice(text);
            match forms.get_mut(raw) {
                Some((count, _)) => *count += 1,
                None => {
                    forms.insert(
                        Box::from(raw),
                        (
                            1,
                            FirstSite {
                                local,
                                span: g.range(),
                            },
                        ),
                    );
                }
            }
        }
    }
    let mut rows: Vec<NormRow> = forms
        .into_iter()
        .map(|(raw, (count, first))| NormRow { raw, count, first })
        .collect();
    rows.sort_by(|a, b| a.raw.cmp(&b.raw));
    NormChapterObs {
        token: Box::from(chapter.chapter),
        forms: Arc::new(NormChapterForms {
            rows: rows.into_boxed_slice(),
        }),
    }
}

/// The NFC key of one raw grapheme form. A non-ASCII form that is already NFC —
/// which includes the both-NFC-and-NFD composition-exclusion case — borrows
/// unchanged; only a form that actually needs normalizing allocates.
fn nfc_key(raw: &str) -> Cow<'_, str> {
    if is_nfc(raw) {
        Cow::Borrowed(raw)
    } else {
        Cow::Owned(raw.nfc().collect::<String>())
    }
}

impl crate::substrate::ObservationSubstrate for NormalizationSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Normalization;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    type Pairing = crate::substrate::NoReference;
    // Normalization keys equivalent forms by grapheme cluster, and reads the tape
    // alongside them for the NORM_RELEVANT prefilter.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::GRAPHEMES;

    type Key = NormKey;
    // Proven from the listener — see `NormChapterObs`.
    type BoundaryState = ();
    type ChapterObservation = NormChapterObs;
    type ReducedChapter = NormReduced;
    type BookContribution = NormBookContribution;
    type CorpusStats = NormCorpusStats;
    // This rule is deterministic and knob-free by design (ADR 0063): no threshold,
    // no calibrated convention. So there is nothing to read at judge either.
    type ExtractorConfig = ();
    type Symbols = ();
    type JudgeConfig = ();
    type EntryOutcome = NormOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> NormChapterObs {
        map_norm_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &NormChapterObs,
        _entering: &(),
        _carry_out: &mut NormReduced,
    ) -> (NormReduced, ()) {
        (
            NormReduced {
                token: observation.token.clone(),
                forms: Arc::clone(&observation.forms),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut NormReduced) {}

    fn fold_book(reduced: &[NormReduced], _symbols: &()) -> NormBookContribution {
        // Sum each distinct raw form over the book's chapters, then key by NFC
        // once per distinct form — the small, book-local set the retired `finish`
        // normalized, not the corpus-wide occurrence stream.
        let mut per_raw: BTreeMap<&str, u64> = BTreeMap::new();
        for r in reduced {
            for row in r.forms.rows.iter() {
                *per_raw.entry(&row.raw).or_default() += row.count;
            }
        }
        let mut counts: Vec<NormCountRow> = per_raw
            .into_iter()
            .map(|(raw, count)| ((Box::from(nfc_key(raw).as_ref()), Box::from(raw)), count))
            .collect();
        counts.sort_by(|a, b| a.0.cmp(&b.0));
        NormBookContribution {
            counts: Arc::new(counts),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut NormCorpusStats,
        slug: &str,
        old: Option<&NormBookContribution>,
        new: Option<&NormBookContribution>,
    ) -> Vec<NormKey> {
        let empty: Vec<NormCountRow> = Vec::new();
        let mut moved = false;
        crate::signals::punctuation::merge_join(
            old.map_or(&empty[..], |c| &c.counts[..]),
            new.map_or(&empty[..], |c| &c.counts[..]),
            |k, o, n| {
                if o == n {
                    return;
                }
                let e = stats.counts.entry(k.clone()).or_default();
                *e = *e + n - o;
                if *e == 0 {
                    stats.counts.remove(k);
                }
                moved = true;
            },
        );
        match new {
            Some(c) => {
                stats.per_book.insert(Box::from(slug), Arc::clone(&c.counts));
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        // Exact, and trivially so: there is one key, so the delta is that key when
        // any form count moved and empty when none did.
        if moved { vec![()] } else { Vec::new() }
    }

    fn judge(_cfg: &(), _key: &NormKey, stats: &NormCorpusStats) -> NormOutcome {
        // `counts` is sorted by `(nfc_key, raw)`, so one scan groups each NFC key's
        // raw forms without building anything.
        let mut out = NormOutcome::default();
        let mut affected = 0u64;
        let mut i = stats.counts.iter().peekable();
        while let Some(((key, _), &first)) = i.next() {
            let mut total = first;
            let mut max = first;
            let mut forms = 1usize;
            while let Some(((k2, _), n)) = i.peek() {
                if *k2 != *key {
                    break;
                }
                let n = **n;
                total += n;
                max = max.max(n);
                forms += 1;
                i.next();
            }
            if forms < 2 {
                continue; // one raw form for this key — silent (non-goal §1.1)
            }
            out.mixed = true;
            affected += total - max;
        }
        out.affected = affected.min(u64::from(u32::MAX)) as u32;
        out
    }
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

/// Resolve the single deterministic finding (ADR 0063) from the resident chapter
/// observations, in the CURRENT layout's order: the earliest deviant occurrence
/// across every mixed NFC key, with the total minority count summed over all of
/// them.
///
/// The ordered pass lives here, not in `judge`, for one reason: corpus order is a
/// property of the current layout, and a cross-call product may never retain a
/// global `KeyIdx` (plan §16). So addresses stay chapter-local in the cache and
/// are rebased here, once, while the layout is in hand. What `judge` supplies is
/// the order-free half — whether anything mixes at all, and the `affected` total —
/// and when it says nothing mixes, this pass does not run.
fn materialize_corpus(
    layout: &[crate::corpus::BookLayout],
    cache: &crate::substrate::SubstrateCache<NormalizationSubstrate>,
    outcome: NormOutcome,
    out: &mut Vec<Finding>,
) {
    if !outcome.mixed {
        return;
    }
    // Per NFC key, per raw form: the corpus-wide count and the earliest global
    // site. Built in layout order, so `first` is a plain first-write.
    let mut merged: FxHashMap<Box<str>, FxHashMap<&str, MergedForm>> = FxHashMap::default();
    for book in layout {
        let Some(contrib) = cache.book_contribution(&book.slug) else {
            continue;
        };
        // Positional zip is truncating: a missing or extra trailing chapter would
        // silently DROP evidence rather than fail. Chapter cardinality is the
        // alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            contrib.chapters.len(),
            book.chapters.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in contrib.chapters.iter().zip(&book.chapters) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for row in chapter.forms.rows.iter() {
                let key_idx = rebase(base, row.first.local);
                let entry = merged
                    .entry(Box::from(nfc_key(&row.raw).as_ref()))
                    .or_default()
                    .entry(&row.raw)
                    .or_insert(MergedForm {
                        count: 0,
                        first_key_idx: key_idx,
                        first_span: row.first.span,
                    });
                entry.count += row.count;
                // Layout order, so an earlier chapter always wrote first; the
                // comparison is kept anyway because two forms can share a verse and
                // the tie must break on byte offset.
                if (key_idx, row.first.span.start) < entry.order_key() {
                    entry.first_key_idx = key_idx;
                    entry.first_span = row.first.span;
                }
            }
        }
    }

    // Per mixed key: the majority form (§3.4's total order), and the earliest
    // candidate anchor among every non-majority form (§3.5).
    let mut anchor: Option<(crate::corpus::KeyIdx, u32, Span, Box<str>)> = None;
    for (key, raw_forms) in &merged {
        if raw_forms.len() < 2 {
            continue; // one raw form for this key — silent (non-goal §1.1)
        }
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
            .map(|(raw, _)| *raw)
            .expect("checked len >= 2 above");
        let key_anchor = raw_forms
            .iter()
            .filter(|(raw, _)| **raw != majority_raw)
            .map(|(_, m)| (m.first_key_idx, m.first_span.start, m.first_span, key.clone()))
            .min_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)))
            .expect("a mixed key has at least one non-majority form");
        if anchor
            .as_ref()
            .is_none_or(|(k, s, ..)| (key_anchor.0, key_anchor.1) < (*k, *s))
        {
            anchor = Some(key_anchor);
        }
    }

    let Some((key_idx, _, span, example)) = anchor else {
        return;
    };
    out.push(Finding {
        key_idx,
        code: MIXED_NORMALIZATION,
        severity: Severity::Warning,
        range: span,
        score: None,
        args: Some(FindingArgs::Normalization {
            affected: outcome.affected,
            example: example.into_string(),
        }),
    });
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct NormMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `uni.mixed-normalization` observation substrate for one analysis: map
/// the dirty chapters through the ordered chapter-map seam, reduce (the identity),
/// judge the one corpus-wide key, and — only when something actually mixes —
/// resolve the single finding's anchor in layout order. When inactive, drop the
/// cached products so an edit while it is disabled does no work for it.
pub(crate) fn drive_normalization(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<NormalizationSubstrate>,
    corpus: &Corpus,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Normalization);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<NormMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp::target_only::<NormalizationSubstrate>(c.hash, &());
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(NormMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView::target(&c.chapter, verses),
                });
            }
            chapters.push((&*c.chapter, stamp));
        }
        if work.len() > run_start {
            book_runs.push(run_start..work.len());
        }
        stamped.push(chapters);
    }
    probe.mark(DrivePhase::Plan);
    let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.map_route = route.label();
    }
    let fresh = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
        NormalizationSubstrate::map_chapter(&w.view, &(), &())
    });
    // Back into caller-order `(book, chapter)` slots, so reduction reads them in
    // corpus order and never in completion order.
    let mut slots: Vec<Vec<Option<NormChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| {
            slots[bi][i].take().unwrap_or_else(|| {
                let c = &book.chapters[i];
                NormalizationSubstrate::map_chapter(
                    &ChapterView::target(&c.chapter, &texts[c.range.clone()]),
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // One key, one verdict — no key-discovery phase to separate.
    let outcome = NormalizationSubstrate::judge(&(), &(), cache.corpus_stats());
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = 1;
    }
    probe.mark(DrivePhase::Judge);
    materialize_corpus(layout, cache, outcome, out);
    probe.mark(DrivePhase::Materialize);
}

/// `uni.mixed-normalization` findings for a whole corpus, via the observation
/// substrate over a fresh transient cache — the single normalization
/// implementation, for tests and calibration callers.
pub fn normalization_findings(corpus: &Corpus) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_normalization(true, &mut cache, corpus, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        normalization_findings(c)
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
    /// mark-order cases together — proves the direct helper and resident
    /// substrate path share one accumulator/emitter and cannot drift.
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
        // Default-off (ADR 0063 perf adjudication) — explicitly enable to
        // exercise the fused path here.
        let mut cfg = crate::Config::v1_defaults();
        cfg.rules.insert(RuleId::MixedNormalization, true);
        let fused: Vec<Finding> = crate::analyze_with_config(&c, None, &cfg)
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
        let without_source = normalization_findings(&target);
        let source = book("GEN", &[(1, "whatever"), (2, "different text entirely")]);
        // The reference is irrelevant to this rule by construction — the substrate
        // declares no reference input at all (`ReferenceStamp::NotDeclared`), so
        // there is no longer an argument to pass one through. Analyzing the same
        // target is the whole of the claim.
        let with_source = normalization_findings(&target);
        let _ = &source;
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

    /// A resident drive, so the incremental path is what the test exercises.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<NormalizationSubstrate>,
        c: &Corpus,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_normalization(true, cache, c, &mut out);
        out
    }

    /// Comparable rendering — key, span and both arg values, so a right-count
    /// wrong-anchor result cannot pass.
    fn render(c: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::Normalization { affected, example }) => {
                        format!("{affected}/{}", example.escape_unicode())
                    }
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}..{}|{a}",
                    c.key(f.key_idx),
                    f.range.start,
                    f.range.end
                )
            })
            .collect()
    }

    /// A multi-chapter book: `(chapter, verse, text)`.
    fn chaptered(name: &str, verses: &[(u16, u16, &str)]) -> Corpus {
        let keys = verses
            .iter()
            .map(|&(ch, v, _)| format!("{name} {ch}:{v}"))
            .collect();
        let texts = verses.iter().map(|&(_, _, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// THE FOLD TEST. The corpus outcome is one finding whose anchor is the
    /// EARLIEST deviant occurrence corpus-wide, and it must fold out of independent
    /// per-chapter observations in layout order. Chapter 1 holds the majority form,
    /// chapter 3 the deviant one; only chapter 3 is remapped when it is introduced,
    /// and the anchor still resolves to chapter 3's verse.
    #[test]
    fn the_corpus_outcome_folds_from_independent_chapter_observations() {
        let clean: &[(u16, u16, &str)] = &[
            (1, 1, YYA),
            (1, 2, YYA),
            (2, 1, YYA),
            (3, 1, "plain"),
            (3, 2, YYA),
        ];
        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(&mut cache, &chaptered("GEN", clean));
        assert!(seeded.is_empty(), "{seeded:?}");
        assert_eq!(cache.mapped, 3, "a cold call maps every chapter");

        let mixed: &[(u16, u16, &str)] = &[
            (1, 1, YYA),
            (1, 2, YYA),
            (2, 1, YYA),
            (3, 1, YA_NUKTA),
            (3, 2, YYA),
        ];
        let corpus = chaptered("GEN", mixed);
        cache.reset_probes();
        let inc = resident(&mut cache, &corpus);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(inc.len(), 1);
        assert_eq!(corpus.key(inc[0].key_idx), "GEN 3:1", "the anchor is the deviant");
        assert_eq!(
            render(&corpus, &inc),
            render(&corpus, &normalization_findings(&corpus)),
            "the resident outcome equals a cold one"
        );
    }

    /// The anchor is the earliest deviant in the CURRENT layout, not the layout the
    /// observation was mapped under: growing an earlier chapter shifts every later
    /// chapter's global base, and the retained addresses are chapter-local.
    #[test]
    fn the_anchor_rebases_when_an_earlier_chapter_grows() {
        let short: &[(u16, u16, &str)] = &[
            (1, 1, YYA),
            (2, 1, YYA),
            (2, 2, YA_NUKTA),
        ];
        let long: &[(u16, u16, &str)] = &[
            (1, 1, YYA),
            (1, 2, YYA),
            (1, 3, YYA),
            (2, 1, YYA),
            (2, 2, YA_NUKTA),
        ];
        let mut cache = crate::substrate::SubstrateCache::new();
        let a = chaptered("GEN", short);
        let first = resident(&mut cache, &a);
        assert_eq!(a.key(first[0].key_idx), "GEN 2:2");

        let b = chaptered("GEN", long);
        cache.reset_probes();
        let second = resident(&mut cache, &b);
        assert_eq!(cache.mapped, 1, "only the grown chapter is remapped");
        assert_eq!(
            b.key(second[0].key_idx),
            "GEN 2:2",
            "the retained chapter-local anchor rebases through the new layout"
        );
        assert_eq!(
            render(&b, &second),
            render(&b, &normalization_findings(&b))
        );
    }

    /// Removing a book removes its forms from the aggregate, which can take the
    /// corpus back to unmixed. Driven residently, so the aggregate under test is
    /// the incrementally maintained one.
    #[test]
    fn removing_a_book_can_unmix_the_corpus() {
        let corpus = multi_book(&[
            ("GEN", &[(1, YYA)]),
            ("EXO", &[(1, YA_NUKTA)]),
        ]);
        let gen_only = multi_book(&[("GEN", &[(1, YYA)])]);
        let mut cache = crate::substrate::SubstrateCache::new();
        assert_eq!(resident(&mut cache, &corpus).len(), 1);

        // Book REMOVAL is shell-driven (`Galley::remove_books` ->
        // `cache.remove_book`), not inferred from a smaller layout.
        cache.remove_book("EXO");
        let after = resident(&mut cache, &gen_only);
        assert!(after.is_empty(), "{after:?}");
    }

    /// Randomized edits across three chapters: a resident cache's finding always
    /// equals a cold analysis of the same corpus (plan §12.6). The shapes mix the
    /// three canonical-order variants and the plain/absent cases, so the majority,
    /// the tie-break and the anchor all move.
    #[test]
    fn resident_normalization_equals_cold_under_randomized_edits() {
        let shapes = [YYA, YA_NUKTA, X_MARKS_CANON, X_MARKS_B, X_MARKS_C, "plain", ""];
        let mut rows: Vec<(u16, u16, String)> = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=5u16 {
                rows.push((ch, v, "plain".to_string()));
            }
        }
        let build = |rows: &[(u16, u16, String)]| {
            let keys = rows.iter().map(|(c, v, _)| format!("GEN {c}:{v}")).collect();
            let texts = rows.iter().map(|(_, _, t)| t.clone()).collect();
            Corpus::try_from_parts(keys, texts).unwrap()
        };
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &build(&rows));
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let ri = (state >> 33) as usize % rows.len();
            let si = (state >> 11) as usize % shapes.len();
            rows[ri].2 = shapes[si].to_string();
            let c = build(&rows);
            let inc = resident(&mut cache, &c);
            assert_eq!(
                render(&c, &inc),
                render(&c, &normalization_findings(&c)),
                "step {step}: resident result diverged from cold"
            );
        }
    }
}
