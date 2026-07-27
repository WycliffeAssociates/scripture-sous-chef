//! Punctuation signals.
//!
//! `punct.adjacency-anomaly` and `punct.spacing-anomaly` are both corpus-relative
//! and stateful with **aggregate-only** state (ADR 0017, ADR 0024, ADR 0029,
//! ADR 0031): each caches per-book *counts* — never sites — so `Stats` stays
//! tiny; at `judge` each re-scans the current call's verses to emit spans,
//! keeping scores corpus-wide and incremental re-analysis correct. Spans always
//! slice the offending characters out of the verse text.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::charclass::class_of;
use crate::config::{PunctuationAdjacencyConfig, PunctuationSpacingConfig};
use crate::corpus::{Corpus, KeyIdx, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{
    Finding, FindingArgs, RuleId, Severity, SpacingClass, SpacingForm, SpacingSide,
};
use crate::evidence::{
    clamp_count, clamp_rate, clamp_unit, clamp_z, dominance, from_strengths, odds_amplify, strength,
};
use crate::grapheme::{self, GSpan};
use crate::span::Span;
use crate::stream;
use crate::tape::TapeEntry;

// ─────────────────────────────────────────────────────────────────────
// Punctuation adjacency anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// A repeated or mixed punctuation cluster is not inherently a typo — `፤፤`
/// (Ethiopic) and `۔۔` (Arabic) are established conventions in their corpora.
/// So this rule keeps the prior **conservative candidate extraction** (see
/// [`adjacency_candidates`]) but replaces the fixed allow-list verdict with a
/// corpus-rate one: each exact candidate pattern's project-wide count `k` is
/// judged against `N_start(a)`, the number of positions where the pattern's lead
/// glyph `a` begins a maximal same-glyph run. A pattern that is a meaningful
/// share of its lead glyph's opportunities is an established convention and goes
/// silent; a rare one surfaces at `Severity::Info` with a continuous score. A
/// systematic *widespread* typo is suppressed exactly like a convention —
/// corpus counts alone cannot tell them apart (documented limitation).
pub const PUNCTUATION_ADJACENCY_ANOMALY: RuleId = RuleId::PunctuationAdjacencyAnomaly;

/// One chapter's input-independent adjacency observation: the counts and the
/// candidate addresses, both pure functions of the chapter's own text.
///
/// Nothing crosses a chapter seam for this rule, and that is **proven from the
/// listener**, not assumed: the retired `AdjacencyAcc::verse` read only the
/// current verse's `tape` and `text`, and [`count_lead_opportunities`] starts its
/// `prev` at `None` on every call — so a maximal same-glyph run is bounded by its
/// verse in the shipped rule, and a chapter boundary is a verse boundary. The
/// three accumulator fields were a book *tally*, never a carry. Hence
/// `BoundaryState = ()` and reduction is the identity. (This is not the repo's
/// verse-seam footgun in disguise: it says the shipped extraction is
/// verse-scoped, which is a property of the extraction, not an assumption that
/// discourse resets.)
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct AdjacencyChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book contribution — reduction is
    /// the identity, so the table is handed on by `Arc` rather than deep-copied.
    counts: Arc<AdjacencyCounts>,
}

/// One chapter's adjacency evidence. The two count tables are sorted boxed
/// slices, not maps: a chapter holds a few dozen glyphs and a handful of
/// patterns, and sorted slices give a deterministic `Eq`, a merge-join fold, and
/// no per-entry node.
///
/// Counts stay `u64` here. Narrowing them (as the mixed-case chapter table does)
/// would buy nothing: this table has one entry per *glyph*, not per word type, so
/// there is no scatter to halve — and a `u32`/`u16` bound would be a new
/// stop-clause to maintain for no measured gain.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct AdjacencyCounts {
    /// Per-lead-glyph run-start opportunities (`N_start(a)`), sorted by glyph.
    lead: Box<[(char, u64)]>,
    /// Per-exact-pattern occurrence counts, sorted by pattern.
    patterns: Box<[(Box<str>, u64)]>,
    /// Candidate addresses in scan order: verse order, then `(start, end)` within
    /// a verse (`adjacency_runs` sorts its spans). That is exactly the retired
    /// judge's `(key_idx, range.start, range.end)` order, which is what preserves
    /// the within-rule emission order the final stable sort relies on (plan §6.4
    /// as amended) — including the 43 equal-key collisions Entry 1 pinned.
    ///
    /// The **pattern text is not stored**: it is a byte slice of its own verse at
    /// the retained span, so re-deriving it at materialization is plan §11's
    /// indexed lookup, not a re-walk — no tape, no segmentation cache, no
    /// tokenization. This is the principle's default case, and the row takes it.
    sites: Box<[SiteAddr]>,
}

/// One chapter's reduced adjacency result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct AdjacencyReduced {
    token: Box<str>,
    counts: Arc<AdjacencyCounts>,
}

/// A book's folded adjacency contribution: its two ordered count tables (the
/// corpus aggregate's addends) plus its chapters' reduced results, which own the
/// candidate addresses materialization walks.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct AdjacencyBookContribution {
    lead: Arc<Vec<(char, u64)>>,
    patterns: Arc<Vec<(Box<str>, u64)>>,
    chapters: Vec<AdjacencyReduced>,
}

/// The adjacency corpus aggregate. **Counts only** — no site ever enters it; the
/// addresses live in the reduced chapters, where an untouched chapter keeps its
/// own. Every sum is maintained incrementally and bit-exactly across book
/// replacement, because the counts are integers.
#[derive(Default)]
pub(crate) struct AdjacencyCorpusStats {
    /// Per-book addends, so a replacement can subtract exactly what it added.
    /// Its `len()` is the breadth denominator `corpus` — books represented in the
    /// aggregate, which is every book of the corpus (an empty book contributes an
    /// empty pair, exactly as the retired per-book map held an empty entry).
    per_book: BTreeMap<Box<str>, AdjacencyAddends>,
    /// Corpus-wide `N_start(a)` per lead glyph.
    lead: BTreeMap<char, u64>,
    /// Corpus-wide per-pattern `(k, books)`: total occurrences, and how many books
    /// contain the pattern at least once (its breadth support).
    patterns: BTreeMap<Box<str>, (u64, u64)>,
}

/// One book's two addends as the corpus aggregate holds them: its lead-glyph
/// table and its pattern table, shared by `Arc` with the book contribution they
/// were folded into (the aggregate never needs its own copy).
type AdjacencyAddends = (Arc<Vec<(char, u64)>>, Arc<Vec<(Box<str>, u64)>>);

/// The judge key: the exact candidate run (`",,"`, `"?!?"`, `"፤፤"`), so
/// `??`/`???`/`????` stay distinct keys as they always did.
pub(crate) type AdjacencyKey = Box<str>;

/// One pattern's verdict: the score and the raw counts behind it, or `None` when
/// the pattern is an established convention (or below the floor) and stays
/// silent. One outcome serves every site of that pattern, corpus-wide.
#[derive(Clone, Copy, Default)]
pub(crate) struct AdjacencyOutcome {
    /// `(score, k, lead_n, books, corpus)` — the descriptive counts ADR 0048
    /// ships beside the score.
    emit: Option<(f32, u32, u32, u32, u32)>,
}

/// The `punct.adjacency-anomaly` observation substrate. Sole consumer: the rule
/// of the same name.
pub(crate) struct AdjacencySubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <AdjacencySubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's adjacency map, over the chapter's own tape.
fn map_adjacency_chapter(chapter: &crate::substrate::ChapterView<'_>) -> AdjacencyChapterObs {
    let mut lead: BTreeMap<char, u64> = BTreeMap::new();
    let mut patterns: BTreeMap<Box<str>, u64> = BTreeMap::new();
    let mut sites: Vec<SiteAddr> = Vec::new();
    let mut tape = Vec::new();
    for (vi, text) in chapter.texts.iter().enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        crate::tape::build(text, &mut tape);
        count_lead_opportunities(&tape, &mut lead);
        for span in adjacency_candidates(&tape) {
            *patterns.entry(Box::from(span.slice(text))).or_default() += 1;
            sites.push(SiteAddr::pack(local_idx, span));
        }
    }
    AdjacencyChapterObs {
        token: Box::from(chapter.chapter),
        counts: Arc::new(AdjacencyCounts {
            lead: lead.into_iter().collect(),
            patterns: patterns.into_iter().collect(),
            sites: sites.into_boxed_slice(),
        }),
    }
}

/// Sum sorted `(key, count)` addends from several chapters into one ordered
/// table. `BTreeMap` for the accumulation so the result is key-ordered without a
/// sort, which is what the corpus merge-join and the deterministic `Eq` want.
fn fold_counts<K: Ord + Clone>(parts: impl Iterator<Item = (K, u64)>) -> Vec<(K, u64)> {
    let mut acc: BTreeMap<K, u64> = BTreeMap::new();
    for (k, n) in parts {
        *acc.entry(k).or_default() += n;
    }
    acc.into_iter().collect()
}

impl crate::substrate::ObservationSubstrate for AdjacencySubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Adjacency;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;

    type Key = AdjacencyKey;
    // Proven from the listener — see `AdjacencyChapterObs`.
    type BoundaryState = ();
    type ChapterObservation = AdjacencyChapterObs;
    type ReducedChapter = AdjacencyReduced;
    type BookContribution = AdjacencyBookContribution;
    type CorpusStats = AdjacencyCorpusStats;
    // Every `PunctuationAdjacencyConfig` field is read at judge (the two
    // convention rates, the two z's, the breadth gate, the length slope, the
    // floor), so a knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // Candidate patterns are their own text; nothing to name through a table.
    type Symbols = ();
    type JudgeConfig = PunctuationAdjacencyConfig;
    type EntryOutcome = AdjacencyOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> AdjacencyChapterObs {
        map_adjacency_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &AdjacencyChapterObs,
        _entering: &(),
        _carry_out: &mut AdjacencyReduced,
    ) -> (AdjacencyReduced, ()) {
        (
            AdjacencyReduced {
                token: observation.token.clone(),
                counts: Arc::clone(&observation.counts),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut AdjacencyReduced) {}

    fn fold_book(reduced: &[AdjacencyReduced], _symbols: &()) -> AdjacencyBookContribution {
        let lead = fold_counts(
            reduced
                .iter()
                .flat_map(|r| r.counts.lead.iter().map(|&(c, n)| (c, n))),
        );
        let patterns = fold_counts(
            reduced
                .iter()
                .flat_map(|r| r.counts.patterns.iter().map(|(p, n)| (p.clone(), *n))),
        );
        AdjacencyBookContribution {
            lead: Arc::new(lead),
            patterns: Arc::new(patterns),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut AdjacencyCorpusStats,
        slug: &str,
        old: Option<&AdjacencyBookContribution>,
        new: Option<&AdjacencyBookContribution>,
    ) -> Vec<AdjacencyKey> {
        let empty_lead: Vec<(char, u64)> = Vec::new();
        let empty_pat: Vec<(Box<str>, u64)> = Vec::new();
        let old_lead = old.map_or(&empty_lead[..], |c| &c.lead[..]);
        let new_lead = new.map_or(&empty_lead[..], |c| &c.lead[..]);
        let old_pat = old.map_or(&empty_pat[..], |c| &c.patterns[..]);
        let new_pat = new.map_or(&empty_pat[..], |c| &c.patterns[..]);

        // The book count is the breadth denominator, so a book entering or
        // leaving the aggregate moves EVERY pattern's judge inputs.
        let books_before = stats.per_book.len();

        // Apply the replacement. Both sides are sorted, so this is a merge-join;
        // the sums are integers, so subtract-then-add restores the identical
        // value and the aggregate never needs re-folding whole.
        let mut moved_glyphs: Vec<char> = Vec::new();
        merge_join(old_lead, new_lead, |c, o, n| {
            if o == n {
                return;
            }
            let e = stats.lead.entry(*c).or_default();
            *e = *e + n - o;
            if *e == 0 {
                stats.lead.remove(c);
            }
            moved_glyphs.push(*c);
        });
        let mut delta: Vec<AdjacencyKey> = Vec::new();
        merge_join(old_pat, new_pat, |p, o, n| {
            if o == n {
                return;
            }
            let e = stats.patterns.entry(p.clone()).or_default();
            e.0 = e.0 + n - o;
            // Breadth support is a book count: presence in this book is worth
            // exactly one, so a book gaining or losing the pattern moves it by
            // one and a count change within a book leaves it alone.
            if o == 0 {
                e.1 += 1;
            }
            if n == 0 {
                e.1 -= 1;
            }
            if e.0 == 0 {
                stats.patterns.remove(p);
            }
            delta.push(p.clone());
        });

        match new {
            Some(c) => {
                stats.per_book.insert(
                    Box::from(slug),
                    (Arc::clone(&c.lead), Arc::clone(&c.patterns)),
                );
            }
            None => {
                stats.per_book.remove(slug);
            }
        }

        // Widen the delta to every pattern whose judge inputs actually moved. A
        // pattern is judged against its OWN `(k, books)`, its LEAD GLYPH's corpus
        // opportunity count, and the corpus book count — so all three have to be
        // honoured or the delta would be the one wrong answer, a subset.
        if stats.per_book.len() != books_before {
            return stats.patterns.keys().cloned().collect();
        }
        if !moved_glyphs.is_empty() {
            for p in stats.patterns.keys() {
                let a = p.chars().next().expect("candidate pattern is non-empty");
                if moved_glyphs.contains(&a) && !delta.contains(p) {
                    delta.push(p.clone());
                }
            }
        }
        delta
    }

    fn judge(
        cfg: &PunctuationAdjacencyConfig,
        key: &AdjacencyKey,
        stats: &AdjacencyCorpusStats,
    ) -> AdjacencyOutcome {
        let rate = clamp_rate(cfg.convention_rate);
        let z = clamp_z(cfg.confidence_z);
        let breadth_rate = clamp_rate(cfg.breadth_convention_rate);
        let breadth_z = clamp_z(cfg.breadth_z);
        let slope = f64::from(cfg.length_gain_slope).max(0.0);
        let floor = f64::from(clamp_unit(cfg.emit_score_min));
        let corpus_books = stats.per_book.len() as u64;
        // Breadth is a corpus-scale signal — meaningless below a handful of
        // books, where every pattern trivially spans "all" of them. Gate it.
        let breadth_active = corpus_books >= u64::from(cfg.breadth_min_books);

        let (k, books) = stats.patterns.get(key).copied().unwrap_or((0, 0));
        let a = key.chars().next().expect("candidate pattern is non-empty");
        let n = stats.lead.get(&a).copied().unwrap_or(0);

        // Frequency and breadth are *independent* convention evidence combined by
        // noisy-OR (either fully establishing a convention zeroes the base); run
        // length then amplifies the residual as an odds multiplier, so it can
        // raise an anomaly toward 1 but never resurrect a convention (ADR 0031).
        let freq_strength = strength(k, n, rate, z);
        let breadth_strength = if breadth_active {
            strength(books, corpus_books, breadth_rate, breadth_z)
        } else {
            0.0
        };
        let base = from_strengths(&[freq_strength, breadth_strength]);
        let len = key.chars().count() as f64;
        let gain = 1.0 + slope * (len - 2.0);
        let ev = odds_amplify(base, gain);
        if ev < floor {
            return AdjacencyOutcome::default();
        }
        let sat = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        AdjacencyOutcome {
            emit: Some((
                ev as f32,
                sat(k),
                sat(n),
                sat(books),
                sat(corpus_books),
            )),
        }
    }
}

/// Walk two key-sorted `(key, count)` tables together, calling `f(key, old, new)`
/// once per key present in either — with `0` standing for absence. The one place
/// a book's adjacency replacement is applied, so the subtract and the add cannot
/// disagree about which keys they touched.
fn merge_join<K: Ord>(
    old: &[(K, u64)],
    new: &[(K, u64)],
    mut f: impl FnMut(&K, u64, u64),
) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some((a, o)), Some((b, n))) => match a.cmp(b) {
                std::cmp::Ordering::Less => {
                    f(a, *o, 0);
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    f(b, 0, *n);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    f(a, *o, *n);
                    i += 1;
                    j += 1;
                }
            },
            (Some((a, o)), None) => {
                f(a, *o, 0);
                i += 1;
            }
            (None, Some((b, n))) => {
                f(b, 0, *n);
                j += 1;
            }
            (None, None) => unreachable!("loop guard"),
        }
    }
}

