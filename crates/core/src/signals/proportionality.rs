//! Proportionality — the first cross-map rule, migrated to the stateful
//! (observe-then-judge) shape (ADR 0017).
//!
//! For each verse present in **both** the target and the reference
//! (`source`), the target/reference grapheme-length ratio is informative:
//! a verse 3× or ⅓ the reference length is often a misplaced verse
//! number, an omission, or gross over/under-translation. We flag verses
//! whose ratio is a robust outlier **within its book**: per book, take
//! the median of the ratios and flag `|z| > z_long` (above the median) or
//! `|z| > z_short` (below it), where `z = 0.6745 · (ratio − median) / MAD`
//! (median+MAD, not mean+stddev, so one bad verse can't poison the
//! threshold — methods §3.4).
//!
//! **Asymmetric spread (ADR 0069).** The ratio distribution is squeezed
//! against zero on the short side (a verse can be at most 100% shorter —
//! empty) and open-ended on the long side (a verse can be arbitrarily
//! longer), so one symmetric MAD mis-sizes one tail. `judge` therefore
//! measures two ONE-SIDED MADs per unit — `MAD_above` from deviations of
//! ratios greater than the median, `MAD_below` from deviations of ratios
//! less than the median — and scores a verse's signed deviation against
//! whichever side it fell on, with its own threshold (`z_long`/`z_short`,
//! both default 3.5 — Phase B confirmed the symmetric value; see
//! `documentation/calibration/2026-07-30-length-ratio-paired-survey.md`
//! and `documentation/adrs/0069-length-ratio-asymmetric-spread.md`).
//!
//! `reduce` records the raw per-book ratios (the sufficient statistic for an
//! order rule — Phase 1 §7); `judge` derives the median/MAD late and flags
//! outliers. `merge` is book-level supersede, so an edit re-reduces only its
//! book.
//!
//! **Surface both** (ADR 0017 §8): `judge` measures each verse against two
//! distributions — its own book and the whole project (all books pooled) —
//! and flags it once if it is an outlier in *either*, tagging the finding's
//! `scope` (`Book` / `Project` / `Both`) with the z-score(s) that fired. The
//! book-scope output matches the prior Mode-A implementation; project-scope is
//! additive (e.g. a verse a short book can't judge alone but the project can).

use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::config::ProportionalityConfig;
use crate::corpus::{Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, LengthRatioScope, RuleId, Severity};
use crate::span::Span;

pub const PROJECT_LENGTH_RATIO: RuleId = RuleId::ProjectLengthRatio;

/// Scale factor making MAD a stddev-equivalent under normality, so
/// `z_long`/`z_short` read in familiar z-score units.
const MAD_TO_SIGMA: f64 = 0.6745;

/// One verse's target/reference ratio, retained so materialization can emit
/// without the text. `local_idx` is **chapter**-local (its chapter's layout block
/// carries the rebase base), and `len` is the verse's byte length — the whole-verse
/// anchor span. 12 bytes.
///
/// This is the retain-vs-rederive choice, and it is the one row where retention is
/// not a judgement call: the ratio is a function of BOTH corpora, so re-deriving it
/// at materialization would mean re-pairing and re-counting graphemes on both
/// sides. `len` is retained with it because it is free (the same walk already has
/// the text) and because materialization must not touch the target text at all —
/// this rule has never scanned it at judge time.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RatioObs {
    local_idx: LocalKeyIdx,
    ratio: f32,
    len: u32,
}

/// Bitwise equality on the ratio, which is what makes `RatioObs` (and so the
/// cached observation) `Eq`.
///
/// `f32` is not `Eq` because of NaN, and a cache-validity comparison is exactly
/// the case where bitwise is the RIGHT relation: the question is "would a
/// recomputation produce the identical bits", not "are these numerically close".
/// Bitwise equality is also reflexive whatever the payload, so `Eq`'s law holds
/// even for a value the map cannot actually produce (a ratio is a quotient of two
/// non-zero grapheme counts, hence always finite).
impl PartialEq for RatioObs {
    fn eq(&self, other: &Self) -> bool {
        self.local_idx == other.local_idx
            && self.len == other.len
            && self.ratio.to_bits() == other.ratio.to_bits()
    }
}

impl Eq for RatioObs {}

/// Index the reference corpus by key string, in presented order — pairing
/// is by (exact key string, occurrence ordinal), never by array position,
/// since `source`/`target` are independent corpora with possibly different
/// lengths and orderings.
pub(crate) type SourceIndex<'a> = FxHashMap<&'a str, Vec<&'a str>>;

/// One chapter's proportionality observation: its paired verses' ratios in verse
/// order.
///
/// **Boundary state is `()`, and the proof is a `Corpus` invariant, not just a
/// listener reading.** The retired `ProportionalityAcc` carried one piece of state
/// across verses — `seen`, the per-key occurrence ordinal that disambiguates
/// duplicate keys. That ordinal is CHAPTER-local: a key's chapter token is parsed
/// from the key itself and a chapter run may not reopen, so every occurrence of a
/// given key string lies inside a single chapter run. Counting occurrences from
/// the book's start therefore gives the same ordinal as counting from the
/// chapter's. Nothing else crossed a verse.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RatioChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book contribution rather than
    /// deep-copied per reduce.
    obs: Arc<Vec<RatioObs>>,
}

/// One chapter's reduced proportionality result — identical to its observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RatioReduced {
    token: Box<str>,
    obs: Arc<Vec<RatioObs>>,
}

/// A book's folded proportionality contribution: its ratios pooled in verse order
/// (the corpus aggregate's addend) plus its chapters' reduced results, which own
/// the addresses materialization rebases.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RatioBookContribution {
    /// Bit-comparable for the same reason `RatioObs` is: a `Vec<f32>` of ratios
    /// compares by `PartialEq`, which is fine here because a NaN can never enter
    /// it, and the wrapper only ever answers "is the cached contribution the one a
    /// recomputation would produce".
    ratios: Arc<Vec<RatioBits>>,
    chapters: Vec<RatioReduced>,
}

/// A ratio in the pooled sample, wrapped so the pool is `Eq`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RatioBits(f32);

impl PartialEq for RatioBits {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for RatioBits {}

/// A median with its two one-sided MADs, their sample sizes, and the
/// pooled symmetric MAD (ADR 0069) fallback — the sample size the whole
/// unit came from is `count`. Knob-FREE: `min_verses` and the per-side
/// data-floor/zero-MAD rules are judging gates, applied in
/// [`judge`](ProportionalitySubstrate::judge), never here — which is what
/// lets the aggregate be maintained without knowing any config.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Spread {
    count: usize,
    med: f64,
    /// Median of `(x - med)` over `x > med` — the long-side MAD.
    mad_above: f64,
    /// Median of `(med - x)` over `x < med` — the short-side MAD.
    mad_below: f64,
    /// How many points landed strictly above/below the median — the
    /// per-side data-floor gate reads these, not just the MAD values.
    n_above: usize,
    n_below: usize,
    /// Median of `|x - med|` over ALL points (both sides pooled) — the
    /// pre-ADR-0069 symmetric MAD, retained as the fallback for whichever
    /// side doesn't clear the data floor (`Spread::gated`).
    mad_symmetric: f64,
}

/// The proportionality corpus aggregate: every book's ratios, that book's spread,
/// and the whole project's pooled spread (ADR 0017 §8's second scope).
#[derive(Default)]
pub(crate) struct RatioCorpusStats {
    per_book: BTreeMap<Box<str>, Arc<Vec<RatioBits>>>,
    book: BTreeMap<Box<str>, Spread>,
    project: Spread,
}

/// The judge key: a book slug. Both distributions a verse is measured against are
/// functions of the book it is in plus the corpus, so one outcome serves every
/// verse of that book — and materialization turns it into each verse's own z.
pub(crate) type RatioKey = Box<str>;

/// One judgeable unit's gated spread: the median, plus each side's MAD when
/// that side has signal. `Spread::gated` is the sole constructor — reached
/// only once the whole unit clears `min_verses`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SideSpreads {
    med: f64,
    mad_above: Option<f64>,
    mad_below: Option<f64>,
}

/// One book's verdict: the two units (book, project) a verse is measured
/// against, each already gated by `min_verses` and, per side, the
/// zero-MAD rule (ADR 0069).
#[derive(Clone, Copy, Default)]
pub(crate) struct RatioOutcome {
    book: Option<SideSpreads>,
    project: Option<SideSpreads>,
}

/// The `proj.length-ratio` observation substrate. Sole consumer: the rule of the
/// same name. **The only substrate that declares a reference input** (plan §5.2);
/// its consumer is also the only `InputDependency::TargetAndReferenceSilentWhenAbsent`
/// rule, and the two facts are the same fact seen from the two ends.
pub(crate) struct ProportionalitySubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <ProportionalitySubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's proportionality map: the ratio of every target verse that pairs
/// with a reference verse, by (exact key string, occurrence ordinal).
fn map_ratio_chapter(chapter: &crate::substrate::ChapterView<'_>) -> RatioChapterObs {
    let mut obs: Vec<RatioObs> = Vec::new();
    // No declared reference chapter -> no ratios at all. The rule is
    // `TargetAndReferenceSilentWhenAbsent`, so an empty observation is the correct
    // answer, not a missing one: the chapter is still cached, still stamped
    // `ReferenceStamp::Absent`, and re-maps the moment a reference appears.
    if let Some(paired) = chapter.paired_view() {
        let mut index: SourceIndex<'_> = FxHashMap::default();
        for (key, text) in paired
            .reference_keys
            .iter()
            .zip(paired.reference_texts.iter())
        {
            index.entry(key.as_str()).or_default().push(text.as_str());
        }
        let mut seen: FxHashMap<&str, usize> = FxHashMap::default();
        for (vi, (key, text)) in paired.keys.iter().zip(chapter.texts.iter()).enumerate() {
            let ordinal = seen.entry(key.as_str()).or_insert(0);
            let src_text = index
                .get(key.as_str())
                .and_then(|texts| texts.get(*ordinal))
                .copied();
            *ordinal += 1;
            let Some(src_text) = src_text else {
                continue;
            };
            let t = crate::grapheme::count(text);
            let s = crate::grapheme::count(src_text);
            // Empty sides carry no signal and would divide by zero.
            if t == 0 || s == 0 {
                continue;
            }
            obs.push(RatioObs {
                local_idx: LocalKeyIdx::from_usize(vi),
                ratio: (t as f64 / s as f64) as f32,
                len: text.len() as u32,
            });
        }
    }
    RatioChapterObs {
        token: Box::from(chapter.chapter),
        obs: Arc::new(obs),
    }
}

/// The median of `v`, destructively — `select_nth_unstable_by` rather than a full
/// sort, so maintaining the pooled project spread on every book replacement is
/// linear rather than `n log n`. Bit-identical to the sorting median it replaces:
/// odd counts take the middle element, even counts average the two middles, and
/// selection puts the `n/2`-th smallest at `n/2` with everything no greater than
/// it before, so the prefix maximum IS the sorted `n/2 - 1`.
fn median_in_place(v: &mut [f64]) -> f64 {
    let cmp = |a: &f64, b: &f64| a.partial_cmp(b).expect("ratios are finite");
    let n = v.len();
    if n % 2 == 1 {
        let (_, mid, _) = v.select_nth_unstable_by(n / 2, cmp);
        *mid
    } else {
        let (lo, mid, _) = v.select_nth_unstable_by(n / 2, cmp);
        let hi = *mid;
        let lo_max = lo
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        (lo_max + hi) / 2.0
    }
}

/// The knob-free median plus the two one-sided MADs, their sample counts,
/// and the pooled symmetric MAD (ADR 0069) of a ratio sample. The median
/// itself is computed from the WHOLE sample (unchanged from the symmetric
/// design) — only the spread around it splits by side, and the symmetric
/// MAD is retained alongside as the per-side fallback's data source.
fn spread_of<'a>(ratios: impl Iterator<Item = &'a RatioBits>) -> Spread {
    let mut v: Vec<f64> = ratios.map(|r| f64::from(r.0)).collect();
    let count = v.len();
    if count == 0 {
        return Spread::default();
    }
    let med = median_in_place(&mut v);
    // The median splits `count` close to 50/50 by construction (ties at
    // `med` land on neither side), so each side's sample is normally
    // large whenever the whole unit clears `min_verses` — a side collapsing
    // to a handful of deviations signals a genuine tie pileup or a skewed
    // distribution, not starvation.
    let mut above: Vec<f64> = v.iter().copied().filter(|&x| x > med).map(|x| x - med).collect();
    let mut below: Vec<f64> = v.iter().copied().filter(|&x| x < med).map(|x| med - x).collect();
    let mut symmetric: Vec<f64> = v.iter().map(|&x| (x - med).abs()).collect();
    let n_above = above.len();
    let n_below = below.len();
    let mad_above = if above.is_empty() {
        0.0
    } else {
        median_in_place(&mut above)
    };
    let mad_below = if below.is_empty() {
        0.0
    } else {
        median_in_place(&mut below)
    };
    // `symmetric` always has `count` (> 0, checked above) entries, so this
    // never hits the empty-median trap `median_in_place` doesn't guard.
    let mad_symmetric = median_in_place(&mut symmetric);
    Spread {
        count,
        med,
        mad_above,
        mad_below,
        n_above,
        n_below,
        mad_symmetric,
    }
}

/// Minimum strict deviations a side needs before its OWN one-sided MAD is
/// trusted (ADR 0069's per-side data floor). Below this, a side falls back
/// to the pooled symmetric MAD instead. 3 is the smallest sample where a
/// one-sided median-of-deviations is not trivially pinned to a single
/// member's own value: at 1 point the "median" IS that point's deviation
/// (self-referential — see the collapse property documented on
/// `Spread::gated`); at 2 points the median is their average, still
/// dominated by whichever of the two is the actual candidate under test;
/// at 3, the median deviation is a real THIRD point's value, independent of
/// whichever single point is being scored against it.
const SIDE_DATA_FLOOR: usize = 3;

impl Spread {
    /// Apply the judging gates. Two layers:
    ///
    /// 1. **Whole-unit gate** (unchanged from the symmetric design): too few
    ///    paired verses and the unit cannot judge at all — `min_verses = 0`
    ///    must still not let an empty sample through, so the count==0 check
    ///    is independent of the caller-supplied floor.
    /// 2. **Per-side gate with pooled fallback** (ADR 0069): each side is
    ///    judged independently once the whole unit clears (1). A side uses
    ///    its OWN one-sided MAD only when it has `>= SIDE_DATA_FLOOR` strict
    ///    deviations AND that MAD is nonzero; otherwise it falls back to the
    ///    pooled symmetric MAD (today's — pre-ADR-0069 — single-MAD design).
    ///    **Collapse property, and why the floor exists**: with too few
    ///    points on a side, that side's own MAD is measured FROM the very
    ///    points it would judge — at n=1 a lone deviation's "median" is
    ///    itself, pinning its z at exactly `MAD_TO_SIGMA` (0.6745) no matter
    ///    how extreme the underlying ratio is (proven in
    ///    `a_side_with_a_single_deviation_can_never_fire_on_that_side`).
    ///    The pooled fallback is never `None` unless the WHOLE unit is
    ///    degenerate (every ratio identical), in which case neither side
    ///    should fire anyway. Net effect: the finer per-side instrument is
    ///    used only where the data can support it; below the floor the rule
    ///    behaves exactly as the symmetric model did — the same
    ///    self-gating shape as every other corpus-relative rule in this
    ///    engine (a convention needs enough recurrence before it is trusted
    ///    to judge at all).
    fn gated(self, min_verses: usize) -> Option<SideSpreads> {
        if self.count == 0 || self.count < min_verses {
            return None;
        }
        let side_or_fallback = |n: usize, mad_side: f64| -> Option<f64> {
            if n >= SIDE_DATA_FLOOR && mad_side > 0.0 {
                Some(mad_side)
            } else {
                (self.mad_symmetric > 0.0).then_some(self.mad_symmetric)
            }
        };
        Some(SideSpreads {
            med: self.med,
            mad_above: side_or_fallback(self.n_above, self.mad_above),
            mad_below: side_or_fallback(self.n_below, self.mad_below),
        })
    }
}