impl AdjacencyBookContribution {
    /// Emit this book's adjacency findings: one per retained candidate whose
    /// pattern survived judging, rebasing each chapter-local address to a global
    /// `KeyIdx` via its chapter's current base.
    ///
    /// The pattern is sliced out of the verse text here rather than retained per
    /// site (plan §11): the span is the address, and the address is all a byte
    /// slice needs. The chapter's observation stamp is its text hash, so a cached
    /// chapter's bytes are the bytes its counts were taken from.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        corpus: &Corpus,
        verdicts: &BTreeMap<AdjacencyKey, AdjacencyOutcome>,
        out: &mut Vec<Finding>,
    ) {
        let texts = corpus.texts();
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.counts.sites.iter() {
                let (local, span) = site.unpack();
                let text = &texts[block.range.start + usize::from(local.get())];
                let pattern = span.slice(text);
                // Every candidate's pattern was counted by the same chapter map
                // that produced this address, so it is in the aggregate and has a
                // verdict — a missing one would mean the counts and the sites came
                // from different text.
                let outcome = verdicts
                    .get(pattern)
                    .expect("every retained candidate's pattern is a judged key");
                let Some((score, k, lead_n, books, corpus_books)) = outcome.emit else {
                    continue;
                };
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: PUNCTUATION_ADJACENCY_ANOMALY,
                    severity: Severity::Info,
                    range: span,
                    score: Some(score),
                    args: Some(FindingArgs::AdjacencyEvidence {
                        pattern: pattern.to_string(),
                        k,
                        lead_n,
                        books,
                        corpus: corpus_books,
                    }),
                });
            }
        }
    }
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct AdjacencyMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `punct.adjacency-anomaly` observation substrate for one analysis:
/// map the dirty chapters through the ordered chapter-map seam, reduce (the
/// identity), judge exactly the patterns its retained candidates name, and
/// materialize. When inactive, drop the cached products so an edit while it is
/// disabled does no work for it.
pub(crate) fn drive_adjacency(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<AdjacencySubstrate>,
    corpus: &Corpus,
    cfg: &PunctuationAdjacencyConfig,
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
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Adjacency);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<AdjacencyMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp {
                schema_stamp: AdjacencySubstrate::SCHEMA_STAMP,
                chapter_hash: c.hash,
                extractor_fp: AdjacencySubstrate::extractor_fp(&()),
            };
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(AdjacencyMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView {
                        chapter: &c.chapter,
                        texts: verses,
                    },
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
        AdjacencySubstrate::map_chapter(&w.view, &(), &())
    });
    // Back into caller-order `(book, chapter)` slots, so reduction reads them in
    // corpus order and never in completion order.
    let mut slots: Vec<Vec<Option<AdjacencyChapterObs>>> = layout
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
                AdjacencySubstrate::map_chapter(
                    &ChapterView {
                        chapter: &c.chapter,
                        texts: &texts[c.range.clone()],
                    },
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    // Judge every pattern in the aggregate. Each is named by at least one
    // retained candidate (a pattern is counted only where a candidate produced
    // it), so this is exactly the key set that can emit — no wider. No
    // key-discovery phase for the same reason: the aggregate's key set already
    // IS the judge key set.
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<AdjacencyKey, AdjacencyOutcome> = stats
        .patterns
        .keys()
        .map(|p| (p.clone(), AdjacencySubstrate::judge(cfg, p, stats)))
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, corpus, &verdicts, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `punct.adjacency-anomaly` findings for a whole corpus at a given config, via
/// the observation substrate over a fresh transient cache — the single adjacency
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
pub fn adjacency_findings(corpus: &Corpus, cfg: &PunctuationAdjacencyConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_adjacency(true, &mut cache, corpus, cfg, &mut out);
    // Total order (incl. `end`) so overlapping candidates that share a start
    // (`..` and `..,`) are ordered deterministically — the retired rule's own
    // final sort key.
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

/// Count, per punctuation glyph, the number of positions where it **begins a
/// maximal same-glyph run** — the corpus-relative denominator `N_start(a)`.
/// Computed over the raw text, independent of candidate boundaries: `.,` has
/// two length-1 runs (`.` and `,`), `...` one (`.`), `.,.` three. So a single
/// clean period, a `..`, and the `.` of a `.,` each count once toward `.`; long
/// runs never inflate their own denominator. Excluded candidate patterns
/// (`...`, `--`) still count here as lead-glyph opportunities — they are
/// suppressed from *extraction*, not from the opportunity pool.
pub(crate) fn count_lead_opportunities(tape: &[TapeEntry], out: &mut BTreeMap<char, u64>) {
    let mut prev: Option<char> = None;
    for e in tape {
        if e.cl.is_punctuation() && prev != Some(e.ch) {
            *out.entry(e.ch).or_default() += 1;
        }
        prev = Some(e.ch);
    }
}

/// Sentence-separator class: the only chars considered for *mixed*-run
/// detection. Mixing quotes/brackets with anything is normal typography
/// (`."`, `?»`), so mixed runs are judged inside this class only;
/// *identical* runs are judged for every punctuation char except quotes.
fn is_separator_punct(c: char) -> bool {
    // GC `Po` minus the quote class. The old ASCII set (`. , ; : ? !`)
    // silently skipped every non-Latin separator — ur-deva's `۔` and the
    // dandas were never judged for spacing while their ASCII neighbours were.
    // `Po` admits every script's separators by class while brackets (Ps/Pe),
    // dashes (Pd), connectors (Pc), and curly quotes (Pi/Pf) stay out;
    // straight quotes are `Po` and are excluded by the quote predicate. The
    // corpus verdict, not the candidate set, decides what's conventional
    // (ADR 0029) — a mark with no dominant form stays silent.
    crate::unicode::is_other_punctuation(c) && !is_quote_char(c)
}

/// Quote-class characters. Excluded from identical-run detection:
/// doubled straight quotes (`''` standing in for a double quote, `""` at
/// nested-quotation closes) are systematic conventions in published
/// corpora (es-419 ULB has hundreds), not typos.
///
/// This is an **engine-defined** set (14 chars), not a UCD property. It is read
/// per punctuation char in the adjacency / spacing / punct-only hot loops, so
/// the set is precomputed into the fused `QUOTE` bit (ADR 0046) — one array
/// index instead of a 14-arm `matches!`. The generator's `QUOTE_CHARS` literal
/// is the source of record; `charclass`'s exhaustive sweep pins the bit to this
/// list, so the two cannot drift.
pub(crate) fn is_quote_char(c: char) -> bool {
    crate::charclass::class_of(c).is_quote()
}

/// The conservative candidate domain, preserved verbatim from the prior
/// deterministic rule (ADR: punctuation adjacency anomaly, §10.1): identical
/// maximal runs of non-quote punctuation, and mixed maximal runs within the
/// separator class, minus the known-safe `...` / `--` / `?!` / `!?` set. A
/// mixed run that contains an internal identical sub-run (`..,,`) yields both
/// candidates, as before — extraction is not changed while the verdict model
/// is. Spans slice the exact candidate run out of `text`.
fn adjacency_candidates(tape: &[TapeEntry]) -> Vec<Span> {
    adjacency_runs(tape, false)
}

/// Every adjacency run **including the known-safe set** (`...`, `--`, `?!`,
/// `!?`, `?`-runs) — the census's extraction (rows are never filtered; the
/// safe-list subtraction is the rule's policy, not the count's).
pub(crate) fn adjacency_runs_all(tape: &[TapeEntry]) -> Vec<Span> {
    adjacency_runs(tape, true)
}

/// The shared run extraction; `include_safe` keeps the known-safe patterns
/// the rule's candidate set subtracts.
fn adjacency_runs(tape: &[TapeEntry], include_safe: bool) -> Vec<Span> {
    let mut spans = Vec::new();

    // A tape scalar is a sentence separator (mixed-run class) iff its fused
    // class is `Po` and it is not a quote — the class the tape already carries.
    let is_sep = |e: &TapeEntry| e.cl.is_other_punctuation() && !is_quote_char(e.ch);

    // Pass 1: identical runs of any non-quote punctuation char.
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        let c = e.ch;
        if !e.cl.is_punctuation() || is_quote_char(c) {
            i += 1;
            continue;
        }
        let start = e.off;
        let mut end = start + c.len_utf8() as u32;
        let mut count = 1usize;
        let mut j = i + 1;
        while j < tape.len() && tape[j].ch == c {
            end = tape[j].off + c.len_utf8() as u32;
            count += 1;
            j += 1;
        }
        // `...` ellipsis and `--` em-dash substitutes are universal typography;
        // a run of 3+ `?` is `hyg.replacement-run`'s finding (encoding-
        // conversion damage), skipped here to avoid double-reporting.
        let allowed = !include_safe
            && ((c == '.' && count == 3) || (c == '-' && count == 2) || (c == '?' && count >= 3));
        if count >= 2 && !allowed {
            spans.push(Span { start, end });
        }
        i = j;
    }

    // Pass 2: mixed runs within the sentence-separator class.
    let mut i = 0usize;
    while i < tape.len() {
        let e = tape[i];
        if !is_sep(&e) {
            i += 1;
            continue;
        }
        let c = e.ch;
        let start = e.off;
        let mut end = start + c.len_utf8() as u32;
        let mut run = String::from(c);
        let mut j = i + 1;
        while j < tape.len() && is_sep(&tape[j]) {
            end = tape[j].off + tape[j].ch.len_utf8() as u32;
            run.push(tape[j].ch);
            j += 1;
        }
        let identical = run.chars().all(|x| x == c); // pass 1's business
        let allowed = !include_safe && (run == "?!" || run == "!?");
        if run.chars().count() >= 2 && !identical && !allowed {
            spans.push(Span { start, end });
        }
        i = j;
    }

    spans.sort();
    spans
}

// ─────────────────────────────────────────────────────────────────────
// Punctuation spacing anomaly (corpus-relative, aggregate-only stateful)
// ─────────────────────────────────────────────────────────────────────

/// Every separator mark carries, **per side and conditioned on the neighbour's
/// content class**, a binary *attached*-vs-*spaced* convention. The insight
/// (ADR 0054 second amendment, the pooled class-conditioned model): the typist
/// chooses the **space**, not the neighbour — so condition on the content and
/// judge the choice. For each `(mark, side, class)` where `class ∈ {Letter,
/// Number, Punct}` is the fused-Class of the **first non-whitespace neighbour**
/// on that side, the judged bit is *did whitespace get crossed* — `Spaced` if
/// so (the verse/book **seam** counts as whitespace, its neighbour class read
/// **across** the seam in book order; repo `CLAUDE.md`), `Attached` if the mark
/// clings directly to the neighbour. A form that is the rare minority against
/// its **own class pool's** Wilson-dominant convention is the anomaly.
///
/// **No top-level fallback** (user ruling): a side is judged by its class pool
/// only; a pool without a Wilson-dominant convention is silent. This is what
/// dissolves the old special cases into structural silence and kills the spike's
/// `?)` over-reach — a `?` before `)` lands in the mark's `Punct` pool, judged
/// only if that pool holds a convention; it never falls through to an all-class
/// bucket. Quote is **merged into `Punct`** (user ruling); the period's `."`
/// divergence is logged (ADR 0054 2nd amend.) as evidence for a possible future
/// per-mark split. One mechanism covers: `word,word` (attached-right against a
/// spaced-right Letter convention — invisible to the old before-only rule),
/// `away!Why?`, swapped Spanish `¿`/`?`, verse-leading `.word` (left = spaced
/// via the seam), the `7. 8` cross-reference vs `7.8` decimal split (both in the
/// `Number` pool), and medial `word.word` run-ons. Per side per pool, `score =
/// dominance(the pool's majority) × rarity(minority recurrence)` — ADR 0048
/// descriptive-share dominance, ADR 0050 volume-scaled recurrence knee, scored
/// over each pool's judged occupancy `N_pool`. Candidate domain widened to GC
/// `Po` minus quotes **plus GC `Pd`** (dashes/hyphens/maqaf; user ruling), lone
/// scalars only. A book-edge side with no neighbour even across the seam
/// abstains. Ships **default-disabled** until the consumer opts into a spacing
/// pass.
pub const PUNCTUATION_SPACING_ANOMALY: RuleId = RuleId::PunctuationSpacingAnomaly;

/// Horizontal whitespace that can separate a word from a clinging mark.
fn is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

// This rule's two vocabularies — a side's judged form and its neighbour's
// content class — are the *same* closed sets the shipped finding args publish,
// so they are one type each, defined on the public args surface
// (`diagnostics::{SpacingForm, SpacingClass}`) and given their counter-layout
// here. Keeping the layout local means the args module owns the published
// vocabulary and this module owns how it packs; a separate rule-internal copy
// would let the two drift.

/// Packed-counter slot for a judged form: `attached` is the low bit, which is
/// what makes [`mark_attached_spaced`]'s "form is the low bit" split true by
/// construction.
impl SpacingForm {
    const fn index(self) -> usize {
        match self {
            Self::Attached => 0,
            Self::Spaced => 1,
        }
    }
}

/// Pool slot for a neighbour class, indexing both the twelve packed counters and
/// a [`SideVerdict`]'s `pools` array.
impl SpacingClass {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Punct => 2,
        }
    }
}

/// The number of content classes: `Letter`, `Number`, `Punct`.
const CLASS_COUNT: usize = 3;

/// Which side of a mark a convention describes; `base` is its offset into the
/// twelve packed per-mark counters (a side owns a contiguous `2 · CLASS_COUNT`
/// block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    const fn base(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => CLASS_COUNT * 2,
        }
    }
}

/// Twelve packed per-mark counters: `[side][class][form]`, side ∈ {left, right},
/// class ∈ {Letter, Number, Punct}, form ∈ {attached, spaced} (ADR 0054 2nd
/// amendment, replacing the `[u64; 4]` per-side shape). A `(side, class)` pool's
/// two counts sum to its judged occupancy `N_pool`; a side is judged only where
/// it has a neighbour (a book edge with no neighbour across the seam abstains).
pub(crate) const SIDE_CELLS: usize = CLASS_COUNT * 2 * 2;

/// Split a mark's packed counters into `(attached, spaced)` totals over both
/// sides and all classes — the census's per-mark profile (its equivalence
/// with the rule's cells is by construction: form is the low bit).
pub(crate) fn mark_attached_spaced(cells: &[u64; SIDE_CELLS]) -> (u64, u64) {
    let mut att = 0u64;
    let mut sp = 0u64;
    for (i, &n) in cells.iter().enumerate() {
        if i % 2 == 0 { att += n } else { sp += n }
    }
    (att, sp)
}

/// Packed-counter index for a `(side, class, form)` triple.
const fn cell_index(side: Side, class: SpacingClass, form: SpacingForm) -> usize {
    side.base() + class.index() * 2 + form.index()
}

/// One book's per-mark **per-side per-class tallies**: the twelve counters
/// above, one set per mark (ADR 0054 2nd amendment, replacing the `[u64; 4]`
/// per-side table). **No sites** — spans re-derive from the text at `judge`, so
/// this stays a few dozen bytes per mark even corpus-wide.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BookPunctuationSpacing {
    pub(crate) per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
}

/// Score one spacing occurrence against its mark's corpus verdict, returning the
/// finding it produces (or `None` when neither side is anomalous). Extracted so
/// the aggregate-only `judge` and the [`SpacingSubstrate`] materializer share
/// one scoring body and cannot drift.
///
/// Each judged side is scored by ITS CLASS POOL ONLY (no fallback, user ruling):
/// a side is anomalous only when its pool holds a Wilson-dominant convention AND
/// this form's composed score clears the floor. A pool without a convention, or
/// a side that abstained (a book edge with no neighbour), is silent. An
/// occurrence violating both sides is ONE finding carrying both.
#[allow(clippy::too_many_arguments)]
fn spacing_finding_for_site(
    v: &MarkVerdict,
    floor: f64,
    key_idx: KeyIdx,
    mark: char,
    left: Option<SideRead>,
    right: Option<SideRead>,
    left_span: Span,
    right_span: Span,
) -> Option<Finding> {
    let hit = |sv: &SideVerdict, r: Option<SideRead>| -> Option<PoolHit> {
        let r = r?;
        let pv = &sv.pools[r.class.index()];
        if !pv.holds {
            return None;
        }
        let s = pv.scores[r.form.index()];
        (s >= floor).then(|| PoolHit {
            score: s,
            class: r.class,
            form: r.form,
            count: pv.counts[r.form.index()],
            n: pv.n,
        })
    };
    let lh = hit(&v.left, left);
    let rh = hit(&v.right, right);
    if lh.is_none() && rh.is_none() {
        return None;
    }
    let side_arg = |h: &PoolHit| SpacingSide {
        form: h.form,
        class: h.class,
        count: h.count.min(u64::from(u32::MAX)) as u32,
        total: h.n.min(u64::from(u32::MAX)) as u32,
    };
    let left_arg = lh.as_ref().map(side_arg);
    let right_arg = rh.as_ref().map(side_arg);
    // Highlight the violated side's neighbourhood — the crossed whitespace /
    // attached neighbour where the anomaly sits — union when both sides fire.
    let range = match (lh.is_some(), rh.is_some()) {
        (true, true) => Span {
            start: left_span.start,
            end: right_span.end,
        },
        (true, false) => left_span,
        (false, true) => right_span,
        (false, false) => unreachable!("guarded above"),
    };
    let sc = lh
        .as_ref()
        .map_or(0.0, |h| h.score)
        .max(rh.as_ref().map_or(0.0, |h| h.score));
    Some(Finding {
        key_idx,
        code: PUNCTUATION_SPACING_ANOMALY,
        severity: Severity::Info,
        range,
        score: Some(sc as f32),
        args: Some(FindingArgs::SpacingConvention {
            mark,
            left: left_arg,
            right: right_arg,
        }),
    })
}

/// One `(side, class)` pool's two-factor verdict: whether the pool holds a
/// Wilson-dominant convention (the no-fallback gate), its judged occupancy
/// `N_pool`, its `[attached, spaced]` counts, and each form's composed score.
pub(crate) struct PoolVerdict {
    /// Whether the pool's majority is Wilson-dominant at the floor — the
    /// "the other convention genuinely holds the field" gate. A pool that does
    /// not hold is **silent** (no top-level fallback; user ruling).
    holds: bool,
    /// `N_pool` — occurrences on this side whose neighbour is of this class.
    n: u64,
    /// `[attached, spaced]` counts (sums to `n`).
    counts: [u64; 2],
    /// `[attached, spaced]` composed score `dominance(majority) × rarity(count)`.
    scores: [f64; 2],
}

/// One side's three class pools (`Letter`, `Number`, `Punct`), indexed by
/// [`SpacingClass::index`].
pub(crate) struct SideVerdict {
    pools: [PoolVerdict; CLASS_COUNT],
}

/// A mark's corpus verdict: an independent [`SideVerdict`] per side (ADR 0054
/// 2nd amendment — pooled class-conditioned model).
pub(crate) struct MarkVerdict {
    left: SideVerdict,
    right: SideVerdict,
}

/// A flagged side's resolved verdict pieces, carried from the pool-gated hit
/// test to the finding args.
pub(crate) struct PoolHit {
    score: f64,
    class: SpacingClass,
    form: SpacingForm,
    count: u64,
    n: u64,
}