/// The signed z of ratio `r` against one gated unit's spreads: `MAD_above`
/// when `r` is longer than the median, `MAD_below` when shorter, `None`
/// when the unit didn't judge, `r` sits exactly at the median (no
/// deviation), or the relevant side abstained. The sign is `(r - med)`'s,
/// preserved through whichever side's MAD divides it — negative means
/// shorter than typical, matching `LengthRatioScope`'s documented
/// convention.
fn side_z(r: f64, outcome: Option<SideSpreads>) -> Option<f64> {
    let s = outcome?;
    if r > s.med {
        Some(MAD_TO_SIGMA * (r - s.med) / s.mad_above?)
    } else if r < s.med {
        Some(MAD_TO_SIGMA * (r - s.med) / s.mad_below?)
    } else {
        None
    }
}

/// Which threshold governs a signed z: `z_long` above the median (`z >=
/// 0`), `z_short` below it. Mirrors `side_z`'s sign convention exactly, so
/// a long-side z is never compared against the short-side knob or vice
/// versa.
fn threshold_for(z: f64, cfg: &ProportionalityConfig) -> f32 {
    if z >= 0.0 { cfg.z_long } else { cfg.z_short }
}

impl crate::substrate::ObservationSubstrate for ProportionalitySubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Proportionality;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;
    // The ONE reference-declaring substrate in the engine, and the declaration —
    // not this driver's code — is what makes its reference stamps and paired views
    // constructible at all.
    type Pairing = crate::substrate::SameSlugSameChapter;
    // Verse lengths and a paired reference lookup: no mechanical view at all.
    const NEEDS: crate::prep::PrepNeeds = crate::prep::PrepNeeds::NONE;

    type Key = RatioKey;
    // Proven from the `Corpus` chapter-token invariant — see `RatioChapterObs`.
    type BoundaryState = ();
    type ChapterObservation = RatioChapterObs;
    type ReducedChapter = RatioReduced;
    type BookContribution = RatioBookContribution;
    type CorpusStats = RatioCorpusStats;
    // All three `ProportionalityConfig` fields (`z_long`, `z_short`,
    // `min_verses`) are read at judge/materialize, so a knob change maps and
    // reduces nothing. The REFERENCE is not config and does not appear here:
    // it enters `ObservationInputStamp::reference` as declared evidence.
    type ExtractorConfig = ();
    type Symbols = ();
    type JudgeConfig = ProportionalityConfig;
    type EntryOutcome = RatioOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        _symbols: &(),
    ) -> RatioChapterObs {
        map_ratio_chapter(chapter)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &RatioChapterObs,
        _entering: &(),
        _carry_out: &mut RatioReduced,
    ) -> (RatioReduced, ()) {
        (
            RatioReduced {
                token: observation.token.clone(),
                obs: Arc::clone(&observation.obs),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut RatioReduced) {}

    fn fold_book(reduced: &[RatioReduced], _symbols: &()) -> RatioBookContribution {
        RatioBookContribution {
            ratios: Arc::new(
                reduced
                    .iter()
                    .flat_map(|r| r.obs.iter().map(|o| RatioBits(o.ratio)))
                    .collect(),
            ),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut RatioCorpusStats,
        slug: &str,
        old: Option<&RatioBookContribution>,
        new: Option<&RatioBookContribution>,
    ) -> Vec<RatioKey> {
        let before = stats.project;
        match new {
            Some(c) => {
                stats
                    .per_book
                    .insert(Box::from(slug), Arc::clone(&c.ratios));
                stats
                    .book
                    .insert(Box::from(slug), spread_of(c.ratios.iter()));
            }
            None => {
                stats.per_book.remove(slug);
                stats.book.remove(slug);
            }
        }
        // A median is not a sum: there is no subtract-then-add for it, so the
        // pooled spread is recomputed from the (unchanged) per-book samples. That
        // is linear in the corpus's ratio count thanks to `median_in_place`, where
        // the retired judge paid `n log n` for the same answer on every call.
        let changed = old.map(|c| &c.ratios[..]) != new.map(|c| &c.ratios[..]);
        if changed {
            stats.project = spread_of(stats.per_book.values().flat_map(|v| v.iter()));
        }

        // The delta is EXACT and honours both of a verse's judge inputs. Its own
        // book's spread moved -> that book. The POOLED spread moved -> every book,
        // because the project scope measures every verse against it. Neither moved
        // -> nothing, which is the case a knob-only or unrelated-book edit takes.
        if stats.project != before {
            return stats.book.keys().cloned().collect();
        }
        if changed {
            return vec![Box::from(slug)];
        }
        Vec::new()
    }

    fn judge(
        cfg: &ProportionalityConfig,
        key: &RatioKey,
        stats: &RatioCorpusStats,
    ) -> RatioOutcome {
        RatioOutcome {
            book: stats
                .book
                .get(key)
                .copied()
                .unwrap_or_default()
                .gated(cfg.min_verses),
            project: stats.project.gated(cfg.min_verses),
        }
    }
}

impl RatioBookContribution {
    /// Emit this book's length-ratio findings from the retained ratios — this rule
    /// has never scanned the target text at judge time and still does not.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        cfg: &ProportionalityConfig,
        outcome: RatioOutcome,
        out: &mut Vec<Finding>,
    ) {
        // Positional zip is truncating: a missing or extra trailing chapter would
        // silently DROP findings rather than fail. Chapter cardinality is the
        // alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for (chapter, block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for o in chapter.obs.iter() {
                let r = f64::from(o.ratio);
                let book_z = side_z(r, outcome.book);
                let project_z = side_z(r, outcome.project);
                let book_fires = book_z.is_some_and(|z| z.abs() > f64::from(threshold_for(z, cfg)));
                let project_fires =
                    project_z.is_some_and(|z| z.abs() > f64::from(threshold_for(z, cfg)));
                let scope = match (book_fires, project_fires) {
                    (true, true) => LengthRatioScope::Both {
                        book_z: book_z.unwrap() as f32,
                        project_z: project_z.unwrap() as f32,
                    },
                    (true, false) => LengthRatioScope::Book {
                        z: book_z.unwrap() as f32,
                    },
                    (false, true) => LengthRatioScope::Project {
                        z: project_z.unwrap() as f32,
                    },
                    (false, false) => continue, // outlier in neither scope
                };
                // Confidence: the strongest firing z, scored against ITS OWN
                // side's threshold — a long-side z never borrows z_short's
                // scale or vice versa.
                let mag = book_fires
                    .then(|| book_z.unwrap())
                    .into_iter()
                    .chain(project_fires.then(|| project_z.unwrap()))
                    .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
                    .expect("scope match above already proved at least one side fires");
                out.push(Finding {
                    key_idx: rebase(base, o.local_idx),
                    code: PROJECT_LENGTH_RATIO,
                    severity: Severity::Warning,
                    // The finding anchors the whole verse; `key_idx` carries identity.
                    range: Span {
                        start: 0,
                        end: o.len,
                    },
                    score: Some(score_from_z(mag.abs(), threshold_for(mag, cfg))),
                    args: Some(FindingArgs::LengthRatio {
                        ratio_pct: r as f32 * 100.0,
                        scope,
                    }),
                });
            }
        }
    }
}