/// The two-factor verdict for one pool's `[attached, spaced]` binary (ADR 0048
/// dominance, ADR 0050 recurrence), plus its Wilson-dominance gate (ADR 0054
/// 2nd amendment — no fallback). Each form is scored independently:
///
/// - `dominance = wilson_lower_bound(N_pool − count, N_pool, z)` — the
///   *conservative dominance of the majority* (a binary's complement *is* its
///   majority): how strongly the pool's **other** form holds the field. The
///   dominant form (`count ≈ N_pool`) has a tiny complement ⇒ score ≈ 0 ⇒
///   silent; a rare one ⇒ ≈ 1.
/// - `rarity = 1 − min(count − 1, K) / K` — a linear recurrence knee (ADR 0028's
///   shape) whose width scales with the pool's volume:
///   `K = minority_k + rate_per_10k · N_pool / 10 000` (ADR 0050 amendment,
///   retained under per-pool denominators by the ADR 0054 2nd amendment). A form
///   seen once is `rarity = 1` (a rare slip); one recurring past `K` is
///   `rarity = 0` (a second convention). Removing occurrences *raises* the
///   surviving ones' score — clean-as-you-go sharpens the signal.
///
/// `holds` gates the whole pool: `wilson_lower_bound(majority, N_pool, z) ≥
/// floor`. A near-even split, or a thin pool, fails it (Wilson self-gates, no
/// min-samples) and the pool is silent — no all-class fallback.
fn pool_verdict(
    counts: [u64; 2],
    z: f64,
    minority_k: f64,
    rate_per_10k: f64,
    floor: f64,
) -> PoolVerdict {
    let n = counts[0] + counts[1];
    let mut scores = [0.0f64; 2];
    let holds = n > 0 && dominance(counts[0].max(counts[1]), n, z) >= floor;
    if n > 0 {
        let knee = minority_k + rate_per_10k * n as f64 / 10_000.0;
        for (i, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let dominance = dominance(n.saturating_sub(count), n, z);
            let recurrence = (count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
            scores[i] = dominance * (1.0 - recurrence);
        }
    }
    PoolVerdict {
        holds,
        n,
        counts,
        scores,
    }
}

/// One side's three pools from its contiguous six-counter block
/// `[l0_att, l0_sp, l1_att, l1_sp, l2_att, l2_sp]`.
fn side_verdict(
    block: &[u64],
    z: f64,
    minority_k: f64,
    rate_per_10k: f64,
    floor: f64,
) -> SideVerdict {
    let pool = |ci: usize| {
        pool_verdict(
            [block[ci * 2], block[ci * 2 + 1]],
            z,
            minority_k,
            rate_per_10k,
            floor,
        )
    };
    SideVerdict {
        pools: [pool(0), pool(1), pool(2)],
    }
}

/// A mark's verdict from its twelve packed counters (ADR 0054 2nd amendment).
fn mark_verdict(
    counts: &[u64; SIDE_CELLS],
    z: f64,
    minority_k: f64,
    rate_per_10k: f64,
    floor: f64,
) -> MarkVerdict {
    let mid = Side::Right.base();
    MarkVerdict {
        left: side_verdict(&counts[..mid], z, minority_k, rate_per_10k, floor),
        right: side_verdict(&counts[mid..], z, minority_k, rate_per_10k, floor),
    }
}

/// A mark's judged read on one side: the neighbour's content class (the pool)
/// and the attached-vs-spaced form (the bit). A side with no neighbour (a book
/// edge whose seam-cross found nothing) has no `SideRead` — it abstains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SideRead {
    pub(crate) class: SpacingClass,
    pub(crate) form: SpacingForm,
}

/// One separator/dash-mark occurrence: the mark, its read on each side (or
/// `None` where that side abstains), and the neighbourhood span to highlight for
/// each side if it is flagged. The batch extractor is retained as the reference
/// the streaming `SpacingAcc` (the census's extractor) is validated against.
#[cfg(test)]
struct SpacingOpportunity {
    mark: char,
    left: Option<SideRead>,
    right: Option<SideRead>,
    /// `[left neighbourhood … mark end)` — highlighted when the left side fires.
    left_span: Span,
    /// `[mark start … right neighbourhood)` — highlighted when the right fires.
    right_span: Span,
}

/// A spacing opportunity with its verse — the reduce→judge forwarded site
/// (ADR 0044). Carries everything judge's verdict needs, so the site path
/// never touches text. The native product may also live in the content-keyed
/// analysis cache between calls.
#[derive(Clone, PartialEq, Eq)]
pub struct SpacingSite {
    pub(crate) local_idx: LocalKeyIdx,
    pub(crate) mark: char,
    pub(crate) left: Option<SideRead>,
    pub(crate) right: Option<SideRead>,
    pub(crate) left_span: Span,
    pub(crate) right_span: Span,
}

/// A candidate mark: a separator (GC `Po` minus quotes, ADR 0033) **or** a dash
/// (GC `Pd`; ADR 0054 2nd amendment widens the domain). A carrying combining
/// cluster excludes it (checked by the caller, lone-scalar guard).
fn is_candidate_mark(c: char) -> bool {
    is_separator_punct(c) || crate::unicode::is_dash_punctuation(c)
}

/// Classify a non-whitespace neighbour cluster into its content [`SpacingClass`].
/// A cluster containing an alphabetic scalar (incl. base + combining mark, so a
/// decomposed word-final letter still counts) → `Letter`; a leading (non-quote)
/// numeric scalar → `Number`; everything else — another mark, a quote, a
/// bracket, a symbol — → `Punct` (quote merged into `Punct`, user ruling).
fn neighbour_class(cluster: &str) -> SpacingClass {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        SpacingClass::Letter
    } else if cluster
        .chars()
        .next()
        .is_some_and(|c| class_of(c).is_numeric() && !class_of(c).is_quote())
    {
        SpacingClass::Number
    } else {
        SpacingClass::Punct
    }
}

/// First / last non-whitespace grapheme's [`SpacingClass`] in a verse — the edge a
/// neighbouring verse's mark reaches across the seam (book order). `None` when a
/// verse is empty or all-whitespace.
fn verse_edge_classes(text: &str, graphemes: &[GSpan]) -> (Option<SpacingClass>, Option<SpacingClass>) {
    let nonws = |gs: &GSpan| {
        let s = gs.slice(text);
        (!s.is_empty() && !s.chars().all(is_spacing_ws)).then(|| neighbour_class(s))
    };
    (
        graphemes.iter().find_map(nonws),
        graphemes.iter().rev().find_map(nonws),
    )
}

/// Walk every spacing opportunity in a **book** (the parallel-walk unit,
/// ADR 0042), resolving each mark's cross-seam neighbour class from its
/// book-ordered verse neighbours (the seam reads as whitespace, its class read
/// across; repo `CLAUDE.md`). Each verse is grapheme-segmented once; a book edge
/// with no neighbour across the seam yields `None` on that side (abstain).
#[cfg(test)]
fn for_each_spacing_opportunity(
    group: &crate::corpus::BookGroup<'_>,
    mut f: impl FnMut(LocalKeyIdx, &SpacingOpportunity),
) {
    let mut per_verse: Vec<Vec<GSpan>> = Vec::with_capacity(group.len());
    for text in group.texts {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        per_verse.push(g);
    }
    let edges: Vec<(Option<SpacingClass>, Option<SpacingClass>)> = group
        .texts
        .iter()
        .zip(&per_verse)
        .map(|(t, g)| verse_edge_classes(t, g))
        .collect();
    for (vi, text) in group.texts.iter().enumerate() {
        // Nearest previous verse's LAST edge (left of a verse-leading mark), and
        // nearest next verse's FIRST edge (right of a verse-trailing mark).
        let left_cross = (0..vi).rev().find_map(|jj| edges[jj].1);
        let right_cross = (vi + 1..group.len()).find_map(|jj| edges[jj].0);
        for opp in spacing_opportunities(text, &per_verse[vi], left_cross, right_cross) {
            f(LocalKeyIdx::from_usize(vi), &opp);
        }
    }
}

/// Extract every candidate mark's per-side reads from a verse. A lone candidate
/// scalar (GC `Po` minus quotes **or** GC `Pd`; a mark carrying a combining
/// cluster is excluded) is an opportunity — the neighbour need **not** be a
/// letter. Each side: walk over horizontal whitespace, then read the first
/// non-whitespace grapheme's class (the pool) and whether whitespace was crossed
/// (the form). Whitespace crossed **or** the verse/book seam reached is
/// `Spaced` — and at the seam the class is read across it, in book order
/// (`left_cross` / `right_cross`); a book edge with no neighbour abstains
/// (`None`). The per-side span highlights where the space is (the crossed
/// whitespace run) or where it belongs (the attached neighbour grapheme), so the
/// highlight works for a missing space after a mark as well as before it.
#[cfg(test)]
fn spacing_opportunities(
    text: &str,
    graphemes: &[GSpan],
    left_cross: Option<SpacingClass>,
    right_cross: Option<SpacingClass>,
) -> Vec<SpacingOpportunity> {
    walk_opportunities(text, graphemes, left_cross)
        .into_iter()
        .map(|raw| raw.resolve(right_cross))
        .collect()
}

/// One extracted opportunity whose right side may still be unresolved: a mark
/// whose rightward whitespace walk reached the verse end reads its neighbour
/// class across the seam, which a streaming walk only knows when the *next*
/// non-empty verse arrives. At most one raw opportunity per verse can be
/// `right: Seam`, and it is necessarily the verse's last (anything after the
/// mark is whitespace, so no later mark exists).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RawOpportunity {
    mark: char,
    left: Option<SideRead>,
    right: RightState,
    left_span: Span,
    right_span: Span,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum RightState {
    /// Resolved within the verse (or an in-verse abstain can't happen — a
    /// non-seam right always has a neighbour).
    Resolved(Option<SideRead>),
    /// Hit the verse seam: form is `Spaced`, class read across in book order.
    Seam,
}

impl RawOpportunity {
    #[cfg(test)]
    fn resolve(self, right_cross: Option<SpacingClass>) -> SpacingOpportunity {
        let right = match self.right {
            RightState::Resolved(r) => r,
            RightState::Seam => right_cross.map(|class| SideRead {
                class,
                form: SpacingForm::Spaced,
            }),
        };
        SpacingOpportunity {
            mark: self.mark,
            left: self.left,
            right,
            left_span: self.left_span,
            right_span: self.right_span,
        }
    }
}

/// The extraction walk shared by the batch path ([`spacing_opportunities`])
/// and the streaming listener ([`SpacingAcc`]): every lone candidate scalar's
/// per-side reads, with the right seam left unresolved.
fn walk_opportunities(
    text: &str,
    graphemes: &[GSpan],
    left_cross: Option<SpacingClass>,
) -> Vec<RawOpportunity> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        // A lone candidate scalar — a mark carrying a combining cluster is not a
        // clean site, so require the grapheme to be exactly the mark.
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && is_candidate_mark(c) => c,
            _ => continue,
        };
        let mark_start = gs.start;
        let mark_end = mark_start + mark.len_utf8() as u32;

        // Left: walk over horizontal whitespace to the governing neighbour. The
        // highlight starts at the whitespace run (spaced) or the neighbour
        // grapheme (attached); at a seam nothing precedes the mark to show.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 {
            let ps = graphemes[j - 1].slice(text);
            if !ps.is_empty() && ps.chars().all(is_spacing_ws) {
                left_ws = true;
                j -= 1;
            } else {
                break;
            }
        }
        let (left, span_start) = if j == 0 {
            // Seam: form spaced, class read across the seam (book order).
            (
                left_cross.map(|class| SideRead {
                    class,
                    form: SpacingForm::Spaced,
                }),
                mark_start,
            )
        } else {
            let nb = graphemes[j - 1];
            let class = neighbour_class(nb.slice(text));
            let form = if left_ws {
                SpacingForm::Spaced
            } else {
                SpacingForm::Attached
            };
            // Highlight the crossed ws run (spaced) or the attached neighbour.
            let span_start = if left_ws {
                graphemes[j].start
            } else {
                nb.start
            };
            (Some(SideRead { class, form }), span_start)
        };

        // Right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() {
            let ns = graphemes[k + 1].slice(text);
            if !ns.is_empty() && ns.chars().all(is_spacing_ws) {
                right_ws = true;
                k += 1;
            } else {
                break;
            }
        }
        let (right, span_end) = if k + 1 >= graphemes.len() {
            (RightState::Seam, mark_end)
        } else {
            let nb = graphemes[k + 1];
            let class = neighbour_class(nb.slice(text));
            let form = if right_ws {
                SpacingForm::Spaced
            } else {
                SpacingForm::Attached
            };
            let span_end = if right_ws {
                graphemes[k].range().end
            } else {
                nb.range().end
            };
            (
                RightState::Resolved(Some(SideRead { class, form })),
                span_end,
            )
        };

        out.push(RawOpportunity {
            mark,
            left,
            right,
            left_span: Span {
                start: span_start,
                end: mark_end,
            },
            right_span: Span {
                start: mark_start,
                end: span_end,
            },
        });
    }
    out
}

/// The spacing counting listener: one book's per-mark tallies plus the
/// forwarded sites, fed per verse by the fused walk. Carries the cross-seam
/// state the batch walk pre-computed: the nearest previous non-empty verse's
/// trailing edge class (a mark's left seam read), and the at-most-one
/// opportunity whose right side awaits the next non-empty verse's leading
/// edge. Both resolve exactly as the batch walk's `left_cross`/`right_cross`
/// (repo CLAUDE.md: the seam reads as whitespace, its class read across it in
/// book order; a book edge with no neighbour abstains).
pub(crate) struct SpacingAcc {
    per_mark: BTreeMap<char, [u64; SIDE_CELLS]>,
    sites: Vec<SpacingSite>,
    left_cross: Option<SpacingClass>,
    pending: Option<PendingSeam>,
}

/// A buffered right-seam opportunity: everything but the right read is known.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PendingSeam {
    local_idx: LocalKeyIdx,
    mark: char,
    left: Option<SideRead>,
    left_span: Span,
    right_span: Span,
}

impl SpacingAcc {
    pub(crate) fn new() -> Self {
        SpacingAcc {
            per_mark: BTreeMap::new(),
            sites: Vec::new(),
            left_cross: None,
            pending: None,
        }
    }

    fn record(
        &mut self,
        local_idx: LocalKeyIdx,
        mark: char,
        left: Option<SideRead>,
        right: Option<SideRead>,
        left_span: Span,
        right_span: Span,
    ) {
        let cell = self.per_mark.entry(mark).or_insert([0u64; SIDE_CELLS]);
        if let Some(r) = left {
            cell[cell_index(Side::Left, r.class, r.form)] += 1;
        }
        if let Some(r) = right {
            cell[cell_index(Side::Right, r.class, r.form)] += 1;
        }
        self.sites.push(SpacingSite {
            local_idx,
            mark,
            left,
            right,
            left_span,
            right_span,
        });
    }

    fn resolve_pending(&mut self, right_cross: Option<SpacingClass>) {
        if let Some(p) = self.pending.take() {
            let right = right_cross.map(|class| SideRead {
                class,
                form: SpacingForm::Spaced,
            });
            self.record(
                p.local_idx,
                p.mark,
                p.left,
                right,
                p.left_span,
                p.right_span,
            );
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        // This verse's edges under the same predicate as `verse_edge_classes`.
        let nonws = |gs: &GSpan| {
            let s = gs.slice(v.text);
            (!s.is_empty() && !s.chars().all(is_spacing_ws)).then(|| neighbour_class(s))
        };
        let first_edge = v.graphemes.iter().find_map(nonws);

        // A non-empty verse resolves the previous verse's seam-right mark —
        // before its own opportunities, preserving site order. An empty /
        // all-whitespace verse (no edge) has no opportunities and leaves both
        // carried states untouched, exactly like the batch walk's `find_map`
        // skipping it.
        if first_edge.is_some() {
            self.resolve_pending(first_edge);
        }

        for raw in walk_opportunities(v.text, v.graphemes, self.left_cross) {
            match raw.right {
                RightState::Resolved(right) => {
                    self.record(
                        v.local_idx,
                        raw.mark,
                        raw.left,
                        right,
                        raw.left_span,
                        raw.right_span,
                    );
                }
                RightState::Seam => {
                    debug_assert!(self.pending.is_none(), "≤1 seam-right mark per verse");
                    self.pending = Some(PendingSeam {
                        local_idx: v.local_idx,
                        mark: raw.mark,
                        left: raw.left,
                        left_span: raw.left_span,
                        right_span: raw.right_span,
                    });
                }
            }
        }

        if let Some(last_edge) = v.graphemes.iter().rev().find_map(nonws) {
            self.left_cross = Some(last_edge);
        }
    }

    pub(crate) fn finish(mut self) -> (BookPunctuationSpacing, Vec<SpacingSite>) {
        // Book edge: no neighbour across the seam — the side abstains.
        self.resolve_pending(None);
        (
            BookPunctuationSpacing {
                per_mark: self.per_mark,
            },
            self.sites,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────
// Spacing observation substrate (plan §5.2, Phase C)
// ─────────────────────────────────────────────────────────────────────

/// The `punct.spacing-anomaly` observation substrate. Its map is the per-verse
/// spacing extraction with the ONE cross-verse dependency deferred (a
/// verse-leading mark's left neighbour reads the previous non-empty verse's
/// trailing edge — carried in reduction, never baked into the observation), so a
/// chapter's observation is identical wherever the chapter sits. Its boundary
/// state is the code-proven seam carry: the previous trailing-edge class plus a
/// pending trailing candidate mark whose right neighbour lives in the next
/// verse/chapter (owner adjudication 2026-07-24; JHN 7:53 → 8:1 is the canonical
/// case). Reduction threads that pair left to right exactly as the streaming
/// walk threads `left_cross`/`pending` across verse seams — a chapter boundary
/// is not a discourse reset (repo `CLAUDE.md`).
pub(crate) struct SpacingSubstrate;

/// Pins the substrate's registry id at compile time (the typed cache slot and
/// the closed `SubstrateId` name the same substrate).
const _: crate::substrate::SubstrateId =
    <SpacingSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One verse's input-independent spacing observation: its extracted
/// opportunities (with the left-seam dependency deferred — a verse-leading
/// mark's `left` is `None` here and resolved against the entering carry in
/// reduction) and its edge classes (the seam neighbours a mark reaches across).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SpacingVerseObs {
    opps: Vec<RawOpportunity>,
    first_edge: Option<SpacingClass>,
    last_edge: Option<SpacingClass>,
}

/// One chapter's spacing observation: its opaque token (the carry owner tag) and
/// its verses' observations in presented order.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SpacingChapterObs {
    token: Box<str>,
    verses: Vec<SpacingVerseObs>,
}

/// The spacing boundary state carried across chapters (plan §5.2): the previous
/// non-empty verse's trailing-edge class (a verse-leading mark's left seam read)
/// and a pending trailing candidate mark whose right neighbour awaits the next
/// non-empty verse — tagged with the opaque token of the chapter that owns it,
/// so a resolution folds into the right chapter even across an all-empty one.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SpacingBoundary {
    left_cross: Option<SpacingClass>,
    pending: Option<(Box<str>, PendingSeam)>,
}

/// One chapter's reduced spacing result: the per-mark cell contributions it
/// resolved (its own marks, plus any cross-seam mark it owns once its far
/// neighbour resolved) and its keyed sites, chapter-local, in scan order.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SpacingReduced {
    token: Box<str>,
    cells: BTreeMap<char, [u64; SIDE_CELLS]>,
    sites: Vec<SpacingSite>,
}

/// A book's folded spacing contribution: its per-mark cells (the corpus
/// aggregate's addends) and its keyed sites grouped by owning chapter token, in
/// book order — the materializer rebases each site via its chapter's current
/// base.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SpacingBookContribution {
    cells: BTreeMap<char, [u64; SIDE_CELLS]>,
    chapters: Vec<(Box<str>, Vec<SpacingSite>)>,
}

/// The spacing corpus aggregate: per-mark cells summed over books — the sole
/// input to the per-mark verdict.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SpacingCorpusStats {
    totals: BTreeMap<char, [u64; SIDE_CELLS]>,
}

/// Reconstruct a mark's left read from its observation: `None` in the
/// observation means the mark was verse-leading (its left seam was deferred),
/// so it reads the entering `left_cross`; a `Some` was resolved within the verse
/// and is independent of the carry.
#[allow(dead_code)] // wired into the transition in Phase C step 2
fn resolve_left(raw_left: Option<SideRead>, left_cross: Option<SpacingClass>) -> Option<SideRead> {
    match raw_left {
        Some(sr) => Some(sr),
        None => left_cross.map(|class| SideRead {
            class,
            form: SpacingForm::Spaced,
        }),
    }
}

/// Add one occurrence's per-side cells + site into a reduced chapter (the shared
/// body behind both a within-verse record and a resolved pending).
#[allow(dead_code)] // wired into the transition in Phase C step 2
fn record_into(
    dest: &mut SpacingReduced,
    local_idx: LocalKeyIdx,
    mark: char,
    left: Option<SideRead>,
    right: Option<SideRead>,
    left_span: Span,
    right_span: Span,
) {
    let cell = dest.cells.entry(mark).or_insert([0u64; SIDE_CELLS]);
    if let Some(r) = left {
        cell[cell_index(Side::Left, r.class, r.form)] += 1;
    }
    if let Some(r) = right {
        cell[cell_index(Side::Right, r.class, r.form)] += 1;
    }
    dest.sites.push(SpacingSite {
        local_idx,
        mark,
        left,
        right,
        left_span,
        right_span,
    });
}

impl crate::substrate::ObservationSubstrate for SpacingSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Spacing;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;

    type Key = char;
    type BoundaryState = SpacingBoundary;
    type ChapterObservation = SpacingChapterObs;
    type ReducedChapter = SpacingReduced;
    type BookContribution = SpacingBookContribution;
    type CorpusStats = SpacingCorpusStats;
    // Spacing has NO extraction knobs — every config field is a judging knob, so
    // the extractor config is `()` and its fingerprint is a constant.
    type ExtractorConfig = ();
    type JudgeConfig = PunctuationSpacingConfig;
    // Spacing's observations are mark/side counts and cross-seam pendings — no
    // word identities to name, so no symbol table.
    type Symbols = ();
    type EntryOutcome = MarkVerdict;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> SpacingChapterObs {
        let mut verses = Vec::with_capacity(chapter.texts.len());
        for text in chapter.texts {
            let mut g = Vec::new();
            grapheme::segment(text, &mut g);
            let (first_edge, last_edge) = verse_edge_classes(text, &g);
            // Predecessor-free: pass `left_cross = None`, so a verse-leading
            // mark's left reads `None` (deferred to reduction). Every other side
            // is resolved within the verse and independent of any carry.
            let opps = walk_opportunities(text, &g, None);
            verses.push(SpacingVerseObs {
                opps,
                first_edge,
                last_edge,
            });
        }
        SpacingChapterObs {
            token: Box::from(chapter.chapter),
            verses,
        }
    }

    fn pending_owner(state: &SpacingBoundary) -> Option<&str> {
        state.pending.as_ref().map(|(tok, _)| &**tok)
    }

    fn reduce_chapter(
        observation: &SpacingChapterObs,
        entering: &SpacingBoundary,
        carry_out: &mut SpacingReduced,
    ) -> (SpacingReduced, SpacingBoundary) {
        let mut this = SpacingReduced {
            token: observation.token.clone(),
            cells: BTreeMap::new(),
            sites: Vec::new(),
        };
        let mut left_cross = entering.left_cross;
        // The pending buffer carries its OWNER: `Some(token)` is a seam mark
        // owned by an earlier chapter (resolve into `carry_out`, whose reduced
        // result the driver selected by that same token), `None` is one buffered
        // from this chapter's own verse (resolve into `this`). The owner travels
        // with the seam until it resolves or a new local seam replaces it — a
        // pending mark may cross an all-empty chapter, and its `local_idx` is an
        // index into its OWNER's chapter, so re-owning it to whatever chapter
        // happens to be passing would rebase the site against the wrong range.
        // Site order matches the streaming walk: a resolved pending records just
        // before the resolving verse's own marks.
        let mut pending: Option<(Option<Box<str>>, PendingSeam)> = entering
            .pending
            .as_ref()
            .map(|(owner, ps)| (Some(owner.clone()), ps.clone()));

        for (vi, v) in observation.verses.iter().enumerate() {
            let li = LocalKeyIdx::from_usize(vi);
            // A non-empty verse resolves the buffered pending (foreign → the
            // owner via `carry_out`, local → `this`), before its own marks.
            if v.first_edge.is_some()
                && let Some((owner, seam)) = pending.take()
            {
                let right = v.first_edge.map(|class| SideRead {
                    class,
                    form: SpacingForm::Spaced,
                });
                // An owned (foreign) seam materializes into its OWNER's reduced
                // result — never the resolving chapter's.
                let dest = if owner.is_some() {
                    &mut *carry_out
                } else {
                    &mut this
                };
                record_into(
                    dest,
                    seam.local_idx,
                    seam.mark,
                    seam.left,
                    right,
                    seam.left_span,
                    seam.right_span,
                );
            }
            for raw in &v.opps {
                let left = resolve_left(raw.left, left_cross);
                match &raw.right {
                    RightState::Resolved(right) => {
                        record_into(
                            &mut this,
                            li,
                            raw.mark,
                            left,
                            *right,
                            raw.left_span,
                            raw.right_span,
                        );
                    }
                    RightState::Seam => {
                        // Buffer this chapter's own trailing seam mark (owner
                        // `None` = local); its right awaits the next non-empty
                        // verse (this chapter or a later one). At most one per
                        // verse (its verse-last mark). This REPLACES any carried
                        // seam — the same single-slot semantics as the streaming
                        // walk. (Unreachable in practice: a verse holding a mark
                        // has non-whitespace content, so it resolves the carried
                        // seam above before buffering its own.)
                        pending = Some((
                            None,
                            PendingSeam {
                                local_idx: li,
                                mark: raw.mark,
                                left,
                                left_span: raw.left_span,
                                right_span: raw.right_span,
                            },
                        ));
                    }
                }
            }
            if v.last_edge.is_some() {
                left_cross = v.last_edge;
            }
        }

        let leaving = SpacingBoundary {
            left_cross,
            // Keep the carried seam's ORIGINAL owner; only a seam buffered here
            // (owner `None`) takes this chapter's token.
            pending: pending
                .map(|(owner, seam)| (owner.unwrap_or_else(|| observation.token.clone()), seam)),
        };
        (this, leaving)
    }

    fn finish_book(leaving: &SpacingBoundary, carry_out: &mut SpacingReduced) {
        // Book edge: no neighbour across the final seam — the pending's right
        // side abstains (its right read is `None`), folded into its owner.
        if let Some((_, seam)) = &leaving.pending {
            record_into(
                carry_out,
                seam.local_idx,
                seam.mark,
                seam.left,
                None,
                seam.left_span,
                seam.right_span,
            );
        }
    }

    fn fold_book(reduced: &[SpacingReduced], _symbols: &()) -> SpacingBookContribution {
        let mut cells: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
        let mut chapters = Vec::with_capacity(reduced.len());
        for r in reduced {
            for (&mark, counts) in &r.cells {
                let e = cells.entry(mark).or_insert([0u64; SIDE_CELLS]);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x += y;
                }
            }
            chapters.push((r.token.clone(), r.sites.clone()));
        }
        SpacingBookContribution { cells, chapters }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut SpacingCorpusStats,
        _slug: &str,
        old: Option<&SpacingBookContribution>,
        new: Option<&SpacingBookContribution>,
    ) -> Vec<char> {
        // EXACT stats-delta keys: a mark is returned only when its FINAL corpus
        // aggregate differs from the value it had before this replacement. A book
        // whose sites merely moved — same per-mark cells — yields an EMPTY stats
        // delta even though its site delta is dirty; the two are unioned into the
        // judge-dirty set by the caller, so site movement is never inferred from
        // (equal) counts and equal counts never force a needless re-judge.
        const EMPTY: [u64; SIDE_CELLS] = [0u64; SIDE_CELLS];
        let candidates: std::collections::BTreeSet<char> = old
            .into_iter()
            .flat_map(|c| c.cells.keys().copied())
            .chain(new.into_iter().flat_map(|c| c.cells.keys().copied()))
            .collect();
        // Snapshot each candidate's aggregate before mutating.
        let before: Vec<(char, [u64; SIDE_CELLS])> = candidates
            .iter()
            .map(|&mark| (mark, *stats.totals.get(&mark).unwrap_or(&EMPTY)))
            .collect();

        if let Some(old) = old {
            for (&mark, counts) in &old.cells {
                let e = stats.totals.entry(mark).or_insert(EMPTY);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x -= y;
                }
            }
        }
        if let Some(new) = new {
            for (&mark, counts) in &new.cells {
                let e = stats.totals.entry(mark).or_insert(EMPTY);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x += y;
                }
            }
        }
        // Drop marks whose aggregate fell to all-zero, so an absent mark and a
        // zeroed mark are the same state (`judge` reads a missing key as EMPTY).
        for &mark in &candidates {
            if stats.totals.get(&mark).is_some_and(|c| *c == EMPTY) {
                stats.totals.remove(&mark);
            }
        }

        before
            .into_iter()
            .filter(|&(mark, prev)| *stats.totals.get(&mark).unwrap_or(&EMPTY) != prev)
            .map(|(mark, _)| mark)
            .collect()
    }

    fn judge(
        cfg: &PunctuationSpacingConfig,
        key: &char,
        stats: &SpacingCorpusStats,
    ) -> MarkVerdict {
        let z = clamp_z(cfg.confidence_z);
        let minority_k = clamp_count(cfg.minority_recurrence_k);
        let minority_rate = clamp_count(cfg.minority_rate_per_10k);
        let floor = f64::from(clamp_unit(cfg.emit_score_min));
        let empty = [0u64; SIDE_CELLS];
        let counts = stats.totals.get(key).unwrap_or(&empty);
        mark_verdict(counts, z, minority_k, minority_rate, floor)
    }
}