/// Plan the `proj.length-ratio` substrate's share of this analysis: enrol it in
/// the chapter-outer schedule for exactly the chapters whose observation input
/// stamp moved. When inactive, drop the cached products so an edit while it is
/// disabled does no work for it, and enrol nothing.
///
/// One of the two source-dependent plans, alongside `lex.untranslated-word`: each
/// chapter's stamp carries the paired reference chapter's hash (or the explicit
/// absent tag), so a reference edit, a reference replacement, and a reference
/// removal each invalidate exactly the chapters whose evidence moved — and a
/// target-only substrate's stamps are untouched by all three.
pub(crate) fn plan_proportionality<'a>(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<ProportionalitySubstrate>,
    schedule: &mut crate::schedule::Schedule<'a>,
    reference: Option<&crate::schedule::ReferencePairingIndex<'a>>,
) -> Option<crate::schedule::SubstratePlan<'a, ProportionalitySubstrate>> {
    use crate::substrate::ObservationInputStamp;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return None;
    }
    Some(
        schedule.enrol::<ProportionalitySubstrate>(cache, |slug, c| {
            // `with_reference` will not compile for a substrate whose registry entry
            // is `TargetOnly`, and `target_only` will not compile here — the
            // declaration, not this planner, decides which stamp shape is legal.
            ObservationInputStamp::with_reference::<ProportionalitySubstrate>(
                c.hash,
                &(),
                reference.and_then(|r| r.hash_of(slug, &c.chapter)),
            )
        }),
    )
}

/// Reduce, judge and materialize `proj.length-ratio` from the observations the
/// chapter-outer scheduler mapped.
pub(crate) fn finish_proportionality(
    cache: &mut crate::substrate::SubstrateCache<ProportionalitySubstrate>,
    corpus: &Corpus,
    cfg: &ProportionalityConfig,
    plan: crate::schedule::SubstratePlan<'_, ProportionalitySubstrate>,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{DrivePhase, DriveProbe, ObservationSubstrate};
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Proportionality);
    let layout = corpus.book_layout();
    let crate::schedule::SubstratePlan { stamped, mut slots } = plan;
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], &(), |i| slots.take(bi, i));
    }
    probe.mark(DrivePhase::Reduce);
    // The judge key set is the aggregate's own book set — one verdict per book,
    // serving every verse in it. No key-discovery phase.
    let stats = cache.corpus_stats();
    let verdicts: BTreeMap<RatioKey, RatioOutcome> = stats
        .book
        .keys()
        .map(|slug| {
            (
                slug.clone(),
                ProportionalitySubstrate::judge(cfg, slug, stats),
            )
        })
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    probe.mark(DrivePhase::Judge);
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            let outcome = verdicts.get(&book.slug).copied().unwrap_or_default();
            contrib.materialize(&book.chapters, cfg, outcome, out);
        }
    }
    probe.mark(DrivePhase::Materialize);
}

/// The whole substrate on its own, over one caller-held cache — the shape the
/// per-rule convenience entry point and its tests use. Same planning pass, same
/// chapter task, same `finish_*`; only the participation mask is narrower.
pub(crate) fn drive_proportionality(
    active: bool,
    cache: &mut crate::substrate::SubstrateCache<ProportionalitySubstrate>,
    corpus: &Corpus,
    source: Option<&Corpus>,
    cfg: &ProportionalityConfig,
    out: &mut Vec<Finding>,
) {
    let keys = corpus.keys();
    let reference = crate::schedule::ReferencePairingIndex::new(source);
    let mut schedule = crate::schedule::Schedule::new(corpus);
    let Some(mut plan) = plan_proportionality(active, cache, &mut schedule, reference.as_ref())
    else {
        return;
    };
    schedule.run_solo::<ProportionalitySubstrate>(&mut plan, &(), &(), |slug, c| {
        reference
            .as_ref()
            .and_then(|r| r.view_of(&keys[c.range.clone()], slug, &c.chapter))
    });
    finish_proportionality(cache, corpus, cfg, plan, out);
}

/// `proj.length-ratio` findings for a whole corpus at a given config, via the
/// observation substrate over a fresh transient cache — the single
/// proportionality implementation, for tests and calibration callers. Findings are
/// in the final stable order.
pub fn length_ratio_findings(
    corpus: &Corpus,
    source: Option<&Corpus>,
    cfg: &ProportionalityConfig,
) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut out = Vec::new();
    drive_proportionality(true, &mut cache, corpus, source, cfg, &mut out);
    out.sort_by_key(|f| (f.key_idx, f.range.start));
    out
}