/// The `punct.spacing-anomaly` emission floor for a config — the two-factor
/// score a rare form must clear to surface. Shared by the substrate materializer
/// and its judge so the threshold is applied identically.
pub(crate) fn spacing_floor(cfg: &PunctuationSpacingConfig) -> f64 {
    f64::from(clamp_unit(cfg.emit_score_min))
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct SpacingMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `punct.spacing-anomaly` observation substrate for one analysis
/// (plan §5.2). When active: plan the dirty chapters across every book, map them
/// through the ordered chapter-map seam (one Rayon grain, caller-order slots),
/// replay each book's ordered reduction to convergence, judge every mark from the
/// cached corpus aggregate, and materialize every book's findings into `out`.
/// When inactive (no enabled consumer): drop the substrate's cached products so
/// an edit while it is disabled does no spacing work.
///
/// Mapping fans out; **reduction does not**. Chapter `n + 1` consumes chapter
/// `n`'s boundary state, so a book's reduction is a sequential carry fold — it
/// walks compact cached observations, not text, and stays deterministic.
pub(crate) fn drive_spacing(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<SpacingSubstrate>,
    corpus: &Corpus,
    cfg: &PunctuationSpacingConfig,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    // Reset the per-analyze work probes up front so a disabled substrate reads as
    // zero work (not a stale count from a prior active analyze).
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Spacing);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Planning pass. Stamps are built once and handed to both the seam and the
    // driver; the dirty question is put to the cache with the same predicate the
    // driver reuses by, so the two cannot disagree.
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<SpacingMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    // The dirty work's size, for the seam's route decision only — summing string
    // lengths already known, reading no text.
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp {
                schema_stamp: SpacingSubstrate::SCHEMA_STAMP,
                chapter_hash: c.hash,
                extractor_fp: SpacingSubstrate::extractor_fp(&()),
            };
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(SpacingMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView {
                        chapter: &c.chapter,
                        texts: verses,
                    },
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
        SpacingSubstrate::map_chapter(&w.view, &(), &())
    });
    // Back into caller-order `(book, chapter)` slots. Reduction reads them in
    // corpus order, never completion order, so serial and parallel builds — and
    // any thread count — produce identical reductions.
    let mut slots: Vec<Vec<Option<SpacingChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| {
            // Pre-mapped above. The planning pass asked the cache the same
            // question the driver asks, so this slot is filled whenever the driver
            // wants it; mapping in place is the correct answer if it ever is not.
            slots[bi][i].take().unwrap_or_else(|| {
                let c = &book.chapters[i];
                SpacingSubstrate::map_chapter(
                    &ChapterView {
                        chapter: &c.chapter,
                        texts: &texts[c.range.clone()],
                    },
                    &(),
                    &(),
                )
            })
        });
    }
    probe.mark(DrivePhase::Reduce);
    let floor = spacing_floor(cfg);
    let stats = cache.corpus_stats();
    // No key-discovery phase: spacing's judge key set IS the aggregate's key set
    // (one mark per cell), so there is nothing to reconstruct from the sites.
    let verdicts: BTreeMap<char, MarkVerdict> = stats
        .totals
        .keys()
        .copied()
        .map(|m| (m, SpacingSubstrate::judge(cfg, &m, stats)))
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in corpus.book_layout() {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.chapters, &verdicts, floor, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// `punct.spacing-anomaly` findings for a whole corpus at a given config, via
/// the observation substrate over a fresh transient cache — the single spacing
/// implementation, for calibration/survey callers that used to construct the
/// retired `PunctuationSpacingAnomaly` rule directly. Findings are in the final
/// stable order (`(key_idx, range.start, range.end)`), as the shipped rule
/// returned them.
pub fn spacing_findings(corpus: &Corpus, cfg: &PunctuationSpacingConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_spacing(true, &mut cache, corpus, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

/// The corpus per-mark spacing cells (summed over books) the substrate builds —
/// the authority the census's `MarkSpacing` lane is validated against. Cells are
/// a pure function of the text, so any config produces the same aggregate.
#[cfg(test)]
pub(crate) fn spacing_corpus_cells(corpus: &Corpus) -> BTreeMap<char, [u64; SIDE_CELLS]> {
    let mut cache: crate::substrate::SubstrateCache<SpacingSubstrate> =
        crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_spacing(
        true,
        &mut cache,
        corpus,
        &PunctuationSpacingConfig::default(),
        &mut out,
    );
    cache.corpus_stats().totals.clone()
}

impl SpacingBookContribution {
    /// Materialize this book's spacing findings from its keyed sites and the
    /// judged per-mark verdicts, rebasing each chapter-local site to a global
    /// `KeyIdx` via its chapter's current base. Sites are visited in book order
    /// (the streaming-walk order), so the identical-span tie the final stable
    /// sort preserves is reproduced. Shares [`spacing_finding_for_site`] with the
    /// aggregate-only path, so the two cannot drift.
    pub(crate) fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        verdicts: &BTreeMap<char, MarkVerdict>,
        floor: f64,
        out: &mut Vec<Finding>,
    ) {
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for ((token, sites), block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, token);
            for s in sites {
                if let Some(v) = verdicts.get(&s.mark)
                    && let Some(f) = spacing_finding_for_site(
                        v,
                        floor,
                        rebase(base, s.local_idx),
                        s.mark,
                        s.left,
                        s.right,
                        s.left_span,
                        s.right_span,
                    )
                {
                    out.push(f);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::by_book;

    /// The streaming listener's cross-seam state (carried left edge, the
    /// at-most-one buffered seam-right opportunity) must reproduce the batch
    /// walk (`for_each_spacing_opportunity`, which pre-computes every verse's
    /// edges) exactly — same per-mark cells, same sites in the same order.
    /// Covers: mark at verse end resolved by the next verse, empty and
    /// whitespace-only verses between them, a leading mark reading the
    /// previous verse's class across the seam, and a trailing mark at book
    /// end (abstain).
    #[test]
    fn streaming_spacing_walk_equals_batch_walk() {
        let books: &[&[&str]] = &[
            &["word,", "next words."],
            &["end.", "", "   ", "Next verse!"],
            &["a ,", ")", "7", ". lead", "tail ."],
            &["", "only."],
            &[".", ",", "!"],
        ];
        for (bi, verses) in books.iter().enumerate() {
            let entries: Vec<(u16, String)> = verses
                .iter()
                .enumerate()
                .map(|(i, t)| ((i + 1) as u16, t.to_string()))
                .collect();
            let corpus = book("GEN", &entries);
            let groups = by_book(&corpus);
            let group = &groups[0];

            // Batch reference.
            type RefSite = (
                LocalKeyIdx,
                char,
                Option<SideRead>,
                Option<SideRead>,
                Span,
                Span,
            );
            let mut ref_cells: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
            let mut ref_sites: Vec<RefSite> = Vec::new();
            for_each_spacing_opportunity(group, |local, opp| {
                let cell = ref_cells.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
                if let Some(r) = opp.left {
                    cell[cell_index(Side::Left, r.class, r.form)] += 1;
                }
                if let Some(r) = opp.right {
                    cell[cell_index(Side::Right, r.class, r.form)] += 1;
                }
                ref_sites.push((
                    local,
                    opp.mark,
                    opp.left,
                    opp.right,
                    opp.left_span,
                    opp.right_span,
                ));
            });

            // Streaming listener over the same verses.
            let (book_stats, sites) = crate::stream::drive_book(
                group,
                crate::stream::Needs {
                    graphemes: true,
                    ..Default::default()
                },
                SpacingAcc::new(),
                |a, v| a.verse(v),
                SpacingAcc::finish,
            );
            let got_sites: Vec<_> = sites
                .iter()
                .map(|s| {
                    (
                        s.local_idx,
                        s.mark,
                        s.left,
                        s.right,
                        s.left_span,
                        s.right_span,
                    )
                })
                .collect();
            assert_eq!(book_stats.per_mark, ref_cells, "cells for book #{bi}");
            assert_eq!(got_sites, ref_sites, "sites for book #{bi}");
        }
    }

    // These first tests pin the *candidate extraction* (`adjacency_candidates`)
    // — the spans that enter stats. They are the old deterministic rule's tests,
    // now testing which runs become candidates rather than which are verdicts:
    // extraction is deliberately unchanged while the verdict model moved to
    // corpus-relative scoring.
    fn tp(text: &str) -> Vec<TapeEntry> {
        let mut v = Vec::new();
        crate::tape::build(text, &mut v);
        v
    }
    fn rp(text: &str) -> Vec<&str> {
        adjacency_candidates(&tp(text))
            .iter()
            .map(|s| s.slice(text))
            .collect()
    }

    #[test]
    fn identical_punct_runs_are_candidates() {
        assert_eq!(rp("wait,, what"), vec![",,"]);
        assert_eq!(rp("end.. next"), vec![".."]);
        assert_eq!(rp("a ;; b"), vec![";;"]);
    }

    #[test]
    fn ellipsis_and_double_dash_excluded_from_candidates() {
        assert!(rp("wait... what").is_empty());
        assert!(rp("a -- b").is_empty());
        // But four dots / three dashes are not the known-safe set.
        assert_eq!(rp("wait.... what"), vec!["...."]);
        assert_eq!(rp("a --- b"), vec!["---"]);
    }

    #[test]
    fn interrobang_excluded_mixed_runs_are_candidates() {
        assert!(rp("what?! yes").is_empty());
        assert!(rp("what!? yes").is_empty());
        assert_eq!(rp("what?!? yes"), vec!["?!?"]);
        assert_eq!(rp("end., next"), vec![".,"]);
    }

    #[test]
    fn quotes_next_to_punct_are_clean() {
        assert!(rp("he said, \"go.\" then").is_empty());
        assert!(rp("«word», said he.").is_empty());
    }

    #[test]
    fn doubled_quotes_are_convention_not_typo() {
        // es-419 ULB writes '' for a double quote and "" at nested
        // closes, corpus-wide. Quote chars are exempt from identical-run
        // detection.
        assert!(rp("dijo: ''Denle a la mujer.''").is_empty());
        assert!(rp("una casa de cedro?\"\"").is_empty());
    }

    #[test]
    fn ellipsis_before_quote_is_clean() {
        assert!(rp("trailing...\" he said").is_empty());
    }

    // ── stateful score behaviour ────────────────────────────────────────

    /// Build a single-book `Corpus`: verse `n` becomes the wire key
    /// `"{book} 1:n"` — chapter fixed at 1, mirroring the old `Sid::new(book,
    /// 1, n)` shape (`n` is just an opaque distinguishing label, not parsed).
    fn book(bk: &str, verses: &[(u16, String)]) -> Corpus {
        let keys = verses.iter().map(|(v, _)| format!("{bk} 1:{v}")).collect();
        let texts = verses.iter().map(|(_, t)| t.clone()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }
    /// The wire key for one `(book, verse)` pair under this file's
    /// `chapter=1` convention — the identity-lookup counterpart to `book()`,
    /// replacing the old `sid()` helper now that a `Finding` carries a
    /// `KeyIdx` (resolved through its originating `Corpus`) instead of a `Sid`.
    fn key_of(bk: &str, v: u16) -> String {
        format!("{bk} 1:{v}")
    }
    /// Build a multi-book `Corpus` from `(book, verses)` blocks laid out in
    /// the given order — the `Corpus` contiguous-book-block invariant, so a
    /// test that wants to extend one book's verses (e.g. add more `,,`
    /// occurrences to GEN) does so on the `Vec` *before* calling this, not by
    /// mutating a built `Corpus`.
    fn build_books(entries: &[(&str, Vec<(u16, String)>)]) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for (bk, verses) in entries {
            for (v, t) in verses {
                keys.push(format!("{bk} 1:{v}"));
                texts.push(t.clone());
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }
    /// The knobs, as the substrate's judge config. (`rule`/`default_rule` used
    /// to build the retired `StatefulRule`; the config *is* the rule now.)
    fn rule(cfg: PunctuationAdjacencyConfig) -> PunctuationAdjacencyConfig {
        cfg
    }
    fn default_rule() -> PunctuationAdjacencyConfig {
        PunctuationAdjacencyConfig::default()
    }
    fn no_floor() -> PunctuationAdjacencyConfig {
        PunctuationAdjacencyConfig {
            emit_score_min: 0.0,
            ..Default::default()
        }
    }
    /// Every adjacency test runs the shipped substrate over a fresh transient
    /// cache — the one adjacency implementation.
    fn run(corpus: &Corpus, cfg: &PunctuationAdjacencyConfig) -> Vec<Finding> {
        adjacency_findings(corpus, cfg)
    }
    /// The `N_start` count for one glyph over a verse (for structural asserts).
    fn n_start(text: &str, glyph: char) -> u64 {
        let mut lead = BTreeMap::new();
        count_lead_opportunities(&tp(text), &mut lead);
        lead.get(&glyph).copied().unwrap_or(0)
    }
    /// Score of the pattern occurrence at a given key, if emitted. `corpus`
    /// resolves each finding's `key_idx` back to its wire key — it must be
    /// the same `Corpus` `f` was judged against.
    fn score_at(corpus: &Corpus, f: &[Finding], key: &str) -> Option<f32> {
        f.iter()
            .find(|x| corpus.key(x.key_idx) == key)
            .and_then(|x| x.score)
    }

    /// Entries for `clean` plain-period verses (2 period run-starts each, no
    /// candidates) to establish a large `N_start('.')`, plus `commas` `.,`
    /// verses — as a plain `Vec` so a test can extend it (e.g. append more
    /// patterns) before building the `Corpus`, which is immutable once built.
    fn periods_and_commas_entries(clean: usize, commas: usize) -> Vec<(u16, String)> {
        let mut v: Vec<(u16, String)> = (1..=clean as u16)
            .map(|i| (i, "He said. She left.".to_string()))
            .collect();
        for j in 0..commas {
            v.push((1000 + j as u16, "word., word".to_string()));
        }
        v
    }
    fn periods_and_commas(clean: usize, commas: usize) -> Corpus {
        book("GEN", &periods_and_commas_entries(clean, commas))
    }

    #[test]
    fn rare_mixed_pattern_among_many_period_starts_stays_high() {
        // Five `.,` among ~400 period run-starts: a sliver of its lead glyph's
        // opportunities, so it stays near-certain anomaly and clears the floor.
        let vm = periods_and_commas(200, 5);
        let f = run(&vm, &default_rule());
        assert_eq!(f.len(), 5, "all five `.,` sites surface");
        for site in &f {
            assert_eq!(site.severity, Severity::Info);
            assert!(site.score.unwrap() > 0.9, "score {:?}", site.score);
        }
    }

    #[test]
    fn adding_occurrences_of_a_pattern_lowers_its_evidence() {
        // Realizable move: more `.,` raises both k(`.,`) and N_start('.'). Since
        // N_start ≥ k always, the pattern's evidence weakly falls.
        let few_vm = periods_and_commas(200, 5);
        let many_vm = periods_and_commas(200, 50);
        let few = run(&few_vm, &rule(no_floor()));
        let many = run(&many_vm, &rule(no_floor()));
        let e_few = score_at(&few_vm, &few, &key_of("GEN", 1000)).unwrap();
        let e_many = score_at(&many_vm, &many, &key_of("GEN", 1000)).unwrap();
        assert!(
            e_many <= e_few,
            "50× evidence {e_many} must not exceed 5× {e_few}"
        );
        assert!(
            e_many < e_few,
            "and here it strictly falls: {e_many} < {e_few}"
        );
    }

    #[test]
    fn a_common_same_lead_pattern_does_not_drag_down_a_rare_one() {
        // Inject many `..` (same lead glyph '.') alongside the rare `.,`. The
        // `..` denominator grows, so the rare `.,` stays high while `..` itself
        // drops — patterns sharing a lead glyph compete for one opportunity
        // pool but are scored independently.
        let mut entries = periods_and_commas_entries(200, 5);
        for j in 0..100u16 {
            entries.push((2000 + j, "end.. next".to_string()));
        }
        let vm = book("GEN", &entries);
        let f = run(&vm, &rule(no_floor()));
        let rare = score_at(&vm, &f, &key_of("GEN", 1000)).unwrap(); // a `.,`
        let common = score_at(&vm, &f, &key_of("GEN", 2000)).unwrap(); // a `..`
        assert!(rare > 0.9, "rare `.,` stays high: {rare}");
        assert!(
            common < rare,
            "common `..` {common} scores below rare `.,` {rare}"
        );
    }

    #[test]
    fn dominant_doubled_convention_falls_below_floor() {
        // An Ethiopic corpus that doubles ፤ as its sentence separator corpus-
        // wide: `፤፤` is ~all of ፤'s run-starts, so it is learned as convention
        // and emits nothing at the default floor.
        let verses: Vec<(u16, String)> = (1..=100).map(|v| (v, "ግፅ፤፤ ግፅ፤፤".to_string())).collect();
        let vm = book("GEN", &verses);
        assert!(
            run(&vm, &default_rule()).is_empty(),
            "dominant ፤፤ must be silent"
        );
        // And the same for a doubled Arabic full stop `۔۔`.
        let ar: Vec<(u16, String)> = (1..=100)
            .map(|v| (v, "كلمة۔۔ كلمة۔۔".to_string()))
            .collect();
        assert!(
            run(&book("GEN", &ar), &default_rule()).is_empty(),
            "dominant ۔۔ must be silent"
        );
    }

    #[test]
    fn exact_run_lengths_are_distinct_patterns_one_event_each() {
        // Each maximal run is one candidate; the exact strings stay distinct,
        // and one long run is a single event (not one-per-adjacent-pair).
        assert_eq!(rp("a!! b"), vec!["!!"]);
        assert_eq!(rp("c!!! d"), vec!["!!!"]);
        assert_eq!(rp("e!!!! f"), vec!["!!!!"]);
        // A `?`-run of 3+ is `hyg.replacement-run`'s finding (encoding
        // damage), not an adjacency candidate; `??` still is one.
        assert_eq!(rp("a?? b"), vec!["??"]);
        assert!(rp("c??? d").is_empty());
        // A run counts once toward its lead's N_start, regardless of length.
        assert_eq!(n_start("e!!!! f", '!'), 1);
    }

    // (The single-formula "no k=4/k=5 discontinuity" property is a pure
    // `strength` fact, unit-tested in `crate::shrinkage`.)

    #[test]
    fn exclusive_lead_glyph_pattern_is_governed_by_confidence_z() {
        // A novel `※※` whose lead glyph appears ONLY as `※※` has observed rate
        // pinned at 1.0 (k == N_start), so the *only* thing separating a
        // seen-twice novelty from an entrenched convention is the confidence
        // lower bound — evidence ≈ 0.32 at 2×, *falling* to ≈0.12 at 3× (more
        // occurrences read as "more established"). This lands in the same
        // moderate band as a real moderate-frequency convention (Arabic `۔۔`
        // ≈ 0.48), so the default floor (0.5) silences it *by design* — corpus
        // counts can't tell a novelty from a convention at that score. A
        // consumer who wants low-evidence novelties lowers `emit_score_min`.
        let novelty = |n: u16| {
            let v: Vec<(u16, String)> = (1..=n).map(|i| (i, "word ※※ word".to_string())).collect();
            book("GEN", &v)
        };

        // Observed rate is exactly 1.0: `※` only ever begins a `※※` run, so
        // each verse contributes k=1 (a `※※` candidate) and N_start('※')=1.
        assert_eq!(rp("word ※※ word"), vec!["※※"]);
        assert_eq!(n_start("word ※※ word", '※'), 1);

        // Evidence FALLS as the exclusive pattern recurs (more confident it is a
        // convention) — the opposite of a common-glyph pattern.
        let n2 = novelty(2);
        let n3 = novelty(3);
        let n20 = novelty(20);
        let e2 = score_at(&n2, &run(&n2, &rule(no_floor())), &key_of("GEN", 1)).unwrap();
        let e3 = score_at(&n3, &run(&n3, &rule(no_floor())), &key_of("GEN", 1)).unwrap();
        let e20 = score_at(&n20, &run(&n20, &rule(no_floor())), &key_of("GEN", 1)).unwrap();
        assert!(
            e2 > e3 && e3 > e20,
            "exclusive-glyph evidence falls with count: {e2},{e3},{e20}"
        );

        // z is the load-bearing knob: raising it (more shrinkage) raises the
        // novelty's evidence; z=0 (no shrinkage, observed rate 1.0) suppresses.
        let with_z = |z: f32| {
            let cfg = PunctuationAdjacencyConfig {
                confidence_z: z,
                emit_score_min: 0.0,
                ..Default::default()
            };
            let n3 = novelty(3);
            score_at(&n3, &run(&n3, &rule(cfg)), &key_of("GEN", 1)).unwrap()
        };
        assert_eq!(
            with_z(0.0),
            0.0,
            "no shrinkage ⇒ rate 1.0 ⇒ fully conventional"
        );
        assert!(
            with_z(3.0) > with_z(1.96),
            "more shrinkage raises the novelty's evidence"
        );

        // At the default floor (0.5) the exclusive-glyph novelty is silent
        // (0.32 < 0.5) — the documented, tunable tradeoff — while a
        // well-evidenced common-glyph rarity always surfaces.
        assert!(
            run(&novelty(2), &default_rule()).is_empty(),
            "2× exclusive novelty silent at default 0.5"
        );
        assert!(
            !run(&periods_and_commas(200, 5), &default_rule()).is_empty(),
            "common-glyph rarity is not silenced"
        );
        // Exposed as a knob: lowering the floor opts into seeing it.
        let low = PunctuationAdjacencyConfig {
            emit_score_min: 0.25,
            ..Default::default()
        };
        assert!(
            !run(&novelty(2), &rule(low)).is_empty(),
            "lowering emit_score_min surfaces the novelty"
        );
    }

    #[test]
    fn quotes_and_brackets_do_not_enter_the_candidate_domain() {
        // Quote runs and lone brackets never become candidates, so they never
        // enter the aggregates or emit.
        let text = "(word) [x] ''y'' \"\"z\"\" said";
        assert!(rp(text).is_empty(), "no spurious quote/bracket candidates");
        assert!(run(&book("GEN", &[(1, text.to_string())]), &default_rule()).is_empty());
    }

    #[test]
    fn spans_multiple_books_corpus_wide() {
        // The rule pools over the whole supplied corpus: a rare `.,` in EXO is
        // scored against period opportunities established across GEN too.
        let full = build_books(&[
            ("GEN", periods_and_commas_entries(200, 0)),
            ("EXO", vec![(1u16, "word., word".to_string())]),
        ]);
        let f = run(&full, &default_rule());
        assert_eq!(f.len(), 1);
        assert_eq!(full.key(f[0].key_idx), key_of("EXO", 1));
        assert!(f[0].score.unwrap() > 0.9);
    }

    #[test]
    fn every_above_floor_occurrence_is_emitted_no_cap() {
        // No cap (the old lossy 512 cap is gone): a rare pattern that recurs
        // *more than 512 times* still emits a finding for every occurrence.
        // 600 `.,` among ~2400 period run-starts stays anomalous (≈0.53).
        let vm = periods_and_commas(900, 600);
        let f = run(&vm, &default_rule());
        assert_eq!(
            f.len(),
            600,
            "all 600 `.,` occurrences surface — no 512 cap"
        );
    }

    /// A resident drive over one corpus, findings in the final stable order —
    /// the incremental path, as `analyze` runs it.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<AdjacencySubstrate>,
        corpus: &Corpus,
        cfg: &PunctuationAdjacencyConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_adjacency(true, cache, corpus, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span text, score, and all four arg counts, so
    /// an equal-length-but-wrong result cannot pass.
    fn render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::AdjacencyEvidence {
                        pattern,
                        k,
                        lead_n,
                        books,
                        corpus: c,
                    }) => format!("{pattern}/{k}/{lead_n}/{books}/{c}"),
                    _ => "-".to_string(),
                };
                format!(
                    "{}|{}|{:?}|{a}",
                    corpus.key(f.key_idx),
                    f.range.slice(corpus.text(f.key_idx)),
                    f.score
                )
            })
            .collect()
    }

    #[test]
    fn incremental_scores_are_corpus_wide_not_book_local() {
        // The point of the aggregate: an edit to EXO scores its `.,` against the
        // CORPUS-wide period opportunities (mostly GEN's), not the book-local
        // rate a stateless project rule would give. Driven residently, so the
        // aggregate is the incrementally maintained one.
        let cfg = default_rule();
        let gen_entries = periods_and_commas_entries(200, 0); // GEN: ~400 period starts
        let clean = vec![(1u16, "word word".to_string())];
        let edited = vec![(1u16, "word., word".to_string())]; // one rare `.,`
        let before = build_books(&[("GEN", gen_entries.clone()), ("EXO", clean)]);
        let after = build_books(&[("GEN", gen_entries), ("EXO", edited)]);

        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(&mut cache, &before, &cfg);
        assert!(seeded.is_empty(), "{:?}", render(&before, &seeded));

        cache.reset_probes();
        let inc = resident(&mut cache, &after, &cfg);
        assert_eq!(
            cache.mapped, 1,
            "only EXO's one changed chapter is remapped"
        );
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(inc.len(), 1, "{:?}", render(&after, &inc));
        assert_eq!(after.key(inc[0].key_idx), key_of("EXO", 1));
        // The whole claim: identical to a cold analysis of the same corpus,
        // args and score included — a book-local rate would score far lower.
        assert_eq!(render(&after, &inc), render(&after, &run(&after, &cfg)));
        assert!(
            inc[0].score.unwrap() > 0.9,
            "corpus-wide rate keeps the lone `.,` anomalous"
        );

        // An unchanged re-drive maps and reduces nothing and says the same thing.
        cache.reset_probes();
        let again = resident(&mut cache, &after, &cfg);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert_eq!(render(&after, &again), render(&after, &inc));
    }

    /// A judging-knob change maps and reduces ZERO chapters: every
    /// `PunctuationAdjacencyConfig` field is read at judge, so the cached
    /// observations and reductions are all still valid (plan §12.4).
    #[test]
    fn a_knob_change_maps_and_reduces_nothing() {
        let vm = periods_and_commas(50, 5);
        let mut cache = crate::substrate::SubstrateCache::new();
        let strict = resident(&mut cache, &vm, &default_rule());
        cache.reset_probes();
        let loose = resident(&mut cache, &vm, &no_floor());
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
        assert!(
            loose.len() >= strict.len(),
            "dropping the floor cannot lose findings"
        );
        assert_eq!(render(&vm, &loose), render(&vm, &run(&vm, &no_floor())));
    }

    /// Randomized edits: a resident cache's findings always equal a cold
    /// analysis of the same corpus (plan §12.6).
    #[test]
    fn resident_adjacency_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "word, word",
            "word,, word",
            "word.. word",
            "word ... word",
            "word?! word",
            "",
            "word.,word",
            "word; word",
        ];
        let books = ["GEN", "EXO", "LEV"];
        let mut entries: Vec<(&str, Vec<(u16, String)>)> = books
            .iter()
            .map(|b| {
                (
                    *b,
                    (1u16..=12)
                        .map(|v| (v, SHAPES[(v as usize) % SHAPES.len()].to_string()))
                        .collect(),
                )
            })
            .collect();
        let build = |e: &[(&str, Vec<(u16, String)>)]| {
            build_books(
                &e.iter()
                    .map(|(b, v)| (*b, v.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        let cfg = no_floor();
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &build(&entries), &cfg);
        // A deterministic pseudo-random walk (no dev-dep on a RNG crate).
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bi = (state >> 33) as usize % entries.len();
            let vi = (state >> 17) as usize % entries[bi].1.len();
            let si = (state >> 5) as usize % SHAPES.len();
            entries[bi].1[vi].1 = SHAPES[si].to_string();
            let corpus = build(&entries);
            let inc = resident(&mut cache, &corpus, &cfg);
            assert_eq!(
                render(&corpus, &inc),
                render(&corpus, &run(&corpus, &cfg)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    #[test]
    fn invalid_config_produces_finite_scores_not_nan() {
        let vm = periods_and_commas(50, 5);
        let bad = PunctuationAdjacencyConfig {
            convention_rate: f32::NAN,
            confidence_z: -3.0,
            breadth_convention_rate: f32::NAN,
            breadth_z: f32::NEG_INFINITY,
            breadth_min_books: 0,
            length_gain_slope: f32::NAN,
            emit_score_min: f32::NAN,
        };
        for f in run(&vm, &rule(bad)) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    // ── breadth + length composition (ADR 0031) ─────────────────────────

    /// Ten real book codes so a synthetic corpus can clear the 8-book breadth
    /// gate and exercise dispersion.
    const TEN_BOOKS: [&str; 10] = [
        "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA",
    ];

    /// Per-book entries for `TEN_BOOKS`, each with 40 `a, b, c, d` filler verses
    /// (a big `N_start(',')`); the first `carriers` books additionally carry
    /// three `x,, y` verses. So `,,` is a tiny share of comma opportunities
    /// (frequency stays ≈0) but its book-breadth is `carriers/10`. Returned as
    /// per-book entry lists (rather than a built `Corpus`) so a test can
    /// extend one book's entries — the `Corpus` itself, once built, is
    /// immutable and requires contiguous per-book blocks.
    fn commas_in_n_books_entries(carriers: usize) -> Vec<(&'static str, Vec<(u16, String)>)> {
        TEN_BOOKS
            .iter()
            .enumerate()
            .map(|(bi, bk)| {
                let mut v: Vec<(u16, String)> =
                    (1..=40u16).map(|v| (v, "a, b, c, d".to_string())).collect();
                if bi < carriers {
                    v.extend((100..=102u16).map(|v| (v, "x,, y".to_string())));
                }
                (*bk, v)
            })
            .collect()
    }
    fn commas_in_n_books(carriers: usize) -> Corpus {
        build_books(&commas_in_n_books_entries(carriers))
    }

    #[test]
    fn breadth_alone_suppresses_a_widespread_low_frequency_pattern() {
        // `,,` is a sliver of all `,` run-starts (frequency evidence ≈ 1) yet
        // spans 8/10 books: dispersion alone establishes it as a convention.
        // This is the `ayn ۔۔۔` shape the multiplicative model got wrong.
        assert!(
            run(&commas_in_n_books(8), &default_rule()).is_empty(),
            "widespread low-frequency `,,` must suppress on breadth alone"
        );
    }

    #[test]
    fn a_concentrated_pattern_of_equal_count_still_surfaces() {
        // Same total `,,` count as the spread case, but all in one book: low
        // breadth (1/10) cannot establish it, so it stays anomalous. Isolates
        // breadth from frequency (k and N_start are ~equal to the spread case).
        let mut entries = commas_in_n_books_entries(0); // filler only, no carriers
        entries[0]
            .1
            .extend((100..=123u16).map(|v| (v, "x,, y".to_string()))); // 24 `,,` in GEN (book 0)
        let vm = build_books(&entries);
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "concentrated `,,` (1/10 books) must still surface"
        );
    }

    #[test]
    fn breadth_gate_is_off_below_min_books() {
        // The identical widespread-low-frequency `,,`, but in a 5-book corpus
        // (< the 8-book gate): dispersion is not consulted, so frequency alone
        // governs and the rare pattern surfaces.
        let entries: Vec<(&str, Vec<(u16, String)>)> = TEN_BOOKS[..5]
            .iter()
            .map(|bk| {
                let mut v: Vec<(u16, String)> =
                    (1..=40u16).map(|v| (v, "a, b, c, d".to_string())).collect();
                v.extend((100..=102u16).map(|v| (v, "x,, y".to_string())));
                (*bk, v)
            })
            .collect();
        let vm = build_books(&entries);
        assert!(
            !run(&vm, &default_rule()).is_empty(),
            "below the book gate, breadth must not suppress — frequency governs"
        );
    }

    #[test]
    fn frequency_alone_suppresses_a_narrow_but_dominant_pattern() {
        // Ten books, but `::` occurs in only ONE — where `:` appears *only* as
        // `::` (observed rate 1.0). Frequency establishes it despite breadth
        // 1/10. This is the `bji ::` shape the multiplicative model got wrong.
        let mut entries = commas_in_n_books_entries(0);
        entries[0]
            .1
            .extend((200..=239u16).map(|v| (v, "word:: next".to_string())));
        let vm = build_books(&entries);
        let colon_findings: Vec<_> = run(&vm, &default_rule())
            .into_iter()
            .filter(|f| f.range.slice(vm.text(f.key_idx)).contains(':'))
            .collect();
        assert!(
            colon_findings.is_empty(),
            "narrow but dominant `::` must suppress on frequency alone: {colon_findings:?}"
        );
    }

    #[test]
    fn length_amplifies_a_longer_identical_run() {
        // At equal frequency footing (one occurrence each, shared `!` pool) and
        // no breadth (single book), a longer identical run scores strictly above
        // a doubling — nothing but the ellipsis is legitimately tripled.
        let mut v: Vec<(u16, String)> =
            (1..=200).map(|i| (i, "why! really!".to_string())).collect(); // N_start('!')
        v.push((900, "a!! b".to_string()));
        v.push((901, "c!!!! d".to_string()));
        let vm = book("GEN", &v);
        let f = run(&vm, &rule(no_floor()));
        let two = score_at(&vm, &f, &key_of("GEN", 900)).unwrap();
        let four = score_at(&vm, &f, &key_of("GEN", 901)).unwrap();
        assert!(
            four > two,
            "longer run scores higher: !!!!={four} > !!={two}"
        );
    }

    // ── punct spacing anomaly — pooled class-conditioned model (ADR 0054 2nd) ─

    fn sp_default() -> PunctuationSpacingConfig {
        PunctuationSpacingConfig::default()
    }
    fn sp_no_floor() -> PunctuationSpacingConfig {
        PunctuationSpacingConfig {
            emit_score_min: 0.0,
            ..Default::default()
        }
    }
    /// `punct.spacing-anomaly` findings for a corpus — through the observation
    /// substrate (the sole spacing implementation now).
    fn sp_run(corpus: &Corpus, cfg: &PunctuationSpacingConfig) -> Vec<Finding> {
        spacing_findings(corpus, cfg)
    }
    /// An isolated verse: both seams are book edges (no cross neighbour), so a
    /// verse-edge mark abstains on the seam side.
    fn opps_of(text: &str) -> Vec<SpacingOpportunity> {
        opps_cross(text, None, None)
    }
    /// A verse with explicit cross-seam neighbour classes (as `for_each_*`
    /// resolves them from book neighbours), to unit-test seam behaviour.
    fn opps_cross(
        text: &str,
        l: Option<SpacingClass>,
        r: Option<SpacingClass>,
    ) -> Vec<SpacingOpportunity> {
        let mut g = Vec::new();
        grapheme::segment(text, &mut g);
        spacing_opportunities(text, &g, l, r)
    }
    fn read(class: SpacingClass, form: SpacingForm) -> Option<SideRead> {
        Some(SideRead { class, form })
    }
    /// Walk a single-book corpus's opportunities in book order (resolving
    /// cross-seam classes), returning `(local_idx, mark, left, right)` per
    /// occurrence.
    fn book_opps(corpus: &Corpus) -> Vec<(LocalKeyIdx, char, Option<SideRead>, Option<SideRead>)> {
        let groups = by_book(corpus);
        let group = &groups[0];
        let mut out = Vec::new();
        for_each_spacing_opportunity(group, |local, opp| {
            out.push((local, opp.mark, opp.left, opp.right))
        });
        out
    }
    /// Build the twelve packed per-mark counters from per-side `[att, sp]` pools
    /// keyed by class (letter, number, punct).
    fn tbl(l: [[u64; 2]; CLASS_COUNT], r: [[u64; 2]; CLASS_COUNT]) -> [u64; SIDE_CELLS] {
        let mut c = [0u64; SIDE_CELLS];
        for (ci, cls) in [SpacingClass::Letter, SpacingClass::Number, SpacingClass::Punct]
            .iter()
            .enumerate()
        {
            c[cell_index(Side::Left, *cls, SpacingForm::Attached)] = l[ci][0];
            c[cell_index(Side::Left, *cls, SpacingForm::Spaced)] = l[ci][1];
            c[cell_index(Side::Right, *cls, SpacingForm::Attached)] = r[ci][0];
            c[cell_index(Side::Right, *cls, SpacingForm::Spaced)] = r[ci][1];
        }
        c
    }
    /// Entries for the English attach-comma corpus: `attached` verses `"word,
    /// word"` (comma reads attached-left / spaced-right, both in the Letter
    /// pool) and `spaced` verses `"word , word"` (a space-before slip —
    /// spaced-left in the Letter pool). A plain `Vec` so a test can combine it
    /// with another book's entries before building the `Corpus`.
    fn commas_entries(attached: usize, spaced: usize) -> Vec<(u16, String)> {
        let mut v: Vec<(u16, String)> = Vec::new();
        let mut n = 1u16;
        for _ in 0..attached {
            v.push((n, "word, word".to_string()));
            n += 1;
        }
        for _ in 0..spaced {
            v.push((n, "word , word".to_string()));
            n += 1;
        }
        v
    }
    fn commas(attached: usize, spaced: usize) -> Corpus {
        book("GEN", &commas_entries(attached, spaced))
    }

    // ── side read extraction: class + form ────────────────────────────────

    #[test]
    fn every_separator_mark_is_an_opportunity_on_both_sides() {
        let o = opps_of("word, word");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(o[0].right, read(SpacingClass::Letter, SpacingForm::Spaced));
    }

    #[test]
    fn a_number_neighbour_selects_the_number_pool() {
        // `7.8` decimal: attached to digits both sides ⇒ Number pool, attached.
        // `7. 8` cross-reference: attached-left, spaced-right, SAME Number pool.
        let dec = opps_of("7.8");
        assert_eq!(dec[0].left, read(SpacingClass::Number, SpacingForm::Attached));
        assert_eq!(dec[0].right, read(SpacingClass::Number, SpacingForm::Attached));
        let refr = opps_of("7. 8");
        assert_eq!(refr[0].left, read(SpacingClass::Number, SpacingForm::Attached));
        assert_eq!(refr[0].right, read(SpacingClass::Number, SpacingForm::Spaced));
    }

    #[test]
    fn a_punct_neighbour_selects_the_punct_pool_quote_merged() {
        // `word?!`: the `!` reads punct-left (the `?`) ⇒ Punct pool, attached.
        // Quote merged into Punct: `word."` reads Punct-attached on the right.
        let cluster = opps_of("word?!");
        let bang = cluster.iter().find(|x| x.mark == '!').unwrap();
        assert_eq!(bang.left, read(SpacingClass::Punct, SpacingForm::Attached));
        let quote = opps_of("word.\" then");
        let p = quote.iter().find(|x| x.mark == '.').unwrap();
        assert_eq!(p.left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(p.right, read(SpacingClass::Punct, SpacingForm::Attached));
    }

    #[test]
    fn a_book_edge_side_abstains_but_a_cross_seam_side_reads_across() {
        let edge = opps_of("word.");
        assert_eq!(edge[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(edge[0].right, None, "book-edge trailing mark abstains");
        let crossed = opps_cross("word.", None, Some(SpacingClass::Letter));
        assert_eq!(crossed[0].right, read(SpacingClass::Letter, SpacingForm::Spaced));
        let mid = opps_of("word. word");
        assert_eq!(
            (crossed[0].left, crossed[0].right),
            (mid[0].left, mid[0].right)
        );
    }

    #[test]
    fn cross_seam_class_is_resolved_from_book_neighbours() {
        let vm = book(
            "GEN",
            &[(1, "see verse.".to_string()), (2, "3 fish".to_string())],
        );
        let o = book_opps(&vm);
        let v1_period = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(0) && *m == '.')
            .unwrap();
        assert_eq!(v1_period.3, read(SpacingClass::Number, SpacingForm::Spaced));
        let vm2 = book("GEN", &[(1, "amen".to_string()), (2, ".word".to_string())]);
        let o2 = book_opps(&vm2);
        let lead = o2
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(1) && *m == '.')
            .unwrap();
        assert_eq!(lead.2, read(SpacingClass::Letter, SpacingForm::Spaced));
    }

    #[test]
    fn first_and_last_verse_book_edges_abstain() {
        let vm = book(
            "GEN",
            &[(1, ".open".to_string()), (2, "close.".to_string())],
        );
        let o = book_opps(&vm);
        let lead = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(0) && *m == '.')
            .unwrap();
        assert_eq!(
            lead.2, None,
            "book-initial leading mark abstains on the left"
        );
        let trail = o
            .iter()
            .find(|(s, m, ..)| *s == LocalKeyIdx::from_usize(1) && *m == '.')
            .unwrap();
        assert_eq!(
            trail.3, None,
            "book-final trailing mark abstains on the right"
        );
    }

    #[test]
    fn pd_dashes_are_in_the_candidate_domain() {
        let hy = opps_of("co-operate");
        assert_eq!(hy.len(), 1);
        assert_eq!(hy[0].mark, '-');
        assert_eq!(hy[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
        assert_eq!(hy[0].right, read(SpacingClass::Letter, SpacingForm::Attached));
        let maqaf = opps_of("\u{05D0}\u{05BE}\u{05D1}");
        assert_eq!(maqaf.len(), 1);
        assert_eq!(maqaf[0].mark, '\u{05BE}');
    }

    #[test]
    fn a_mark_carrying_a_combining_cluster_is_excluded() {
        assert!(opps_of("a,\u{0301}b").is_empty());
        let o = opps_of("cafe\u{0301}, then");
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].mark, ',');
        assert_eq!(o[0].left, read(SpacingClass::Letter, SpacingForm::Attached));
    }

    // ── verdict units (pool dominance × rarity, no fallback) ──────────────

    const WIDE_K: f64 = 1.0e9;

    #[test]
    fn dominance_reads_as_the_pool_majority_share_at_z_zero() {
        let v = pool_verdict([25, 75], 0.0, WIDE_K, 0.0, 0.5);
        assert!(v.holds);
        assert!(
            (v.scores[0] - 0.75).abs() < 1e-6,
            "attached score {}",
            v.scores[0]
        );
        assert!(
            (v.scores[1] - 0.25).abs() < 1e-6,
            "spaced score {}",
            v.scores[1]
        );
    }

    #[test]
    fn a_sole_form_pool_is_silent() {
        let v = pool_verdict([40, 0], 1.96, 24.0, 0.0, 0.5);
        assert!(v.holds);
        assert_eq!(v.scores, [0.0, 0.0]);
    }

    #[test]
    fn a_pool_without_a_dominant_convention_does_not_hold() {
        assert!(!pool_verdict([40, 40], 1.96, 24.0, 0.0, 0.5).holds);
        assert!(!pool_verdict([1, 1], 1.96, 24.0, 0.0, 0.5).holds);
        assert!(!pool_verdict([2, 0], 1.96, 24.0, 0.0, 0.5).holds);
    }

    #[test]
    fn rarity_fades_as_a_minority_form_recurs_at_fixed_dominance() {
        let s = |min: u64, maj: u64| pool_verdict([maj, min], 1.96, 24.0, 0.0, 0.5).scores[1];
        let (s1, s8, s500) = (s(1, 200), s(8, 1600), s(500, 100_000));
        assert!(s1 > s8 && s8 > s500, "{s1} {s8} {s500}");
        assert_eq!(s500, 0.0);
    }

    #[test]
    fn the_knee_widens_with_pool_volume() {
        let heavy = pool_verdict([38_000, 17], 1.96, 32.0, 40.0, 0.5).scores[1];
        let thin = pool_verdict([380, 17], 1.96, 32.0, 40.0, 0.5).scores[1];
        let absolute = pool_verdict([38_000, 17], 1.96, 32.0, 0.0, 0.5).scores[1];
        assert!(heavy > 0.85, "heavy {heavy}");
        assert!(thin < heavy, "thin {thin}");
        assert!(absolute < 0.51, "absolute {absolute}");
    }

    #[test]
    fn mark_verdict_splits_the_twelve_counters_into_two_sides_and_three_pools() {
        let counts = tbl([[25, 75], [0, 0], [0, 0]], [[0, 0], [90, 10], [0, 0]]);
        let v = mark_verdict(&counts, 0.0, WIDE_K, 0.0, 0.5);
        assert_eq!(v.left.pools[SpacingClass::Letter.index()].n, 100);
        assert_eq!(v.right.pools[SpacingClass::Number.index()].n, 100);
        assert!((v.left.pools[SpacingClass::Letter.index()].scores[0] - 0.75).abs() < 1e-6);
        assert!((v.right.pools[SpacingClass::Number.index()].scores[1] - 0.90).abs() < 1e-6);
        assert!(!v.left.pools[SpacingClass::Number.index()].holds);
    }

    // ── corpus behaviour ───────────────────────────────────────────────────

    #[test]
    fn a_no_dominant_convention_mark_is_silent() {
        assert!(sp_run(&commas(40, 40), &sp_default()).is_empty());
        assert!(sp_run(&commas(1, 1), &sp_default()).is_empty());
    }

    #[test]
    fn a_sole_form_corpus_is_silent() {
        assert!(sp_run(&commas(40, 0), &sp_default()).is_empty());
        assert!(sp_run(&commas(0, 40), &sp_default()).is_empty());
    }

    #[test]
    fn a_rare_before_side_slip_surfaces_in_the_letter_pool() {
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            assert_eq!(x.severity, Severity::Info);
            assert!(x.score.unwrap() > 0.5);
            match &x.args {
                Some(FindingArgs::SpacingConvention {
                    left: Some(s),
                    right: None,
                    ..
                }) => {
                    assert_eq!(s.form, SpacingForm::Spaced);
                    assert_eq!(s.class, SpacingClass::Letter);
                }
                other => panic!("expected a left-side spaced/letter violation, got {other:?}"),
            }
        }
    }

    #[test]
    fn word_comma_word_missing_space_after_surfaces() {
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(vm.key(f[0].key_idx), key_of("GEN", 200));
        let slip = v
            .iter()
            .find(|(n, _)| *n == 200)
            .map(|(_, t)| t.clone())
            .unwrap();
        assert_eq!(f[0].range.slice(&slip), ",w");
        match &f[0].args {
            Some(FindingArgs::SpacingConvention {
                left: None,
                right: Some(s),
                ..
            }) => {
                assert_eq!(s.form, SpacingForm::Attached);
                assert_eq!(s.class, SpacingClass::Letter);
            }
            other => panic!("expected a right-side attached violation, got {other:?}"),
        }
    }

    #[test]
    fn away_bang_why_after_side_anomaly_surfaces() {
        let mut v: Vec<(u16, String)> = (1..=60).map(|i| (i, "Stop! Go".to_string())).collect();
        v.push((200, "away!Why".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        let bang: Vec<_> = f
            .iter()
            .filter(|x| vm.key(x.key_idx) == key_of("GEN", 200))
            .collect();
        assert_eq!(bang.len(), 1);
    }

    #[test]
    fn spanish_reversed_open_question_mark_surfaces_both_sides() {
        let mut v: Vec<(u16, String)> = (1..=50)
            .map(|i| (i, "espacio \u{00BF}Qué?".to_string()))
            .collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        let hits: Vec<_> = f
            .iter()
            .filter(|x| vm.key(x.key_idx) == key_of("GEN", 100))
            .collect();
        assert_eq!(hits.len(), 1);
        match &hits[0].args {
            Some(FindingArgs::SpacingConvention {
                mark,
                left: Some(l),
                right: Some(r),
            }) => {
                assert_eq!(*mark, '\u{00BF}');
                assert_eq!((l.form, l.class), (SpacingForm::Attached, SpacingClass::Letter));
                assert_eq!((r.form, r.class), (SpacingForm::Spaced, SpacingClass::Letter));
            }
            other => panic!("expected a two-sided SpacingConvention, got {other:?}"),
        }
    }

    // ── dissolved special cases: now judged inside their pools ─────────────

    #[test]
    fn numeric_colon_learns_silent_in_the_number_pool() {
        let v: Vec<(u16, String)> = (1..=100)
            .map(|i| (i, "see 1:1 and 2:2".to_string()))
            .collect();
        assert!(
            sp_run(&book("GEN", &v), &sp_default()).is_empty(),
            "digit-flanked attached colon is silent"
        );

        let mut v2: Vec<(u16, String)> =
            (1..=200).map(|i| (i, "at 1:1 here".to_string())).collect();
        v2.push((300, "at 1: 1 here".to_string()));
        let vm2 = book("GEN", &v2);
        let f = sp_run(&vm2, &sp_default());
        assert_eq!(
            f.iter()
                .filter(|x| vm2.key(x.key_idx) == key_of("GEN", 300))
                .count(),
            1
        );
        assert!(f.iter().all(|x| vm2.key(x.key_idx) == key_of("GEN", 300)));
    }

    #[test]
    fn cluster_tail_learns_silent_in_the_punct_pool() {
        let v: Vec<(u16, String)> = (1..=100)
            .map(|i| (i, "what?! really?!".to_string()))
            .collect();
        assert!(
            sp_run(&book("GEN", &v), &sp_default()).is_empty(),
            "cluster tail is silent by its Punct pool"
        );
    }

    #[test]
    fn medial_period_flags_but_medial_hyphen_is_conventional() {
        let mut v: Vec<(u16, String)> = (1..=120)
            .map(|i| (i, "a end. Next one.".to_string()))
            .collect();
        v.push((300, "a run.together word.".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        assert!(
            f.iter().any(|x| vm.key(x.key_idx) == key_of("GEN", 300)),
            "medial period surfaces"
        );

        let hy: Vec<(u16, String)> = (1..=100)
            .map(|i| (i, "co-operate and re-enter".to_string()))
            .collect();
        assert!(
            sp_run(&book("GEN", &hy), &sp_default()).is_empty(),
            "conventional medial hyphen is silent"
        );
        let mut hy2 = hy.clone();
        hy2.push((300, "a - b co-operate".to_string()));
        let vm2 = book("GEN", &hy2);
        let fh = sp_run(&vm2, &sp_default());
        assert!(
            fh.iter().any(|x| vm2.key(x.key_idx) == key_of("GEN", 300)),
            "lone spaced hyphen surfaces"
        );
    }

    #[test]
    fn a_recurring_minority_goes_silent_as_a_second_convention() {
        assert!(sp_run(&commas(6000, 400), &sp_default()).is_empty());
        let few = sp_run(&commas(1200, 8), &sp_default());
        assert_eq!(few.len(), 8);
    }

    #[test]
    fn clean_as_you_go_raises_the_surviving_slips_score() {
        let score_of = |sp: usize| {
            sp_run(&commas(1000, sp), &sp_no_floor())
                .iter()
                .find_map(|x| match &x.args {
                    Some(FindingArgs::SpacingConvention { left: Some(s), .. })
                        if s.form == SpacingForm::Spaced =>
                    {
                        x.score
                    }
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        let (s12, s3, s1) = (score_of(12), score_of(3), score_of(1));
        assert!(s12 < s3 && s3 < s1, "{s12} < {s3} < {s1}");
    }

    #[test]
    fn spans_point_at_the_spacing_neighborhood() {
        let vm = commas(100, 1);
        let f = sp_run(&vm, &sp_default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].range.slice(vm.text(f[0].key_idx)), " ,");
        let mut v: Vec<(u16, String)> = (1..=100).map(|i| (i, "word, word".to_string())).collect();
        v.push((200, "word,word".to_string()));
        let vm2 = book("GEN", &v);
        let f2 = sp_run(&vm2, &sp_default());
        assert_eq!(f2[0].range.slice(vm2.text(f2[0].key_idx)), ",w");
    }

    #[test]
    fn both_sides_span_is_the_union() {
        let mut v: Vec<(u16, String)> = (1..=50)
            .map(|i| (i, "espacio \u{00BF}Qué?".to_string()))
            .collect();
        v.push((100, "así\u{00BF} no".to_string()));
        let vm = book("GEN", &v);
        let f = sp_run(&vm, &sp_default());
        let hit = f
            .iter()
            .find(|x| vm.key(x.key_idx) == key_of("GEN", 100))
            .unwrap();
        assert_eq!(hit.range.slice(vm.text(hit.key_idx)), "í\u{00BF} ");
    }

    #[test]
    fn finding_carries_the_side_form_class_and_counts() {
        let f = sp_run(&commas(100, 3), &sp_default());
        assert_eq!(f.len(), 3);
        for x in &f {
            match &x.args {
                Some(FindingArgs::SpacingConvention {
                    mark,
                    left: Some(s),
                    right: None,
                }) => {
                    assert_eq!(*mark, ',');
                    assert_eq!(s.form, SpacingForm::Spaced);
                    assert_eq!(s.class, SpacingClass::Letter);
                    assert_eq!((s.count, s.total), (3, 103));
                }
                other => panic!("expected a left-side SpacingConvention, got {other:?}"),
            }
        }
    }

    // ── stateful: corpus-wide pooling, incrementality, removal ───────────

    #[test]
    fn spacing_score_is_corpus_wide_and_incremental() {
        // The substrate judges every mark from the corpus aggregate, and its
        // resident cache reaches that aggregate incrementally: seed GEN, then add
        // EXO's rare `word,word` on the SAME cache, and the EXO finding is scored
        // against the corpus-wide comma opportunities — byte-identical to a cold
        // analysis of the whole corpus.
        use crate::substrate::SubstrateCache;
        let cfg = sp_default();
        let gen_entries = commas_entries(100, 0);
        let exo_entries = vec![(1u16, "word,word".to_string())];
        let gen_only = book("GEN", &gen_entries);
        let full = build_books(&[("GEN", gen_entries), ("EXO", exo_entries)]);

        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();
        let mut seed = Vec::new();
        drive_spacing(true, &mut cache, &gen_only, &cfg, &mut seed);
        let mut inc = Vec::new();
        drive_spacing(true, &mut cache, &full, &cfg, &mut inc);
        inc.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));

        let cold = spacing_findings(&full, &cfg);
        assert_eq!(inc, cold, "resident incremental == cold full analysis");
        assert!(
            inc.iter().any(|f| full.key(f.key_idx) == key_of("EXO", 1)),
            "the EXO comma surfaces against the corpus-wide convention"
        );
    }

    #[test]
    fn removing_a_book_drops_its_substrate_contribution() {
        // GEN establishes the spaced-comma convention that makes EXO's attached
        // `word,word` anomalous; dropping GEN from the substrate cache removes
        // that convention, so EXO no longer fires — the corpus aggregate is
        // maintained through `SubstrateCache::remove_book`.
        use crate::substrate::SubstrateCache;
        let cfg = sp_default();
        let gen_entries = commas_entries(100, 0);
        let exo_entries = vec![
            (1u16, "word,word".to_string()),
            (2u16, "word, word".to_string()),
        ];
        let exo = book("EXO", &exo_entries);
        let full = build_books(&[("GEN", gen_entries), ("EXO", exo_entries)]);

        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();
        let mut before = Vec::new();
        drive_spacing(true, &mut cache, &full, &cfg, &mut before);
        assert!(
            before.iter().any(|f| full.key(f.key_idx) == key_of("EXO", 1)),
            "EXO's attached comma fires against GEN's spaced-comma convention"
        );

        cache.remove_book("GEN");
        // Re-judge EXO alone against the now GEN-free aggregate.
        let mut after = Vec::new();
        drive_spacing(true, &mut cache, &exo, &cfg, &mut after);
        assert!(
            after.iter().all(|f| exo.key(f.key_idx) != key_of("EXO", 1)),
            "with GEN's convention gone, EXO's comma no longer surfaces"
        );
    }

    #[test]
    fn invalid_config_produces_finite_scores() {
        let cfg = PunctuationSpacingConfig {
            emit_score_min: f32::NAN,
            confidence_z: f32::INFINITY,
            minority_recurrence_k: f32::NAN,
            minority_rate_per_10k: f32::NAN,
        };
        for f in sp_run(&commas(100, 3), &cfg) {
            let s = f.score.unwrap();
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "score {s}");
        }
    }

    // ── Spacing observation substrate byte-identity (Phase C step 1) ─────────

    /// Build a multi-book / multi-chapter corpus from explicit `(key, text)`
    /// rows — the substrate byte-identity fixtures need real chapter tokens.
    fn multi(rows: &[(&str, &str)]) -> Corpus {
        let keys = rows.iter().map(|(k, _)| k.to_string()).collect();
        let texts = rows.iter().map(|(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Fold one book's spacing contribution straight from `(chapter token,
    /// verse texts)` specs — map each chapter, carry-reduce left to right, fold —
    /// so a test can compare contributions without going through a `Corpus`.
    /// The materializer's positional zip is truncating, so without the
    /// cardinality assert a missing or extra trailing contribution chapter is
    /// SILENT finding loss, not an error. Witness: two contribution chapters
    /// against a one-chapter layout must fail loud before any pairing.
    #[test]
    #[should_panic(expected = "contribution/layout chapter count mismatch")]
    fn materialize_panics_on_chapter_cardinality_mismatch() {
        let corpus = crate::Corpus::try_from_parts(
            vec!["GEN 1:1".to_string()],
            vec!["a . b".to_string()],
        )
        .unwrap();
        let contrib = contribution_of("GEN", &[("1", &["a . b"]), ("2", &["c . d"])]);
        let book = &corpus.book_layout()[0];
        let mut out = Vec::new();
        contrib.materialize(&book.chapters, &BTreeMap::new(), 0.0, &mut out);
    }

    fn contribution_of(
        _slug: &str,
        chapters: &[(&str, &[&str])],
    ) -> SpacingBookContribution {
        use crate::substrate::{ChapterView, ObservationSubstrate};
        let obs: Vec<SpacingChapterObs> = chapters
            .iter()
            .map(|(token, verses)| {
                let texts: Vec<String> = verses.iter().map(|t| t.to_string()).collect();
                SpacingSubstrate::map_chapter(
                    &ChapterView {
                        chapter: token,
                        texts: &texts,
                    },
                    &(),
                    &(),
                )
            })
            .collect();
        let mut reduced: Vec<SpacingReduced> = Vec::new();
        let mut carry = SpacingBoundary::default();
        for o in &obs {
            let owner = SpacingSubstrate::pending_owner(&carry)
                .and_then(|tok| reduced.iter().position(|r| *r.token == *tok));
            let (this, leaving) = match owner {
                Some(k) => SpacingSubstrate::reduce_chapter(o, &carry, &mut reduced[k]),
                None => {
                    let mut sink = SpacingReduced::default();
                    SpacingSubstrate::reduce_chapter(o, &carry, &mut sink)
                }
            };
            reduced.push(this);
            carry = leaving;
        }
        if let Some(owner) = SpacingSubstrate::pending_owner(&carry)
            .and_then(|tok| reduced.iter().position(|r| *r.token == *tok))
        {
            SpacingSubstrate::finish_book(&carry, &mut reduced[owner]);
        }
        SpacingSubstrate::fold_book(&reduced, &())
    }

    /// Corpus per-mark cells from the INDEPENDENT whole-book batch walk
    /// (`for_each_spacing_opportunity`, which pre-computes every verse's edges
    /// and resolves seams by scanning book neighbours). The substrate's
    /// chapter-map + carry-reduce must reproduce this exactly — it is the
    /// reference the retired rule was built on.
    fn batch_corpus_cells(corpus: &Corpus) -> BTreeMap<char, [u64; SIDE_CELLS]> {
        let mut totals: BTreeMap<char, [u64; SIDE_CELLS]> = BTreeMap::new();
        for group in by_book(corpus) {
            for_each_spacing_opportunity(&group, |_local, opp| {
                let cell = totals.entry(opp.mark).or_insert([0u64; SIDE_CELLS]);
                if let Some(r) = opp.left {
                    cell[cell_index(Side::Left, r.class, r.form)] += 1;
                }
                if let Some(r) = opp.right {
                    cell[cell_index(Side::Right, r.class, r.form)] += 1;
                }
            });
        }
        totals
    }

    /// Resident-substrate findings for a corpus on a persisted cache — the
    /// incremental path the transition drives (map only changed chapters,
    /// whole-book carry-reduce only a changed book), in final stable order.
    fn resident_findings(
        cache: &mut crate::substrate::SubstrateCache<SpacingSubstrate>,
        corpus: &Corpus,
        cfg: &PunctuationSpacingConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_spacing(true, cache, corpus, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// The spacing substrate is byte-identical cold vs. incremental: a resident
    /// cache carried across a sequence of real mutations (chapter replacement, a
    /// new book, whole-book replacement, book removal) reproduces a cold
    /// full-corpus analysis at every step. This is the Phase C gate (plan §8
    /// step 2 / step 3) at the unit level; the fleet oracle enforces it at scale.
    #[test]
    fn spacing_substrate_incremental_equals_cold_under_edits() {
        use crate::corpus::{BookBlock, ChapterBlock};
        use crate::substrate::SubstrateCache;
        let cfg = sp_no_floor(); // widest finding set surfaces every holding pool
        let mut corpus = multi(&[
            ("GEN 1:1", "In the beginning, God created the heavens."),
            ("GEN 1:2", "The earth was formless, and void, and dark."),
            ("GEN 1:3", "God said, Let there be light, and light"),
            ("GEN 2:1", ", thus the heavens and the earth were finished."),
            ("GEN 2:2", "On the seventh day God ended,his work."),
            ("EXO 1:1", "Now these are the names, of the sons,"),
            ("EXO 1:2", "who came into Egypt,every man and his household."),
        ]);
        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();
        assert_eq!(
            resident_findings(&mut cache, &corpus, &cfg),
            spacing_findings(&corpus, &cfg),
            "cold seed"
        );

        // Replace GEN chapter 2 (changes its hash; GEN ch1 + EXO reuse).
        corpus
            .replace_chapter(ChapterBlock {
                slug: "GEN".into(),
                chapter: "2".into(),
                keys: vec!["GEN 2:1".into(), "GEN 2:2".into(), "GEN 2:3".into()],
                texts: vec![
                    "thus the heavens,and the earth were finished.".into(),
                    "and God rested, and blessed the seventh day,".into(),
                    ",a new leading comma reading across the chapter seam.".into(),
                ],
            })
            .unwrap();
        assert_eq!(
            resident_findings(&mut cache, &corpus, &cfg),
            spacing_findings(&corpus, &cfg),
            "after chapter replacement"
        );

        // Append a new book.
        corpus
            .replace_books(vec![BookBlock {
                slug: "LEV".into(),
                keys: vec!["LEV 1:1".into(), "LEV 1:2".into()],
                texts: vec![
                    "And the Lord called, unto Moses,and spake:".into(),
                    "Speak unto the children, of Israel,".into(),
                ],
            }])
            .unwrap();
        assert_eq!(
            resident_findings(&mut cache, &corpus, &cfg),
            spacing_findings(&corpus, &cfg),
            "after appending a new book"
        );

        // Replace an existing whole book in place.
        corpus
            .replace_books(vec![BookBlock {
                slug: "EXO".into(),
                keys: vec!["EXO 1:1".into(), "EXO 2:1".into()],
                texts: vec![
                    "Now these,are the names of the sons of Israel,".into(),
                    "who came,into Egypt.".into(),
                ],
            }])
            .unwrap();
        assert_eq!(
            resident_findings(&mut cache, &corpus, &cfg),
            spacing_findings(&corpus, &cfg),
            "after whole-book replacement"
        );

        // Remove a book — the cache drops its contribution.
        corpus.remove_book("GEN");
        cache.remove_book("GEN");
        assert_eq!(
            resident_findings(&mut cache, &corpus, &cfg),
            spacing_findings(&corpus, &cfg),
            "after book removal"
        );
    }

    /// The substrate work probes show exactly the intended map/reduce/judge work
    /// (plan §8 Phase C gate): cold maps+reduces every chapter; a judging-knob
    /// change maps/reduces ZERO chapters and only re-judges; a content edit maps
    /// only the changed chapter and re-reduces only its owning book; an unchanged
    /// re-analyze (edit-then-undo) does no map/reduce.
    #[test]
    fn spacing_substrate_work_probes_show_exact_work() {
        use crate::corpus::ChapterBlock;
        use crate::substrate::SubstrateCache;
        let mut corpus = multi(&[
            ("GEN 1:1", "In the beginning, God created,the heavens."),
            ("GEN 1:2", "The earth was formless, and void, and dark,"),
            ("GEN 2:1", ", and God said, Let there be light,"),
            ("EXO 1:1", "Now these are the names, of the sons,"),
        ]); // GEN: chapters 1 (2 verses) + 2 (1 verse); EXO: chapter 1 — 3 chapters
        let cfg = sp_no_floor();
        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();

        // Cold: every chapter mapped and reduced.
        let mut out = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &cfg, &mut out);
        assert_eq!(cache.mapped, 3, "cold maps every chapter");
        assert_eq!(cache.reduced, 3, "cold reduces every chapter");
        assert!(cache.judged >= 1, "cold judges the marks present");

        // Judging-knob change (same corpus, different floor): zero map/reduce.
        let mut out2 = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &sp_default(), &mut out2);
        assert_eq!(cache.mapped, 0, "a knob change maps zero chapters");
        assert_eq!(cache.reduced, 0, "a knob change reduces zero chapters");
        assert!(cache.judged >= 1, "a knob change still re-judges spacing");

        // Edit-then-undo before analyze: the final corpus equals the cached one,
        // so re-analyze maps/reduces nothing.
        let mut out3 = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &cfg, &mut out3);
        assert_eq!(cache.mapped, 0, "an unchanged re-analyze maps zero chapters");
        assert_eq!(cache.reduced, 0, "an unchanged re-analyze reduces zero chapters");

        // Content edit to GEN chapter 2: maps ONLY that chapter, re-reduces ONLY
        // GEN's chapters (its owning book); EXO is untouched.
        corpus
            .replace_chapter(ChapterBlock {
                slug: "GEN".into(),
                chapter: "2".into(),
                keys: vec!["GEN 2:1".into()],
                texts: vec![", and God saw the light,that it was good,".into()],
            })
            .unwrap();
        let mut out4 = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &cfg, &mut out4);
        assert_eq!(cache.mapped, 1, "a one-chapter edit maps exactly that chapter");
        assert_eq!(
            cache.reduced, 2,
            "it re-reduces only the owning book's chapters (GEN has 2), not EXO"
        );
    }

    /// Toggle isolation: disabling spacing drops the substrate's products, and an
    /// edit while disabled does no spacing work; re-enabling cold-builds only the
    /// spacing substrate (plan §7.2 rule toggles / §12.4).
    #[test]
    fn spacing_substrate_toggle_drops_and_rebuilds() {
        use crate::substrate::SubstrateCache;
        let corpus = multi(&[
            ("GEN 1:1", "In the beginning, God created,the heavens."),
            ("GEN 2:1", ", and God said, Let there be light,"),
        ]);
        let cfg = sp_no_floor();
        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();
        let mut out = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &cfg, &mut out);
        assert_eq!(cache.mapped, 2, "cold builds every chapter");
        assert!(cache.book_contribution("GEN").is_some());

        // Disable: the substrate drops its products.
        let mut off = Vec::new();
        drive_spacing(false, &mut cache, &corpus, &cfg, &mut off);
        assert!(off.is_empty(), "disabled substrate emits nothing");
        assert!(
            cache.book_contribution("GEN").is_none(),
            "disabling drops the substrate's cached products"
        );
        // Edit while disabled: still no spacing work.
        let mut off2 = Vec::new();
        drive_spacing(false, &mut cache, &corpus, &cfg, &mut off2);
        assert_eq!(
            (cache.mapped, cache.reduced, cache.judged),
            (0, 0, 0),
            "an edit while spacing is disabled does no spacing work"
        );

        // Re-enable: a cold rebuild of the substrate.
        let mut on = Vec::new();
        drive_spacing(true, &mut cache, &corpus, &cfg, &mut on);
        assert_eq!(cache.mapped, 2, "re-enabling rebuilds the substrate");
        assert_eq!(on, spacing_findings(&corpus, &cfg), "rebuild equals cold");
    }

    /// `replace_book_in_corpus_stats` returns EXACT stats-delta keys: replacing a
    /// book whose sites MOVED but whose per-mark cells are unchanged yields an
    /// EMPTY stats delta, while the site delta is dirty (the sites really did
    /// move). Phase D consumes this as the exact set, so an over-broad delta
    /// would silently re-judge everything; the site half is why equal counts may
    /// never be read as equal sites (plan §6.2, from the other side).
    #[test]
    fn spacing_stats_delta_is_exact_when_sites_move_but_counts_do_not() {
        use crate::substrate::ObservationSubstrate;
        let mut stats = SpacingCorpusStats::default();

        // Two books. GEN's text is rearranged so the same marks occur with the
        // same per-side/per-class reads — identical cells, different sites.
        let before = contribution_of("GEN", &[("1", &["alpha, beta,", "gamma, delta,"])]);
        let after = contribution_of("GEN", &[("1", &["gamma, delta,", "alpha, beta,"])]);
        assert_eq!(
            before.cells, after.cells,
            "fixture precondition: the rearrangement leaves per-mark cells equal"
        );
        assert!(
            before.chapters != after.chapters,
            "fixture precondition: the SITES really did move"
        );

        // Seed, then replace with the moved-site version.
        let seeded =
            SpacingSubstrate::replace_book_in_corpus_stats(&mut stats, "GEN", None, Some(&before));
        assert!(
            !seeded.is_empty(),
            "the initial contribution changes the aggregate"
        );
        let moved = SpacingSubstrate::replace_book_in_corpus_stats(
            &mut stats,
            "GEN",
            Some(&before),
            Some(&after),
        );
        assert!(
            moved.is_empty(),
            "moved sites with equal per-mark counts produce an EMPTY stats delta, got {moved:?}"
        );

        // A genuine count change DOES report — and reports only the marks that moved.
        let extra = contribution_of("GEN", &[("1", &["gamma, delta,", "alpha, beta, more,"])]);
        let grew = SpacingSubstrate::replace_book_in_corpus_stats(
            &mut stats,
            "GEN",
            Some(&after),
            Some(&extra),
        );
        assert_eq!(grew, vec![','], "only the comma's aggregate moved");

        // Full removal returns to empty and reports the marks that vanished.
        let removed =
            SpacingSubstrate::replace_book_in_corpus_stats(&mut stats, "GEN", Some(&extra), None);
        assert_eq!(removed, vec![',']);
        assert!(
            stats.totals.is_empty(),
            "a fully removed book leaves no zeroed residue"
        );
    }

    /// A pending seam mark that crosses an ALL-EMPTY chapter keeps its original
    /// owner: it resolves against the next chapter that has content, but its
    /// site materializes into the chapter it actually lives in and rebases
    /// against THAT chapter's range. Regression for the owner-retag bug, where
    /// the carried seam was re-owned by whatever chapter it passed through — the
    /// site then rebased against the wrong chapter (silently wrong verse when
    /// in range, containment panic when not). The fleet has no all-empty
    /// intervening chapters, so only a synthetic case can catch it.
    #[test]
    fn spacing_pending_seam_keeps_its_owner_across_an_empty_chapter() {
        // GEN 1:2 ends with a trailing period whose right neighbour is the next
        // non-empty verse — GEN 3:1, two chapters later (GEN 2 is all-empty).
        // The finding must belong to GEN 1 verse 2 (`GEN 1:2`).
        let corpus = multi(&[
            ("GEN 1:1", "many words, and more words, and yet more,"),
            ("GEN 1:2", "the sentence ends here ."),
            ("GEN 2:1", ""),
            ("GEN 2:2", "   "),
            ("GEN 3:1", "Then the next chapter opens, with words,"),
        ]);
        // Ownership asserted on the REDUCED contribution, before any judging —
        // deliberately independent of scoring. The judged-finding checks below
        // are conditional (a mark the judge does not emit for has no finding to
        // inspect), so a future scoring change could quietly make them vacuous;
        // this one cannot. The period's site must sit in chapter `1` at
        // chapter-local verse index 1, with a resolved right side: the resolution
        // travelled across the all-empty GEN 2 and folded into GEN 1's reduced
        // result, its owner.
        let contribution = contribution_of(
            "GEN",
            &[
                (
                    "1",
                    &[
                        "many words, and more words, and yet more,",
                        "the sentence ends here .",
                    ],
                ),
                ("2", &["", "   "]),
                ("3", &["Then the next chapter opens, with words,"]),
            ],
        );
        let period_sites: Vec<(&str, u16, bool)> = contribution
            .chapters
            .iter()
            .flat_map(|(token, sites)| {
                sites
                    .iter()
                    .filter(|s| s.mark == '.')
                    .map(move |s| (&**token, s.local_idx.get(), s.right.is_some()))
            })
            .collect();
        assert_eq!(
            period_sites,
            vec![("1", 1u16, true)],
            "the period's site must be owned by GEN 1 at chapter-local verse 1, right side resolved"
        );
        let cfg = sp_no_floor();
        let got = spacing_findings(&corpus, &cfg);
        // Whatever the verdicts, every emitted finding must address a verse whose
        // text actually contains its mark — the direct witness that no site was
        // rebased into the wrong chapter.
        for f in &got {
            let text = corpus.text(f.key_idx);
            let FindingArgs::SpacingConvention { mark, .. } =
                f.args.as_ref().expect("spacing carries args")
            else {
                panic!("spacing emits SpacingConvention args");
            };
            assert!(
                text.contains(*mark),
                "finding at {} claims mark {mark:?} but that verse reads {text:?} \
                 — the site rebased against the wrong chapter",
                corpus.key(f.key_idx)
            );
            assert!(
                f.range.end as usize <= text.len(),
                "span {:?} out of bounds for {} ({:?})",
                f.range,
                corpus.key(f.key_idx),
                text
            );
        }
        // The period's own occurrence is owned by GEN 1:2 (not GEN 2:x / GEN 3:1).
        if let Some(f) = got.iter().find(|f| {
            matches!(
                f.args.as_ref(),
                Some(FindingArgs::SpacingConvention { mark: '.', .. })
            )
        }) {
            assert_eq!(
                corpus.key(f.key_idx),
                "GEN 1:2",
                "the trailing period belongs to the chapter it lives in"
            );
        }
        // And the cells match the whole-book batch walk: the seam-crossing
        // period's right side reads GEN 3:1's leading letter, exactly as the
        // batch walk's `find_map` skips the empty verses.
        assert_eq!(
            spacing_corpus_cells(&corpus),
            batch_corpus_cells(&corpus),
            "substrate cells diverge from the batch walk across an empty chapter"
        );
    }

    /// A pending seam at the book's LAST chapter never resolves — the book edge
    /// has no neighbour across the seam, so that side abstains — matching the
    /// batch walk's book-end semantics exactly, with the site owned by its own
    /// chapter.
    #[test]
    fn spacing_pending_seam_at_book_edge_abstains_like_the_batch_walk() {
        let corpus = multi(&[
            ("GEN 1:1", "many words, and more words, and yet more,"),
            ("GEN 2:1", "a trailing mark closes the book ."),
            ("GEN 2:2", ""),
        ]);
        assert_eq!(
            spacing_corpus_cells(&corpus),
            batch_corpus_cells(&corpus),
            "book-edge abstain must match the batch walk"
        );
        for f in &spacing_findings(&corpus, &sp_no_floor()) {
            let text = corpus.text(f.key_idx);
            let FindingArgs::SpacingConvention { mark, .. } =
                f.args.as_ref().expect("spacing carries args")
            else {
                panic!("spacing emits SpacingConvention args")
            };
            assert!(text.contains(*mark), "site rebased into the wrong chapter");
        }
    }

    /// The carry is load-bearing: a verse-leading mark at a chapter's start reads
    /// its left neighbour ACROSS the chapter seam (the previous chapter's last
    /// verse), so its Left cell is populated — a `()`-boundary migration that
    /// dropped the carry would leave it empty and diff the fleet. Witnessed
    /// directly on the substrate's corpus cells, independent of any threshold.
    #[test]
    fn spacing_substrate_carry_populates_the_cross_chapter_left_cell() {
        // GEN 1:2 ends with a letter ("light"); GEN 2:1 begins with a comma whose
        // left neighbour is that letter, read across the chapter seam as spaced.
        let corpus = multi(&[
            ("GEN 1:1", "the beginning"),
            ("GEN 1:2", "and there was light"),
            ("GEN 2:1", ", thus it was"),
        ]);
        let cells = spacing_corpus_cells(&corpus);
        let comma = cells.get(&',').expect("the comma has cells");
        let left_letter_spaced = comma[cell_index(Side::Left, SpacingClass::Letter, SpacingForm::Spaced)];
        assert_eq!(
            left_letter_spaced, 1,
            "the chapter-leading comma's left must read the previous chapter's \
             trailing letter across the seam (the code-proven carry)"
        );
    }

    /// Editing the chapter that RESOLVES an earlier chapter's pending seam mark
    /// must not lose (or double) that resolution. The replay window has to reach
    /// back to the owning chapter, because the owner's reduced result is rebuilt
    /// from nothing and the resolution folds into it again. Compared against the
    /// independent whole-book batch walk, which shares no code with the substrate.
    #[test]
    fn editing_the_resolving_chapter_keeps_the_owners_cross_seam_cell() {
        use crate::corpus::ChapterBlock;
        use crate::substrate::SubstrateCache;
        // GEN 1:2 ends with a trailing comma whose right neighbour lives in GEN 2
        // — a pending seam owned by chapter 1 and resolved by chapter 2.
        let mut corpus = multi(&[
            ("GEN 1:1", "In the beginning"),
            ("GEN 1:2", "and there was light,"),
            ("GEN 2:1", "thus it was so"),
            ("GEN 3:1", "and the day ended"),
        ]);
        let cfg = sp_no_floor();
        let mut cache: SubstrateCache<SpacingSubstrate> = SubstrateCache::new();
        let cold = resident_findings(&mut cache, &corpus, &cfg);
        assert_eq!(
            cache.corpus_stats().totals,
            batch_corpus_cells(&corpus),
            "cold cells equal the independent batch walk"
        );
        let _ = cold;

        // Edit the RESOLVING chapter only.
        corpus
            .replace_chapter(ChapterBlock {
                slug: "GEN".into(),
                chapter: "2".into(),
                keys: vec!["GEN 2:1".into()],
                // The first character's class CHANGES (letter → digit), so the
                // owning chapter's cross-seam cell must actually move: a replay
                // that started after the owner would keep the stale letter cell.
                texts: vec!["40 days it came to be".into()],
            })
            .unwrap();
        cache.reset_probes();
        let warm = resident_findings(&mut cache, &corpus, &cfg);
        assert_eq!(cache.mapped, 1, "only the edited chapter is re-mapped");
        assert_eq!(
            cache.corpus_stats().totals,
            batch_corpus_cells(&corpus),
            "the owning chapter keeps exactly one cross-seam resolution"
        );
        assert_eq!(warm, spacing_findings(&corpus, &cfg), "resident equals cold");
    }
}