/// Map `|z|` to a bounded confidence: 0.5 at the firing threshold,
/// saturating to 1.0 at twice the threshold. Linear in between — the
/// score orders findings for the editor's confidence chip; it is not a
/// calibrated probability.
fn score_from_z(abs_z: f64, z_threshold: f32) -> f32 {
    (abs_z / (2.0 * f64::from(z_threshold))).min(1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProportionalityConfig;

    /// A key string for chapter 1, verse `verse` of `book` — the wire format
    /// (`"GEN 1:3"`) both target and source corpora key on. Pairing is by
    /// exact key string (occurrence ordinal for duplicates), so target/source
    /// verses that should pair just need to share this string.
    fn key(book: &str, verse: u16) -> String {
        format!("{book} 1:{verse}")
    }

    /// Four-bucket background jitter around `base` (ADR 0069 test fixture):
    /// two buckets below the eventual median, two above. A single
    /// two-value alternation (as the old symmetric-MAD tests used) puts a
    /// planted outlier ALONE on its side of the median — under double-MAD
    /// that makes the outlier's own deviation equal its side's MAD exactly
    /// (z pins at `MAD_TO_SIGMA` regardless of magnitude, since nothing
    /// else on that side gives the outlier something to be extreme
    /// RELATIVE TO). Real fleet ratios are continuous and don't collapse
    /// this way; this helper gives synthetic corpora the same genuine
    /// same-side company a real distribution has, on both sides.
    fn spread_jitter(base: &str, v: u16) -> String {
        match v % 4 {
            0 => base[..base.len() - 2].to_string(), // shorter: below median
            1 => base.to_string(),
            2 => format!("{base}x"),  // longer: above median
            _ => format!("{base}xx"), // longer still: above median
        }
    }

    /// `n` parallel verses of equal length, with target verse `outlier_at`
    /// (if any) inflated by `factor`. Target and source share key strings
    /// 1:1 (the common, non-duplicate-key pairing case).
    fn corpus(n: u16, outlier_at: Option<u16>, factor: usize) -> (Corpus, Corpus) {
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=n {
            let base = "abcdefghij ".repeat(4); // 44 graphemes
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            let t = if outlier_at == Some(v) {
                base.repeat(factor)
            } else {
                base.clone()
            };
            target_keys.push(k);
            target_texts.push(t);
        }
        (
            Corpus::try_from_parts(target_keys, target_texts).unwrap(),
            Corpus::try_from_parts(source_keys, source_texts).unwrap(),
        )
    }

    /// Rebuild `c` with each text passed through `f(index, text)` —
    /// `Corpus` has no `iter_mut`, so a length jitter goes through the owned
    /// `texts()` vec and a fresh validated `Corpus`, standing in for the old
    /// `VerseMap::iter_mut` mutation-in-place.
    fn jitter(c: &Corpus, f: impl Fn(usize, &mut String)) -> Corpus {
        let mut texts = c.texts().to_vec();
        for (i, t) in texts.iter_mut().enumerate() {
            f(i, t);
        }
        Corpus::try_from_parts(c.keys().to_vec(), texts).unwrap()
    }

    fn rule() -> ProportionalityConfig {
        ProportionalityConfig::default()
    }

    fn small_book_rule() -> ProportionalityConfig {
        ProportionalityConfig {
            min_verses: 5,
            ..Default::default()
        }
    }

    /// A cold whole-corpus analysis, in the final stable order.
    fn run(cfg: &ProportionalityConfig, target: &Corpus, source: Option<&Corpus>) -> Vec<Finding> {
        length_ratio_findings(target, source, cfg)
    }

    /// A resident drive, findings in the final stable order — the incremental
    /// path, as `analyze` runs it.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<ProportionalitySubstrate>,
        target: &Corpus,
        source: Option<&Corpus>,
        cfg: &ProportionalityConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_proportionality(true, cache, target, source, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start));
        out
    }

    /// Comparable rendering — key, span, score and both arg values.
    fn render(c: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::LengthRatio { ratio_pct, scope }) => {
                        format!("{ratio_pct:?}/{scope:?}")
                    }
                    _ => "-".to_string(),
                };
                format!("{}|{}|{:?}|{a}", c.key(f.key_idx), f.range.end, f.score, a = a)
            })
            .collect()
    }

    /// Two duplicate keys on both sides pair first-with-first,
    /// second-with-second (occurrence ordinal) — never positionally
    /// (first-source-match-wins), which would falsely flag the second
    /// duplicate as an outlier.
    #[test]
    fn pairs_duplicate_target_keys_to_duplicate_source_keys_by_occurrence_ordinal() {
        let base = "abcdefghij ".repeat(4); // 44 graphemes, this file's baseline unit
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        // Ordinary same-length verses (mild jitter so MAD > 0), enough to
        // clear the default `min_verses` book-distribution floor.
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(if v % 2 == 0 {
                format!("{base}x")
            } else {
                base.clone()
            });
        }
        // Two duplicate "GEN 1:59" keys on both sides. Ordinal 0 is 5x
        // longer; ordinal 1 matches the baseline. Correct pairing gives both
        // target duplicates a ~1.0 ratio (no outlier); positional
        // first-match pairing would instead compare target ordinal 1 (base
        // length) against source ordinal 0 (5x length), a false outlier.
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        source_keys.push(key("GEN", 59));
        source_texts.push(base.clone());
        target_keys.push(key("GEN", 59));
        target_texts.push(base.repeat(5));
        target_keys.push(key("GEN", 59));
        target_texts.push(base.clone());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();

        let findings = run(&rule(), &target, Some(&source));
        assert!(
            findings.is_empty(),
            "correct ordinal pairing keeps both duplicate verses at ratio ~1.0, no outliers: {findings:?}"
        );
    }

    /// More target duplicates of a key than the source has: the extra
    /// occurrences find no source counterpart at their ordinal and must be
    /// skipped entirely — never falling back to reuse an earlier ordinal's
    /// source text, which would wrongly fire them too.
    #[test]
    fn more_target_duplicates_than_source_are_skipped() {
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(spread_jitter(&base, v));
        }
        // Source has one "GEN 1:59" (5x, a genuine outlier length); target
        // has three. Only the first target occurrence has a source
        // counterpart; occurrences two and three must be skipped, not
        // silently re-paired against that same source text (which would
        // wrongly make all three fire instead of just the first).
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        for _ in 0..3 {
            target_keys.push(key("GEN", 59));
            target_texts.push(base.clone());
        }

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(
            findings.len(),
            1,
            "only the first duplicate pairs with the source's sole occurrence; \
             the extra two must be skipped, not reprocessed against the same \
             source text: {findings:?}"
        );
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 59));
    }

    /// More source duplicates of a key than the target has: the target's
    /// single occurrence pairs with the source's first occurrence only —
    /// the extra source occurrences are never consulted, however extreme
    /// their content, since nothing on the target side reaches their ordinal.
    #[test]
    fn more_source_duplicates_than_target_are_irrelevant() {
        let base = "abcdefghij ".repeat(4);
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(spread_jitter(&base, v));
        }
        // Target has one "GEN 1:59" (baseline length); source has three —
        // the first is 5x (a genuine outlier pairing for ordinal 0), the
        // other two are extreme/degenerate lengths that must have zero
        // influence since the target never reaches their ordinal.
        target_keys.push(key("GEN", 59));
        target_texts.push(base.clone());
        source_keys.push(key("GEN", 59));
        source_texts.push(base.repeat(5));
        source_keys.push(key("GEN", 59));
        source_texts.push("z".repeat(9_999));
        source_keys.push(key("GEN", 59));
        source_texts.push(String::new());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(
            findings.len(),
            1,
            "the target's sole occurrence pairs with the source's first \
             duplicate only; the other two extreme source duplicates must \
             be irrelevant: {findings:?}"
        );
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 59));
    }

    /// A complete-snapshot call where an earlier book's verse count shifts
    /// (growing or shrinking) must still resolve a later book's *stored*
    /// `RatioObs` correctly: `judge` rebases against the *current* call's
    /// `BookGroup::base`, not whatever base was current when the
    /// observation was reduced.
    #[test]
    fn earlier_book_shift_rebases_a_stored_proportionality_observation() {
        let base = "abcdefghij ".repeat(4);
        let r = rule();

        // GEN: `gen_len` baseline (mildly jittered) verses — enough to clear
        // `min_verses` alone or pooled with EXO. EXO: 5 verses, EXO 1:3 a 5x
        // outlier — the stored observation under test.
        let build_target = |gen_len: u16| {
            let mut keys = Vec::new();
            let mut texts = Vec::new();
            for v in 1..=gen_len {
                keys.push(key("GEN", v));
                texts.push(spread_jitter(&base, v));
            }
            for v in 1..=5u16 {
                keys.push(key("EXO", v));
                // Same jitter shape as GEN (keeps the tie/MAD structure
                // stable across the grown/shrunk variants below), except
                // verse 3, unconditionally overridden to the 5x outlier.
                let t = spread_jitter(&base, v);
                texts.push(if v == 3 { base.repeat(5) } else { t });
            }
            Corpus::try_from_parts(keys, texts).unwrap()
        };
        let build_source = |gen_len: u16| {
            let mut keys = Vec::new();
            let mut texts = Vec::new();
            for v in 1..=gen_len {
                keys.push(key("GEN", v));
                texts.push(base.clone());
            }
            for v in 1..=5u16 {
                keys.push(key("EXO", v));
                texts.push(base.clone());
            }
            Corpus::try_from_parts(keys, texts).unwrap()
        };

        // Reduce once with GEN at 60 verses. EXO's `RatioObs` are stored
        // book-local (`LocalKeyIdx`), independent of GEN's size.
        let target60 = build_target(60);
        let source60 = build_source(60);
        let mut cache = crate::substrate::SubstrateCache::new();
        let findings60 = resident(&mut cache, &target60, Some(&source60), &r);
        assert_eq!(findings60.len(), 1);
        assert_eq!(target60.key(findings60[0].key_idx), key("EXO", 3));

        // The SAME resident cache, answering for a call where GEN grew to 61
        // verses. EXO's chapter is untouched, so its observation is reused
        // verbatim — its addresses are chapter-local and materialization rebases
        // them through EXO's current layout block, whose global base has shifted.
        let target61 = build_target(61);
        let source61 = build_source(61);
        let grown = resident(&mut cache, &target61, Some(&source61), &r);
        assert_eq!(grown.len(), 1);
        assert_eq!(
            target61.key(grown[0].key_idx),
            key("EXO", 3),
            "EXO's retained ratio must resolve to EXO 1:3 even after GEN grew"
        );

        // And shrank to 59 verses — EXO's base shifts the other way.
        let target59 = build_target(59);
        let source59 = build_source(59);
        let shrunk = resident(&mut cache, &target59, Some(&source59), &r);
        assert_eq!(shrunk.len(), 1);
        assert_eq!(
            target59.key(shrunk[0].key_idx),
            key("EXO", 3),
            "EXO's retained ratio must resolve to EXO 1:3 even after GEN shrank"
        );
    }

    #[test]
    fn uniform_ratios_produce_nothing() {
        // Identical ratios everywhere → MAD == 0 → skip, no findings.
        let (target, source) = corpus(60, None, 1);
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn no_source_produces_nothing() {
        let (target, _) = corpus(60, Some(3), 5);
        assert!(run(&rule(), &target, None).is_empty());
    }

    #[test]
    fn book_under_min_verses_is_skipped() {
        // A gross outlier, but only 10 shared verses < default 50.
        let (target, source) = corpus(10, Some(3), 5);
        // Perturb lengths so MAD wouldn't be the reason for silence.
        let target = jitter(&target, |i, t| t.push_str(&"x".repeat(i)));
        let source = jitter(&source, |i, s| s.push_str(&"y".repeat(i / 2)));
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn outlier_fires_with_key_score_and_args() {
        // Mild length jitter so MAD > 0, plus one 5× verse.
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(target.key(f.key_idx), key("GEN", 3));
        assert_eq!(f.code, PROJECT_LENGTH_RATIO);
        assert_eq!(f.severity, Severity::Warning);
        // Whole-verse anchor.
        assert_eq!(
            f.range,
            Span {
                start: 0,
                end: target.text(f.key_idx).len() as u32
            }
        );
        // A 5× outlier saturates the confidence scale.
        assert_eq!(f.score, Some(1.0));
        let Some(FindingArgs::LengthRatio { ratio_pct, scope }) = f.args else {
            panic!("expected LengthRatio args");
        };
        assert!((ratio_pct - 500.0).abs() < 15.0, "ratio_pct = {ratio_pct}");
        // A single-book corpus: the book and project distributions coincide,
        // so the verse is an outlier in both.
        let LengthRatioScope::Both { book_z, project_z } = scope else {
            panic!("expected Both scope, got {scope:?}");
        };
        assert!(book_z > 2.5, "book_z = {book_z}");
        assert!(
            (book_z - project_z).abs() < 0.01,
            "single book ⇒ z should match"
        );
    }

    #[test]
    fn project_scope_flags_verses_a_small_book_cannot_judge_alone() {
        // GEN: 60 ~equal verses (a valid book distribution, no outlier).
        // EXO: 3 verses 5× longer — too few for a book distribution of their
        // own, but gross outliers against the pooled project. They fire on
        // Project scope only. GEN and EXO must each be a contiguous block
        // (`Corpus::try_from_parts`'s invariant), so EXO's keys are appended
        // after all of GEN's.
        let base = "abcdefghij ".repeat(4); // 44 graphemes
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=60 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(spread_jitter(&base, v));
        }
        for v in 1..=3 {
            let k = key("EXO", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(base.repeat(5));
        }
        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(findings.len(), 3);
        for f in &findings {
            assert!(target.key(f.key_idx).starts_with("EXO "));
            let Some(FindingArgs::LengthRatio { scope, .. }) = f.args else {
                panic!("expected LengthRatio args");
            };
            assert!(
                matches!(scope, LengthRatioScope::Project { .. }),
                "expected Project scope, got {scope:?}"
            );
        }
    }

    #[test]
    fn verses_missing_from_source_are_ignored() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        // A target-only verse with absurd length: no ratio, no finding.
        let mut target_keys = target.keys().to_vec();
        let mut target_texts = target.texts().to_vec();
        target_keys.push(key("GEN", 200));
        target_texts.push("z".repeat(10_000));
        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    /// The symmetric case: a key present only in the source (the target
    /// never presents it) is never looked up — `index_source` is consulted
    /// per *target* verse, so a source-only key has no target verse to
    /// anchor a finding to, regardless of how extreme its text is.
    #[test]
    fn keys_present_only_in_source_are_ignored() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let mut source_keys = source.keys().to_vec();
        let mut source_texts = source.texts().to_vec();
        source_keys.push(key("GEN", 200));
        source_texts.push("z".repeat(10_000));
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn empty_sides_are_skipped() {
        let (target, source) = corpus(60, None, 1);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });

        let mut target_keys = target.keys().to_vec();
        let mut target_texts = target.texts().to_vec();
        let mut source_keys = source.keys().to_vec();
        let mut source_texts = source.texts().to_vec();

        target_keys.push(key("GEN", 61));
        target_texts.push(String::new());
        source_keys.push(key("GEN", 61));
        source_texts.push("abc".to_string());

        target_keys.push(key("GEN", 62));
        target_texts.push("abc".to_string());
        source_keys.push(key("GEN", 62));
        source_texts.push(String::new());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();
        assert!(run(&rule(), &target, Some(&source)).is_empty());
    }

    #[test]
    fn min_verses_knob_activates_small_books() {
        let (target, source) = corpus(10, Some(3), 8);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let findings = run(&small_book_rule(), &target, Some(&source));
        assert_eq!(findings.len(), 1);
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 3));
    }

    #[test]
    fn editing_a_book_supersedes_its_prior_ratios() {
        // Reduce a corpus with an outlier, then a corrected edit; merging
        // supersedes the book so the outlier disappears.
        let r = rule();
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let mut cache = crate::substrate::SubstrateCache::new();
        assert_eq!(resident(&mut cache, &target, Some(&source), &r).len(), 1);

        // Fix verse 3 to a normal length; the resident cache remaps that chapter
        // and the outlier disappears.
        let mut texts = target.texts().to_vec();
        texts[2] = "abcdefghij ".repeat(4); // index 2 == "GEN 1:3"
        let fixed = Corpus::try_from_parts(target.keys().to_vec(), texts).unwrap();
        let after = resident(&mut cache, &fixed, Some(&source), &r);
        assert!(after.is_empty(), "{after:?}");
        assert_eq!(
            render(&fixed, &after),
            render(&fixed, &run(&r, &fixed, Some(&source)))
        );
    }

    #[test]
    fn re_reducing_a_book_with_no_usable_ratios_clears_stale_findings() {
        // A book that loses its source must supersede its prior ratios to
        // *empty* — not leave the prior reduction's stale findings standing.
        let r = rule();
        let (target, source) = corpus(60, Some(3), 5);
        let target = jitter(&target, |i, t| {
            if i % 2 == 0 {
                t.push('x');
            }
        });
        let mut cache = crate::substrate::SubstrateCache::new();
        assert_eq!(resident(&mut cache, &target, Some(&source), &r).len(), 1);

        // THE REFERENCE-REMOVAL INVALIDATION (plan §7.1's source-replace row).
        // The target text is byte-identical, so nothing in `chapter_hash` moved —
        // only `ObservationInputStamp::reference`, from `Present(h)` to `Absent`.
        // That must remap every chapter and empty the aggregate; a stamp that
        // ignored the reference would silently keep emitting the stale outlier.
        cache.reset_probes();
        let after = resident(&mut cache, &target, None, &r);
        assert!(after.is_empty(), "{after:?}");
        assert!(
            cache.mapped >= 1,
            "losing the reference must invalidate the observations it produced"
        );
        // And a reference APPEARING again re-maps and restores the finding — the
        // third stamp state, so `Absent` and `NotDeclared` cannot collapse.
        cache.reset_probes();
        let restored = resident(&mut cache, &target, Some(&source), &r);
        assert_eq!(restored.len(), 1);
        assert!(cache.mapped >= 1);
    }

    /// An edit maps and reduces exactly its own chapter, and a judging-knob change
    /// maps and reduces nothing (plan §12.4) — both config fields are read at
    /// judge, and the REFERENCE is not config.
    #[test]
    fn edit_locality_and_knob_isolation() {
        let r = rule();
        // Three chapters so an edit's locality is visible.
        let base = "abcdefghij ".repeat(4);
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut src_keys = Vec::new();
        let mut src_texts = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=20u16 {
                let k = format!("GEN {ch}:{v}");
                keys.push(k.clone());
                texts.push(if v % 2 == 0 {
                    format!("{base}x")
                } else {
                    base.clone()
                });
                src_keys.push(k);
                src_texts.push(base.clone());
            }
        }
        let source = Corpus::try_from_parts(src_keys, src_texts).unwrap();
        let mut cache = crate::substrate::SubstrateCache::new();
        let seeded = resident(
            &mut cache,
            &Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap(),
            Some(&source),
            &r,
        );
        assert!(seeded.is_empty(), "{seeded:?}");
        assert_eq!(cache.mapped, 3, "a cold call maps every chapter");

        // Blow up one verse of chapter 2.
        texts[25] = base.repeat(5);
        let edited = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
        cache.reset_probes();
        let inc = resident(&mut cache, &edited, Some(&source), &r);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(
            render(&edited, &inc),
            render(&edited, &run(&r, &edited, Some(&source)))
        );

        // `min_verses` above the whole corpus gates BOTH distributions off, so the
        // knob change is observable as silence without depending on how extreme a
        // z-score the synthetic outlier happens to reach.
        let strict = ProportionalityConfig {
            min_verses: 100_000,
            ..Default::default()
        };
        cache.reset_probes();
        let none = resident(&mut cache, &edited, Some(&source), &strict);
        assert_eq!(
            (cache.mapped, cache.reduced),
            (0, 0),
            "a knob is not an extraction input"
        );
        assert!(none.is_empty());
    }

    /// The duplicate-key occurrence ordinal is CHAPTER-local, which is what makes
    /// this substrate's boundary state `()`. Two `GEN 1:1`s and two `GEN 2:1`s: if
    /// the ordinal leaked across the chapter seam, chapter 2's first verse would
    /// pair with its SECOND source occurrence and its ratio would be wrong. The
    /// chapter-grained resident answer must equal the cold whole-corpus one.
    #[test]
    fn the_duplicate_key_ordinal_is_chapter_local() {
        let base = "abcdefghij ".repeat(4);
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut src_keys = Vec::new();
        let mut src_texts = Vec::new();
        for ch in 1..=2u16 {
            // The duplicated key first, twice, then ordinary verses so both
            // chapters can carry a distribution.
            for _ in 0..2 {
                keys.push(format!("GEN {ch}:1"));
                src_keys.push(format!("GEN {ch}:1"));
                src_texts.push(base.clone());
            }
            // The two target duplicates differ in length, so pairing them to the
            // wrong source occurrence is observable in the ratios.
            texts.push(base.clone());
            texts.push(base.repeat(5));
            for v in 2..=30u16 {
                let k = format!("GEN {ch}:{v}");
                keys.push(k.clone());
                texts.push(spread_jitter(&base, v));
                src_keys.push(k);
                src_texts.push(base.clone());
            }
        }
        let target = Corpus::try_from_parts(keys, texts).unwrap();
        let source = Corpus::try_from_parts(src_keys, src_texts).unwrap();
        let r = rule();
        let mut cache = crate::substrate::SubstrateCache::new();
        let inc = resident(&mut cache, &target, Some(&source), &r);
        let cold = run(&r, &target, Some(&source));
        assert_eq!(render(&target, &inc), render(&target, &cold));
        // Both chapters' 5x duplicates surface, which is the positive control:
        // an ordinal that slid would have mispaired one of them.
        assert_eq!(inc.len(), 2, "{:?}", render(&target, &inc));
    }

    /// Randomized edits on BOTH sides: a resident cache's findings always equal a
    /// cold analysis of the same target/reference pair (plan §12.6). The reference
    /// moves too, which is the half only a source-dependent substrate has.
    #[test]
    fn resident_proportionality_equals_cold_under_randomized_edits() {
        let base = "abcdefghij ".repeat(4);
        let shapes: Vec<String> = vec![
            base.clone(),
            format!("{base}x"),
            base.repeat(5),
            base.repeat(2),
            String::new(),
        ];
        let r = ProportionalityConfig {
            min_verses: 5,
            ..Default::default()
        };
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for ch in 1..=3u16 {
            for v in 1..=12u16 {
                keys.push(format!("GEN {ch}:{v}"));
                texts.push(base.clone());
            }
        }
        let mut src_texts = texts.clone();
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for step in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let vi = (state >> 33) as usize % texts.len();
            let si = (state >> 11) as usize % shapes.len();
            // Alternate which side moves, and every fourth step drop the
            // reference entirely and bring it back.
            if step % 2 == 0 {
                texts[vi] = shapes[si].clone();
            } else {
                src_texts[vi] = shapes[si].clone();
            }
            let target = Corpus::try_from_parts(keys.clone(), texts.clone()).unwrap();
            let source = Corpus::try_from_parts(keys.clone(), src_texts.clone()).unwrap();
            let src = if step % 4 == 3 { None } else { Some(&source) };
            let inc = resident(&mut cache, &target, src, &r);
            assert_eq!(
                render(&target, &inc),
                render(&target, &run(&r, &target, src)),
                "step {step}: resident result diverged from cold"
            );
        }
    }

    #[test]
    fn min_verses_zero_does_not_panic_on_an_empty_book() {
        // `min_verses` is caller-supplied (wasm config); 0 must not let an
        // empty ratio set reach `median([])`.
        let r = ProportionalityConfig {
            min_verses: 0,
            ..Default::default()
        };
        let (target, _) = corpus(3, None, 1);
        // No source ⇒ every book's sample is empty; judging must not trap.
        assert!(run(&r, &target, None).is_empty());
    }

    #[test]
    fn score_is_bounded_and_anchored_at_threshold() {
        assert_eq!(score_from_z(2.5, 2.5), 0.5);
        assert_eq!(score_from_z(5.0, 2.5), 1.0);
        assert_eq!(score_from_z(50.0, 2.5), 1.0);
    }

    /// ADR 0069: `z_long` and `z_short` are independent knobs. A long-side
    /// outlier fires under a permissive `z_long` even when `z_short` is set
    /// strict enough that no short-side deviation could ever clear it, and
    /// vice versa — each side's threshold is scored against ONLY its own
    /// side's z, never the other side's.
    #[test]
    fn long_and_short_thresholds_apply_independently() {
        let base = "abcdefghij ".repeat(4);
        let mut target_keys = Vec::new();
        let mut target_texts = Vec::new();
        let mut source_keys = Vec::new();
        let mut source_texts = Vec::new();
        for v in 1..=58u16 {
            let k = key("GEN", v);
            source_keys.push(k.clone());
            source_texts.push(base.clone());
            target_keys.push(k);
            target_texts.push(spread_jitter(&base, v));
        }
        // One long-side outlier (5x) and one short-side outlier (tiny).
        target_keys.push(key("GEN", 59));
        target_texts.push(base.repeat(5));
        source_keys.push(key("GEN", 59));
        source_texts.push(base.clone());
        target_keys.push(key("GEN", 60));
        target_texts.push("a".to_string());
        source_keys.push(key("GEN", 60));
        source_texts.push(base.clone());

        let target = Corpus::try_from_parts(target_keys, target_texts).unwrap();
        let source = Corpus::try_from_parts(source_keys, source_texts).unwrap();

        // Permissive long side, impossibly strict short side: only the
        // long-side (5x) outlier can fire.
        let long_only = ProportionalityConfig {
            z_long: 2.0,
            z_short: 1000.0,
            ..Default::default()
        };
        let findings = run(&long_only, &target, Some(&source));
        assert!(
            findings.iter().any(|f| target.key(f.key_idx) == key("GEN", 59)),
            "the long-side outlier must fire under a permissive z_long: {findings:?}"
        );
        assert!(
            findings.iter().all(|f| target.key(f.key_idx) != key("GEN", 60)),
            "the short-side outlier must NOT fire under an impossibly strict z_short: {findings:?}"
        );

        // The mirror image: permissive short side, impossibly strict long side.
        let short_only = ProportionalityConfig {
            z_long: 1000.0,
            z_short: 2.0,
            ..Default::default()
        };
        let findings = run(&short_only, &target, Some(&source));
        assert!(
            findings.iter().any(|f| target.key(f.key_idx) == key("GEN", 60)),
            "the short-side outlier must fire under a permissive z_short: {findings:?}"
        );
        assert!(
            findings.iter().all(|f| target.key(f.key_idx) != key("GEN", 59)),
            "the long-side outlier must NOT fire under an impossibly strict z_long: {findings:?}"
        );
    }

    /// The documented per-side abstain invariant, proven directly: when a
    /// side has exactly one strict deviation, that point's own deviation
    /// defines its side's MAD, so its z is pinned at exactly
    /// `MAD_TO_SIGMA` (0.6745) if it had to use its OWN one-sided MAD —
    /// exactly the collapse the per-side data floor (`SIDE_DATA_FLOOR`)
    /// exists to route around. Below the floor, `Spread::gated` falls back
    /// to the pooled symmetric MAD (the pre-ADR-0069 design) instead, so
    /// this outlier fires exactly where the OLD symmetric-MAD rule would
    /// have fired.
    #[test]
    fn a_lone_deviation_fires_via_the_pooled_fallback() {
        let (target, source) = corpus(60, Some(3), 50); // 1 gross outlier, else identical
        // Mild below-typical jitter on the background (skipping the
        // outlier verse itself) gives the pooled symmetric MAD genuine,
        // nonzero signal — while the outlier remains the ONLY point above
        // the median, i.e. below `SIDE_DATA_FLOOR` on its own side.
        let target = jitter(&target, |i, t| {
            if i != 2 && i % 2 == 0 {
                t.truncate(t.len() - 2);
            }
        });
        let findings = run(&rule(), &target, Some(&source));
        assert_eq!(
            findings.len(),
            1,
            "the pooled fallback must catch what the (undertrusted) lone-point \
             one-sided MAD alone could not: {findings:?}"
        );
        assert_eq!(target.key(findings[0].key_idx), key("GEN", 3));
    }

    /// A side with `>= SIDE_DATA_FLOOR` strict deviations is trusted with
    /// its OWN one-sided MAD, never silently substituted with the pooled
    /// symmetric one — proven directly at the `Spread` level, where the
    /// two candidate MADs can be told apart even where the whole-rule
    /// output alone might not distinguish them.
    #[test]
    fn a_well_populated_side_uses_its_own_mad_not_the_pooled_fallback() {
        // A below-median cluster of 10 (tight: deviation 1 each), a big
        // tie block anchoring the true median at 10.0 regardless of how
        // the flanking clusters' sizes interact with `median_in_place`'s
        // even/odd averaging, and an above-median cluster of 10 (loose:
        // deviation 5 each) — comfortably past the floor on both sides,
        // with deliberately different spreads so "own" and "pooled" are
        // numerically distinguishable.
        let mut ratios = Vec::new();
        for _ in 0..10 {
            ratios.push(RatioBits(9.0));
        }
        for _ in 0..15 {
            ratios.push(RatioBits(10.0));
        }
        for _ in 0..10 {
            ratios.push(RatioBits(15.0));
        }
        let spread = spread_of(ratios.iter());
        assert_eq!(spread.med, 10.0, "the tie block must anchor the median exactly");
        assert!(spread.n_above >= SIDE_DATA_FLOOR && spread.n_below >= SIDE_DATA_FLOOR);
        assert_ne!(
            spread.mad_above, spread.mad_symmetric,
            "sanity: the own-side and pooled MADs must actually differ here, \
             or this test would pass vacuously"
        );
        let gated = spread.gated(0).expect("count > 0 and min_verses=0 always judges");
        assert_eq!(
            gated.mad_above,
            Some(spread.mad_above),
            "a well-populated side must use its OWN MAD, not the pooled fallback"
        );
        assert_eq!(gated.mad_below, Some(spread.mad_below));
    }

    /// The exact boundary the floor draws: one deviation short of
    /// `SIDE_DATA_FLOOR` falls back to the pooled MAD; right AT the floor,
    /// the side's own MAD is trusted. Both builds keep the below side
    /// (`n_above + 1` points) comfortably clear of the floor throughout, so
    /// only the above side's count crosses it.
    #[test]
    fn per_side_data_floor_boundary() {
        // A small tie block anchors the median at exactly 10.0 regardless
        // of the flanking clusters' varying sizes (the thing under test) —
        // `assert_eq!(med, 10.0)` below is the proof this construction
        // does what it claims, not an assumption. Deliberately small (3,
        // not e.g. 9): an oversized tie block would itself dominate more
        // than half the sample with zero-deviations, collapsing the
        // POOLED symmetric MAD to 0 too and making the fallback
        // assertions vacuous — the same shape of pitfall this whole test
        // exists to keep out of the judge.
        let build = |n_above: usize| {
            let n_below = n_above + 1;
            let mut ratios = Vec::new();
            for _ in 0..n_below {
                ratios.push(RatioBits(9.0));
            }
            for _ in 0..3 {
                ratios.push(RatioBits(10.0));
            }
            for _ in 0..n_above {
                ratios.push(RatioBits(15.0));
            }
            spread_of(ratios.iter())
        };

        let below_floor = build(SIDE_DATA_FLOOR - 1);
        assert_eq!(below_floor.med, 10.0);
        assert_eq!(below_floor.n_above, SIDE_DATA_FLOOR - 1);
        let gated = below_floor.gated(0).unwrap();
        assert_eq!(
            gated.mad_above,
            Some(below_floor.mad_symmetric),
            "one short of the floor: the above side must fall back to the pooled MAD"
        );
        assert_ne!(
            below_floor.mad_symmetric, below_floor.mad_above,
            "sanity: fallback and own-side MAD must differ here, or the assertion above \
             would pass whichever branch `gated` took"
        );

        let at_floor = build(SIDE_DATA_FLOOR);
        assert_eq!(at_floor.med, 10.0);
        assert_eq!(at_floor.n_above, SIDE_DATA_FLOOR);
        let gated = at_floor.gated(0).unwrap();
        assert_eq!(
            gated.mad_above,
            Some(at_floor.mad_above),
            "right at the floor: the above side must use its OWN MAD"
        );
    }

    /// The selection median must agree with the sorting one it replaced, on both
    /// parities and whatever order the input arrives in — the prefix-maximum step
    /// is the part that would be wrong if `select_nth_unstable_by`'s partition
    /// contract were misread.
    #[test]
    fn median_handles_even_and_odd() {
        assert_eq!(median_in_place(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median_in_place(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median_in_place(&mut [1.0]), 1.0);
        assert_eq!(median_in_place(&mut [2.0, 1.0]), 1.5);
        // Duplicates around the split, and a reverse-sorted input.
        assert_eq!(median_in_place(&mut [5.0, 5.0, 1.0, 1.0]), 3.0);
        assert_eq!(median_in_place(&mut [9.0, 8.0, 7.0, 6.0, 5.0]), 7.0);
        // Bit-for-bit against a sorting median over a pseudo-random sample, both
        // parities — this is the equivalence the aggregate's correctness rests on.
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for n in [7usize, 8, 63, 64, 255, 256] {
            let mut v: Vec<f64> = (0..n)
                .map(|_| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    f64::from((state >> 40) as u32) / 1024.0
                })
                .collect();
            let mut sorted = v.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let want = if n % 2 == 1 {
                sorted[n / 2]
            } else {
                (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
            };
            assert_eq!(median_in_place(&mut v).to_bits(), want.to_bits(), "n = {n}");
        }
    }
}
