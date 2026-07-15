//! Casing — two rules over one word lexicon, gated by learned mark trust.
//!
//! ADR 0051 rebuilt casing on a per-word case table (superseding ADR 0035's
//! per-glyph dominance). An occurrence's case is the OR of two causes: the
//! position forces uppercase, or the word is intrinsically capitalized.
//! Censoring is one-directional — uppercase at a forced position is
//! uninformative about the word; lowercase is informative everywhere. From that
//! model two rules judge the 2×2 of (position, word's intrinsic class):
//!
//! - [`SentenceInitialLowercase`] (`case.sentence-initial-lowercase`) — a
//!   **forced-position** lowercase site: `score = habit(class) × rarity(this
//!   word's forced-lowercase recurrence)`, where `habit` is the lexicon-
//!   restricted capitalize-after-terminal dominance.
//! - [`InconsistentWordCasing`] (`case.inconsistent-word-casing`) — a lowercase
//!   site of an **intrinsically-capitalized** word: `score = dominance(word's
//!   soft-censored capitalized share) × rarity(word's lowercase recurrence)`.
//!
//! ## ADR 0052 — `terminal_strength` gates flagging, weights the discount
//!
//! A forced position now carries its boundary **class**, not just a glyph: the
//! terminal mark plus whether a close-quote intervened before the next word
//! (`.` and `."` are separate classes). Per corpus, per class, two witnesses
//! combine noisy-OR into `trust(class) = 1 − (1 − s_case)(1 − s_reshuffle)`:
//!
//! - `s_case` — the case-follow witness (bicameral only): the lexicon-
//!   restricted capitalize-after-class rate. This *is* the ADR 0051 habit,
//!   reused. Absent (caseless / no lexicon-lowercase followers) ⇒ 0.
//! - `s_reshuffle` — the word-reshuffle witness (case-free): the class's
//!   following-word distribution's differentness from the corpus word-start
//!   baseline (Dunning G² / Fisher via [`crate::analysis::association`]),
//!   **guarded** by total-variation agreement with the reference terminal's
//!   aftermath. Plain differentness cannot rank marks (a genealogy list-comma
//!   reshapes its neighborhood as much as a period); all discriminating power
//!   lives in the agreement guard.
//!
//! Consumption, per ADR 0052's "verdicts gate, evidence weighs":
//!
//! - **Flagging is gated, never scaled.** A forced site is scored with the
//!   *unchanged* `habit × rarity` iff `trust(class) ≥ cfg.trust_gate`; below the
//!   gate its positional channel is not scored. Multiplying trust into the score
//!   would compound honest ~0.97 factors below the floor (the measured erosion
//!   of 373 genuine findings); since `habit ≤ trust` (the case witness is a term
//!   of the noisy-OR), any site clearing the 0.95 floor already has `trust` well
//!   above the 0.90 gate — the gate only readmits erosion victims and admits the
//!   promoted quote-context classes.
//! - **The censoring discount is weighted.** Forced-position uppercase re-enters
//!   a word's intrinsic profile at `1 − trust(class) · habit(class)`: a capital
//!   after a distrusted mark is not position-explained and returns to the
//!   profile. Here trust genuinely is proportional, so it multiplies.
//! - **Quote-context sites are conditionally forced.** A `."`/`:"` class the
//!   walk once collapsed to mid-flow becomes a forced class when `trust >
//!   PROMOTE_BAR`; below the bar it **falls back to mid-flow** exactly as ADR
//!   0051 (no loss). Trust is only known at judge, so the walk records every
//!   quote-context tally by class and the judge folds untrusted classes back.
//!
//! ## Stats shape and merge semantics (raw, per book)
//!
//! Per book, [`CasingStats`] stores a word→[`WordStats`] table of **raw**
//! tallies (mid-flow upper/lower; forced upper/lower split by the bare terminal
//! glyph, by the quote-context glyph, and book-initial separately). Nothing is
//! censored and no trust is computed at reduce: the lexicon classification, the
//! per-class habit, and the two witnesses are corpus-wide, so they are all
//! **judge-time** arithmetic over the merged table. The W2 aggregates the ADR
//! calls for — per-class following-word counts and the baseline word-start
//! distribution — are *reindexed at judge from these same per-word tallies*
//! (the reshuffle witness is case-free, so a word's forced upper+lower is its
//! occurrence count after that class); no second stored table, no size cost
//! beyond the quote-context split. This keeps book-supersede sound (a book
//! carries its own counts, replaced wholesale on edit) and `reduce` one walk.
//!
//! **Pruning.** As ADR 0051: the sole per-book-safe drop is an *uncased-only*
//! word (a caseless-script token) — it yields no candidate site and never enters
//! the lexicon-lowercase habit or the (bicameral) witnesses. Every cased word is
//! kept with raw tallies.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::analysis::association::Table2;
use crate::charclass::class_of;
use crate::config::CasingConfig;
use crate::corpus::{BookGroup, Books, Corpus, LocalKeyIdx, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::signals::case_shape::{CaseShape, case_shape};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::stream;

pub const SENTENCE_INITIAL_LOWERCASE: RuleId = RuleId::SentenceInitialLowercase;
pub const INCONSISTENT_WORD_CASING: RuleId = RuleId::InconsistentWordCasing;

// ── Frozen witness internals (ADR 0052; documented constants, not knobs). ────

/// A word must appear as a word-start at least this often to be a reshuffle
/// juror (the Zipf gate).
const JUROR_MIN: u64 = 10;
/// A boundary class needs at least this many forced events for its witnesses to
/// be estimated; below it both are too thin and the class earns no trust.
const CLASS_MIN_EVENTS: u64 = 30;
/// A quote-context class is *promoted* to a forced class (censored in the
/// intrinsic channel, given a habit key) only above this trust. Below it the
/// class falls back to mid-flow exactly as ADR 0051. Deliberately far below the
/// `trust_gate`: promotion controls censoring/lexicon fold, the gate controls
/// positional flagging.
const PROMOTE_BAR: f64 = 0.5;
/// The W2 differentness sigmoid `logistic((dev − THR)/SCALE)` on the
/// standardized multinomial-G² deviate — a gentle floor that only zeroes out
/// no-structure classes; the ranking power lives in the agreement guard.
const W2_SIGMOID_THR: f64 = 8.0;
const W2_SIGMOID_SCALE: f64 = 6.0;

/// First-letter case of a word, from its first grapheme's base scalar.
/// `Uncased` is a caseless letter — evidence for neither convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Uncased,
}

/// A boundary class: the candidate terminal mark plus whether a close-quote
/// intervened before the next word. `.` and `."` are distinct classes, each
/// earning its own trust. In-memory only (judge-time); the wire stores the two
/// glyph maps split by `quoted` (see [`WordStats`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassKey {
    pub mark: char,
    pub quoted: bool,
}

/// The structural position class of a word, fixed at its first letter and
/// defined *before any casing knowledge*. Forced right after an attached
/// terminal (bare, or with an intervening close-quote), or book-initial.
/// Everything else — including a token after *non-quote* intervening
/// punctuation (`...`) — is [`Midflow`](PosClass::Midflow). Verse-initial is
/// NOT forced (`CLAUDE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PosClass {
    /// The first word of the book — forced with no terminal glyph.
    BookInitial,
    /// A word whose first letter consumed an attached terminal (carried across
    /// verse seams). The [`ClassKey`] is the positional habit / trust key.
    ForcedAfterTerminal(ClassKey),
    /// Not position-forced: uppercase here is intrinsic to the word.
    Midflow,
}

impl PosClass {
    pub(crate) fn is_forced(self) -> bool {
        !matches!(self, PosClass::Midflow)
    }

    /// Descriptive `(glyph, quoted)` for the finding args (ADR 0048/0052).
    fn habit_glyph(self) -> (Option<char>, bool) {
        match self {
            PosClass::ForcedAfterTerminal(ck) => (Some(ck.mark), ck.quoted),
            _ => (None, false),
        }
    }
}

/// Forced-position first-letter tallies after one key. Raw and mergeable.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct ForcedTally {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    upper: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    lower: u32,
}

/// `skip_serializing_if` predicate — the per-word table is dominated by zero
/// tallies, so omitting them from the wire is a large, lossless size win.
#[cfg(feature = "serde")]
fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[cfg(feature = "serde")]
fn is_default_tally(t: &ForcedTally) -> bool {
    t.upper == 0 && t.lower == 0
}

#[cfg(feature = "serde")]
fn is_empty_map(m: &BTreeMap<char, ForcedTally>) -> bool {
    m.is_empty()
}

impl ForcedTally {
    fn add(&mut self, o: &ForcedTally) {
        self.upper += o.upper;
        self.lower += o.lower;
    }
    fn upper(&self) -> u64 {
        u64::from(self.upper)
    }
    fn lower(&self) -> u64 {
        u64::from(self.lower)
    }
    fn total(&self) -> u64 {
        self.upper() + self.lower()
    }
}

/// One word's raw case tallies within one book. Mid-flow upper/lower (the
/// intrinsic profile), forced upper/lower split by the *bare* terminal glyph
/// (`after_glyph`) and by the *quote-context* glyph (`after_quote`, the `."`
/// classes ADR 0051 discarded to mid-flow), and book-initial. All raw — no
/// censoring, no trust — so book-supersede holds.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct WordStats {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    mid_upper: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_zero"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    mid_lower: u32,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_default_tally"))]
    #[cfg_attr(feature = "wasm", tsify(optional))]
    book_initial: ForcedTally,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_empty_map"))]
    #[cfg_attr(
        feature = "wasm",
        tsify(optional, type = "Record<string, ForcedTally>")
    )]
    after_glyph: BTreeMap<char, ForcedTally>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "is_empty_map"))]
    #[cfg_attr(
        feature = "wasm",
        tsify(optional, type = "Record<string, ForcedTally>")
    )]
    after_quote: BTreeMap<char, ForcedTally>,
}

impl WordStats {
    /// Sum another book's counts for the same word into this (corpus-wide
    /// aggregation at judge).
    fn add(&mut self, o: &WordStats) {
        self.mid_upper += o.mid_upper;
        self.mid_lower += o.mid_lower;
        self.book_initial.add(&o.book_initial);
        for (g, t) in &o.after_glyph {
            self.after_glyph.entry(*g).or_default().add(t);
        }
        for (g, t) in &o.after_quote {
            self.after_quote.entry(*g).or_default().add(t);
        }
    }

    fn record(&mut self, pos: PosClass, case: Case) {
        match (pos, case) {
            (_, Case::Uncased) => {}
            (PosClass::Midflow, Case::Upper) => self.mid_upper += 1,
            (PosClass::Midflow, Case::Lower) => self.mid_lower += 1,
            (PosClass::BookInitial, Case::Upper) => self.book_initial.upper += 1,
            (PosClass::BookInitial, Case::Lower) => self.book_initial.lower += 1,
            (PosClass::ForcedAfterTerminal(ck), c) => {
                let map = if ck.quoted {
                    &mut self.after_quote
                } else {
                    &mut self.after_glyph
                };
                let t = map.entry(ck.mark).or_default();
                match c {
                    Case::Upper => t.upper += 1,
                    Case::Lower => t.lower += 1,
                    Case::Uncased => {}
                }
            }
        }
    }

    /// True iff the word has ≥1 cased word-start — the pruning predicate.
    fn has_case(&self) -> bool {
        self.mid_upper > 0
            || self.mid_lower > 0
            || self.book_initial.total() > 0
            || self.after_glyph.values().any(|t| t.total() > 0)
            || self.after_quote.values().any(|t| t.total() > 0)
    }

    // ── Fold-invariant raw sums (position labels don't affect these). ────────
    fn all_upper(&self) -> u64 {
        u64::from(self.mid_upper)
            + self.book_initial.upper()
            + self
                .after_glyph
                .values()
                .map(ForcedTally::upper)
                .sum::<u64>()
            + self
                .after_quote
                .values()
                .map(ForcedTally::upper)
                .sum::<u64>()
    }
    fn all_lower(&self) -> u64 {
        u64::from(self.mid_lower)
            + self.book_initial.lower()
            + self
                .after_glyph
                .values()
                .map(ForcedTally::lower)
                .sum::<u64>()
            + self
                .after_quote
                .values()
                .map(ForcedTally::lower)
                .sum::<u64>()
    }
    fn all_total(&self) -> u64 {
        self.all_upper() + self.all_lower()
    }

    /// The **baseline** mid pool (ADR 0051): mid-flow plus *all* quote-context
    /// tallies — the pool ADR 0051 saw, since it collapsed `."` to mid-flow.
    /// The lexicon classification is frozen here so promoting a quote class
    /// never moves a word's intrinsic verdict off its baseline.
    fn baseline_mid(&self) -> (u64, u64) {
        let mut up = u64::from(self.mid_upper);
        let mut lo = u64::from(self.mid_lower);
        for t in self.after_quote.values() {
            up += t.upper();
            lo += t.lower();
        }
        (up, lo)
    }

    /// Hard lexicon class over the baseline mid pool: mid-flow-lower-dominant.
    fn is_lexicon_lower(&self, z: f64) -> bool {
        let (up, lo) = self.baseline_mid();
        let n = up + lo;
        n > 0 && wilson_lower_bound(lo, n, z) > 0.5
    }
}

/// Cached casing statistics, keyed by book so an edit supersedes only its book.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CasingStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookCasing>"))]
    pub(crate) per_book: BTreeMap<Box<str>, BookCasing>,
}

/// One book's contribution: the pruned word table plus the cased-word-start
/// count that drives the emergent gate.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub(crate) struct BookCasing {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, WordStats>"))]
    words: BTreeMap<String, WordStats>,
    /// Cased word-start observations in the book — the emergent gate input,
    /// counted before pruning.
    cased_starts: u32,
}

impl CasingStats {
    /// Book-level supersede: books in `other` replace those in `self`.
    pub(crate) fn merge(mut self, other: CasingStats) -> CasingStats {
        for (book, bc) in other.per_book {
            self.per_book.insert(book, bc);
        }
        self
    }

    /// Drop a book's contribution.
    pub(crate) fn remove_book(&mut self, slug: &str) {
        self.per_book.remove(slug);
    }
}

/// Wilson lower bound with a fractional success count `k` (the soft-censoring
/// reweight yields a fractional upper), otherwise identical to
/// [`crate::evidence::wilson_lower_bound`].
fn wilson_lower_bound_f(k: f64, n: f64, z: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    let z = z.max(0.0);
    let p = (k / n).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}

/// The absolute linear recurrence knee (ADR 0050/0051): a hapax minority scores
/// `1`, fading linearly to `0` past `k`.
fn rarity(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

fn logistic(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

// ── W2 word-reshuffle machinery (case-free; ADR 0052). ──────────────────────

/// Aggregate per-juror 2×2 association (Dunning G² / Fisher) of a class's
/// aftermath vs the corpus baseline, standardized: under the null each juror's
/// statistic is ~χ²₁ (mean 1), so `(Σ − df)/√(2·df)` is comparable across corpus
/// sizes. `base` is the word-start distribution and **includes** the after-c
/// occurrences, so the "elsewhere" column subtracts them out.
fn reshuffle_deviate(
    after: &HashMap<&str, u64>,
    base: &HashMap<&str, u64>,
    jurors: &[&str],
) -> f64 {
    let n_after: u64 = jurors
        .iter()
        .map(|w| after.get(w).copied().unwrap_or(0))
        .sum();
    let n_base: u64 = jurors
        .iter()
        .map(|w| base.get(w).copied().unwrap_or(0))
        .sum();
    if n_after == 0 || n_base <= n_after {
        return 0.0;
    }
    let n_else = n_base - n_after;
    let mut sum = 0.0;
    let mut df = 0u64;
    for w in jurors {
        let a = after.get(w).copied().unwrap_or(0);
        let total_w = base.get(w).copied().unwrap_or(0);
        if total_w == 0 {
            continue;
        }
        df += 1;
        let b = n_after - a;
        let c = total_w.saturating_sub(a);
        let d = n_else.saturating_sub(c);
        sum += Table2::new(a, b, c, d).association_score();
    }
    let df = df.max(1);
    (sum - df as f64) / (2.0 * df as f64).sqrt()
}

/// Total-variation distance `½·Σ|p_w − q_w|` between two juror distributions —
/// a size-independent effect size in `[0, 1]`; 0 iff the distributions match.
fn tv_distance(p: &HashMap<&str, u64>, q: &HashMap<&str, u64>, jurors: &[&str]) -> f64 {
    let np: u64 = jurors.iter().map(|w| p.get(w).copied().unwrap_or(0)).sum();
    let nq: u64 = jurors.iter().map(|w| q.get(w).copied().unwrap_or(0)).sum();
    if np == 0 || nq == 0 {
        return 1.0;
    }
    let sum: f64 = jurors
        .iter()
        .map(|w| {
            let pp = p.get(w).copied().unwrap_or(0) as f64 / np as f64;
            let qq = q.get(w).copied().unwrap_or(0) as f64 / nq as f64;
            (pp - qq).abs()
        })
        .sum();
    (0.5 * sum).clamp(0.0, 1.0)
}

/// Per-class trust computed from the merged word table (ADR 0052). Returns the
/// noisy-OR trust for every candidate class (≥ `CLASS_MIN_EVENTS`); a class
/// absent from the map has trust 0. The W2 aggregates (per-class following-word
/// counts, baseline word-start distribution) are reindexed here from the
/// per-word forced tallies — the reshuffle witness is case-free, so a word's
/// forced upper+lower after a class is its occurrence count there.
fn build_trust(words: &HashMap<String, WordStats>, z: f64) -> HashMap<ClassKey, f64> {
    // Baseline word-start distribution + per-class aftermath (reindex).
    let mut word_start_total: HashMap<&str, u64> = HashMap::new();
    let mut after: HashMap<ClassKey, HashMap<&str, u64>> = HashMap::new();
    for (key, w) in words {
        let key = key.as_str();
        let total = w.all_total();
        if total == 0 {
            continue;
        }
        *word_start_total.entry(key).or_default() += total;
        for (m, t) in &w.after_glyph {
            if t.total() > 0 {
                *after
                    .entry(ClassKey {
                        mark: *m,
                        quoted: false,
                    })
                    .or_default()
                    .entry(key)
                    .or_default() += t.total();
            }
        }
        for (m, t) in &w.after_quote {
            if t.total() > 0 {
                *after
                    .entry(ClassKey {
                        mark: *m,
                        quoted: true,
                    })
                    .or_default()
                    .entry(key)
                    .or_default() += t.total();
            }
        }
    }

    let jurors: Vec<&str> = word_start_total
        .iter()
        .filter(|&(_, &n)| n >= JUROR_MIN)
        .map(|(&k, _)| k)
        .collect();

    let kept: Vec<ClassKey> = after
        .iter()
        .filter(|(_, c)| c.values().sum::<u64>() >= CLASS_MIN_EVENTS)
        .map(|(&k, _)| k)
        .collect();
    if kept.is_empty() {
        return HashMap::new();
    }

    // W1 case-follow per class: capitalize dominance over lexicon-lowercase
    // followers — exactly ADR 0051's per-glyph habit, re-derived per class.
    let mut w1: HashMap<ClassKey, (u64, u64)> = HashMap::new();
    for w in words.values() {
        if !w.is_lexicon_lower(z) {
            continue;
        }
        for (m, t) in &w.after_glyph {
            let e = w1
                .entry(ClassKey {
                    mark: *m,
                    quoted: false,
                })
                .or_default();
            e.0 += t.upper();
            e.1 += t.total();
        }
        for (m, t) in &w.after_quote {
            let e = w1
                .entry(ClassKey {
                    mark: *m,
                    quoted: true,
                })
                .or_default();
            e.0 += t.upper();
            e.1 += t.total();
        }
    }

    struct Prelim {
        s_case: f64,
        case_seen: bool,
        diff: f64,
        events: u64,
    }
    let mut prelim: HashMap<ClassKey, Prelim> = HashMap::new();
    for &ck in &kept {
        let a = &after[&ck];
        let dev = reshuffle_deviate(a, &word_start_total, &jurors);
        let diff = logistic((dev - W2_SIGMOID_THR) / W2_SIGMOID_SCALE);
        let (s_case, case_seen) = match w1.get(&ck) {
            Some(&(up, total)) if total > 0 => (wilson_lower_bound(up, total, z), true),
            _ => (0.0, false),
        };
        prelim.insert(
            ck,
            Prelim {
                s_case,
                case_seen,
                diff,
                events: a.values().sum(),
            },
        );
    }

    // Reference terminal for the agreement guard: the highest-VOLUME strongly-
    // case-trusted BARE class (`.` in Latin corpora), so the canonical
    // terminator anchors the comparison and does not erode itself. Ties break by
    // mark for determinism. Caseless fallbacks: highest-differentness bare,
    // then any highest-differentness class.
    let by_diff = |pred: &dyn Fn(&ClassKey) -> bool| {
        prelim
            .iter()
            .filter(|(ck, _)| pred(ck))
            .max_by(|(a, pa), (b, pb)| {
                pa.diff
                    .partial_cmp(&pb.diff)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.mark.cmp(&b.mark))
            })
            .map(|(&ck, _)| ck)
    };
    let reference = prelim
        .iter()
        .filter(|(ck, p)| p.case_seen && !ck.quoted && p.s_case >= 0.5)
        .max_by(|(a, pa), (b, pb)| pa.events.cmp(&pb.events).then_with(|| a.mark.cmp(&b.mark)))
        .map(|(&ck, _)| ck)
        .or_else(|| by_diff(&|ck| !ck.quoted))
        .or_else(|| by_diff(&|_| true));

    // Signature agreement: `1 − TV(after_c, ref) / TV(baseline, ref)` — how much
    // closer to the reference aftermath the class sits than a random word-start.
    // A real terminal resets to the sentence-start distribution; a list
    // separator's aftermath is its own, so agreement collapses (the genealogy
    // guard plain differentness cannot supply).
    let ref_after = reference.map(|r| &after[&r]);
    let ref_base_tv = ref_after.map(|ra| tv_distance(&word_start_total, ra, &jurors).max(1e-6));

    let mut trust = HashMap::with_capacity(prelim.len());
    for (&ck, p) in &prelim {
        let agree = if Some(ck) == reference {
            1.0
        } else if let (Some(ra), Some(rbt)) = (ref_after, ref_base_tv) {
            (1.0 - tv_distance(&after[&ck], ra, &jurors) / rbt).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let s_reshuffle = p.diff * agree;
        trust.insert(ck, 1.0 - (1.0 - p.s_case) * (1.0 - s_reshuffle));
    }
    trust
}

/// The lexicon-restricted per-class habit, plus the corpus trust map (ADR
/// 0052). Built corpus-wide at judge over the merged table.
///
/// Owns its word table (rather than borrowing `&'a str` out of the input
/// `CasingStats`) specifically so it can be cached independent of the
/// caller's borrow — see [`Model::build`]'s within-process memo (perf note
/// below `judge_casing`: both casing rules build the identical model from
/// the same merged stats within one `analyze` call).
struct Model {
    words: HashMap<String, WordStats>,
    /// Per class trust; `None`-keyed book-initial is always fully trusted.
    trust: HashMap<ClassKey, f64>,
    /// Lexicon-restricted capitalize-after-class counts (up, total). `None` =
    /// book-initial. A quote class is present only when promoted.
    habit: HashMap<Option<ClassKey>, (u64, u64)>,
    z: f64,
    gate: f64,
}

/// A convention factor pair: the dominance the site breaks and its rarity
/// inputs. `score = dominance × rarity(minority, k)`.
#[derive(Debug, Clone, Copy)]
pub struct Factors {
    pub dominance: f64,
    pub minority: u64,
    pub opportunities: u64,
    pub raw_major: u64,
    pub raw_total: u64,
}

// Within-process memo for `Model::build` (perf note above `judge_casing`):
// `SentenceInitialLowercase` and `InconsistentWordCasing` both build the
// identical model from the same merged `CasingStats` + `CasingConfig` inside
// one `analyze_stateful` call — the Fisher/G² association math in
// `build_trust` is ~44% of everything-on self-time on English, so rebuilding
// it twice per call is pure waste. Keyed by *content* equality (both
// `CasingStats` and `CasingConfig` already derive `PartialEq`), not by
// reference identity — the two calls pass distinct clones with identical
// content, so identity-keying would never hit. A size-2 LRU is enough to
// catch the two adjacent judge calls; it is not a correctness dependency —
// a miss just rebuilds.
thread_local! {
    static MODEL_CACHE: RefCell<Vec<(CasingStats, CasingConfig, Arc<Model>)>> =
        const { RefCell::new(Vec::new()) };
}

const MODEL_CACHE_CAP: usize = 2;

impl Model {
    fn build(stats: &CasingStats, cfg: &CasingConfig) -> Arc<Model> {
        if let Some(hit) = MODEL_CACHE.with(|c| {
            c.borrow()
                .iter()
                .find(|(s, c, _)| s == stats && c == cfg)
                .map(|(_, _, m)| Arc::clone(m))
        }) {
            return hit;
        }

        let model = Arc::new(Self::build_uncached(stats, cfg));

        MODEL_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= MODEL_CACHE_CAP {
                c.remove(0);
            }
            c.push((stats.clone(), *cfg, Arc::clone(&model)));
        });

        model
    }

    fn build_uncached(stats: &CasingStats, cfg: &CasingConfig) -> Model {
        let z = clamp_z(cfg.confidence_z);
        let gate = f64::from(clamp_unit(cfg.trust_gate));
        // Corpus-wide word table: sum each book's raw tallies.
        let mut words: HashMap<String, WordStats> = HashMap::new();
        for bc in stats.per_book.values() {
            for (key, w) in &bc.words {
                words.entry(key.clone()).or_default().add(w);
            }
        }

        let trust = build_trust(&words, z);

        // Lexicon-restricted per-class habit over the words the (baseline)
        // lexicon calls intrinsically lowercase. Bare glyphs and book-initial
        // always contribute (structurally forced); a quote class contributes
        // only when promoted, so trust adds the quote channel without moving the
        // bare-terminal convention.
        let mut habit: HashMap<Option<ClassKey>, (u64, u64)> = HashMap::new();
        for w in words.values() {
            if !w.is_lexicon_lower(z) {
                continue;
            }
            if w.book_initial.total() > 0 {
                let e = habit.entry(None).or_default();
                e.0 += w.book_initial.upper();
                e.1 += w.book_initial.total();
            }
            for (m, t) in &w.after_glyph {
                let e = habit
                    .entry(Some(ClassKey {
                        mark: *m,
                        quoted: false,
                    }))
                    .or_default();
                e.0 += t.upper();
                e.1 += t.total();
            }
            for (m, t) in &w.after_quote {
                let ck = ClassKey {
                    mark: *m,
                    quoted: true,
                };
                if trust.get(&ck).copied().unwrap_or(0.0) > PROMOTE_BAR {
                    let e = habit.entry(Some(ck)).or_default();
                    e.0 += t.upper();
                    e.1 += t.total();
                }
            }
        }

        Model {
            words,
            trust,
            habit,
            z,
            gate,
        }
    }

    fn trust_class(&self, ck: ClassKey) -> f64 {
        self.trust.get(&ck).copied().unwrap_or(0.0)
    }

    /// Is a quote-context class promoted to a forced class? (Bare classes are
    /// always forced; this only decides the quote fold.)
    fn quote_promoted(&self, mark: char) -> bool {
        self.trust_class(ClassKey { mark, quoted: true }) > PROMOTE_BAR
    }

    fn habit_dominance(&self, key: Option<ClassKey>) -> f64 {
        match self.habit.get(&key) {
            Some(&(up, total)) => wilson_lower_bound(up, total, self.z),
            None => 0.0,
        }
    }

    /// Effective mid-flow pool (upper, lower): mid-flow plus quote-context
    /// classes that did **not** clear the promotion bar (they fall back to
    /// mid-flow, exactly ADR 0051 — no loss).
    fn eff_mid(&self, w: &WordStats) -> (u64, u64) {
        let mut up = u64::from(w.mid_upper);
        let mut lo = u64::from(w.mid_lower);
        for (m, t) in &w.after_quote {
            if !self.quote_promoted(*m) {
                up += t.upper();
                lo += t.lower();
            }
        }
        (up, lo)
    }

    /// Soft-censored effective uppercase count: the effective mid-flow uppercase
    /// plus each *forced* uppercase re-entering at `1 − trust(class)·habit(class)`
    /// (ADR 0052 — the discount is weighted by trust). Book-initial is fully
    /// trusted.
    fn effective_upper(&self, w: &WordStats) -> f64 {
        let (mid_up, _) = self.eff_mid(w);
        let mut up = mid_up as f64;
        if w.book_initial.upper > 0 {
            up += (1.0 - self.habit_dominance(None)) * f64::from(w.book_initial.upper);
        }
        for (m, t) in &w.after_glyph {
            if t.upper > 0 {
                let ck = ClassKey {
                    mark: *m,
                    quoted: false,
                };
                let discount = 1.0 - self.trust_class(ck) * self.habit_dominance(Some(ck));
                up += discount * f64::from(t.upper);
            }
        }
        for (m, t) in &w.after_quote {
            if t.upper > 0 && self.quote_promoted(*m) {
                let ck = ClassKey {
                    mark: *m,
                    quoted: true,
                };
                let discount = 1.0 - self.trust_class(ck) * self.habit_dominance(Some(ck));
                up += discount * f64::from(t.upper);
            }
        }
        up
    }

    /// Forced-lowercase count for the positional minority: book-initial plus
    /// bare glyphs plus *promoted* quote classes (an unpromoted quote's
    /// lowercase folded into the mid-flow pool).
    fn forced_lower(&self, w: &WordStats) -> u64 {
        let mut lo = w.book_initial.lower();
        lo += self.after_glyph_sum(w, ForcedTally::lower);
        for (m, t) in &w.after_quote {
            if self.quote_promoted(*m) {
                lo += t.lower();
            }
        }
        lo
    }

    fn forced_total(&self, w: &WordStats) -> u64 {
        let mut n = w.book_initial.total();
        n += self.after_glyph_sum(w, ForcedTally::total);
        for (m, t) in &w.after_quote {
            if self.quote_promoted(*m) {
                n += t.total();
            }
        }
        n
    }

    fn after_glyph_sum(&self, w: &WordStats, f: fn(&ForcedTally) -> u64) -> u64 {
        w.after_glyph.values().map(f).sum()
    }

    fn is_cap_soft(&self, w: &WordStats) -> bool {
        let up = self.effective_upper(w);
        let (_, lo) = self.eff_mid(w);
        let n = up + lo as f64;
        n > 0.0 && wilson_lower_bound_f(up, n, self.z) > 0.5
    }

    fn is_lower_soft(&self, w: &WordStats) -> bool {
        let up = self.effective_upper(w);
        let (_, lo) = self.eff_mid(w);
        let n = up + lo as f64;
        n > 0.0 && wilson_lower_bound_f(lo as f64, n, self.z) > 0.5
    }

    /// The intrinsic-channel factors for a lowercase site of word `key`, if the
    /// word is intrinsically capitalized (soft-censored). The censoring discount
    /// is trust-weighted; the channel is never gated.
    fn intrinsic(&self, key: &str) -> Option<Factors> {
        let w = self.words.get(key)?;
        if !self.is_cap_soft(w) {
            return None;
        }
        let up = self.effective_upper(w);
        let (_, lo) = self.eff_mid(w);
        Some(Factors {
            dominance: wilson_lower_bound_f(up, up + lo as f64, self.z),
            minority: w.all_lower(),
            opportunities: w.all_total(),
            raw_major: w.all_upper(),
            raw_total: w.all_total(),
        })
    }

    /// The positional-channel factors for a forced-position lowercase site of
    /// word `key` at `pos`, if the word is classifiable AND the site's class
    /// clears the trust gate (ADR 0052). An unpromoted quote-context site has
    /// folded to mid-flow and is not forced ⇒ `None`.
    fn positional(&self, key: &str, pos: PosClass) -> Option<Factors> {
        if !pos.is_forced() {
            return None;
        }
        let w = self.words.get(key)?;
        let (habit_key, trust) = match pos {
            PosClass::BookInitial => (None, 1.0),
            PosClass::ForcedAfterTerminal(ck) => {
                if ck.quoted && !self.quote_promoted(ck.mark) {
                    return None; // folded back to mid-flow — not a forced site
                }
                (Some(ck), self.trust_class(ck))
            }
            PosClass::Midflow => return None,
        };
        // Gate: verdicts gate. Below the trust gate the positional channel is
        // not scored (the discount already weighted the intrinsic channel).
        if trust < self.gate {
            return None;
        }
        // A forced-lowercase site of a word the lexicon cannot classify is
        // genuine ambiguity, not an anomaly.
        if !self.is_cap_soft(w) && !self.is_lower_soft(w) {
            return None;
        }
        let (raw_major, raw_total) = self.habit.get(&habit_key).copied().unwrap_or((0, 0));
        Some(Factors {
            dominance: self.habit_dominance(habit_key),
            minority: self.forced_lower(w),
            opportunities: self.forced_total(w),
            raw_major,
            raw_total,
        })
    }
}

/// A lowercase word-start observed by the book walk — a flag candidate for
/// either rule. Forwarded reduce→judge within a call (ADR 0044), and retained
/// in the content-keyed analysis cache when its owning book is clean.
#[derive(Clone)]
pub struct LowerSite {
    pub(crate) local_idx: LocalKeyIdx,
    pub(crate) start: u32,
    pub(crate) end: u32,
    /// Interned word-type id — an index into the owning [`CasingSites`]'
    /// `keys` table (per-book, first-sight order). A `Copy` id instead of a
    /// `String` so the judge memo hashes a `(u32, PosClass)` instead of
    /// re-hashing/memcmp-ing the folded string per site occurrence.
    pub(crate) key: u32,
    pub(crate) pos: PosClass,
}

/// One book's lowercase sites plus the per-book word-type interner that
/// resolves each site's `key` id back to its case-folded string. It never
/// enters serialized stats; ids remain meaningful only against this book's
/// `keys` table, including when the native product is retained by the
/// content-keyed analysis cache.
#[derive(Clone, Default)]
pub struct CasingSites {
    /// id → folded word-type key, in first-sight order during the book walk.
    pub(crate) keys: Vec<String>,
    pub(crate) sites: Vec<LowerSite>,
}

/// True iff `c` is a cased/uncased letter (GC L*).
fn is_letter(c: char) -> bool {
    class_of(c).is_alphabetic()
}

/// The verse's word units: UAX #29 word tokens, then adjacent tokens joined
/// across a single letter-flanked hyphen merged into one span. Pure-number
/// tokens are dropped. `tokens` is the verse's shared tokenization (the fused
/// walk's product; the standalone driver tokenizes itself). Writes into
/// `out` (clear + refill) instead of allocating a fresh `Vec` per verse — the
/// caller reuses one buffer across a book's verses (ADR 0057 allocation-diet
/// follow-up).
fn compound_words(text: &str, tokens: &[crate::token::Token], out: &mut Vec<Span>) {
    out.clear();
    for t in tokens.iter().copied() {
        if let Some(prev) = out.last_mut() {
            let gap = &text[prev.end as usize..t.span.start as usize];
            let mut g = gap.chars();
            let hyphen = matches!(g.next(), Some('\u{002D}' | '\u{2010}')) && g.next().is_none();
            if hyphen
                && text[..prev.end as usize]
                    .chars()
                    .next_back()
                    .is_some_and(is_letter)
                && text[t.span.start as usize..]
                    .chars()
                    .next()
                    .is_some_and(is_letter)
            {
                prev.end = t.span.end;
                continue;
            }
        }
        out.push(t.span);
    }
    out.retain(|s| {
        text[s.start as usize..s.end as usize]
            .chars()
            .any(is_letter)
    });
}

/// The pending-terminal state across a gap between words. `mark` is the
/// candidate terminal (first punctuation after a letter); `quote` records a
/// close-quote glyph seen after it; `other` records any *non-quote* intervening
/// punctuation, which collapses the boundary to mid-flow (`...`).
///
/// `pub(crate)`: `signals::rare_glyph` reuses this exact pending-terminal
/// machine (ADR 0053) so the forced-position definition lives in one place.
#[derive(Clone, Copy)]
pub(crate) struct Pending {
    mark: char,
    quote: bool,
    other: bool,
}

/// Advance the pending-terminal machine over a gap (all non-word scalars).
pub(crate) fn advance_gap(gap: &str, pending: &mut Option<Pending>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some(p) => {
                    if cl.is_quote() {
                        p.quote = true;
                    } else {
                        p.other = true;
                    }
                }
                None if *prev_letter => {
                    *pending = Some(Pending {
                        mark: c,
                        quote: false,
                        other: false,
                    });
                }
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Resolve a taken pending state to the next word's position class. Non-quote
/// intervening punctuation collapses to mid-flow (ADR 0051); a bare terminal or
/// a terminal-then-close-quote becomes the boundary class (ADR 0052).
pub(crate) fn pos_of(book_initial: bool, taken: Option<Pending>) -> PosClass {
    if book_initial {
        return PosClass::BookInitial;
    }
    match taken {
        Some(p) if p.other => PosClass::Midflow,
        Some(p) if p.quote => PosClass::ForcedAfterTerminal(ClassKey {
            mark: p.mark,
            quoted: true,
        }),
        Some(p) => PosClass::ForcedAfterTerminal(ClassKey {
            mark: p.mark,
            quoted: false,
        }),
        None => PosClass::Midflow,
    }
}

/// The casing counting listener: one book's raw per-word table plus the
/// lowercase flag candidates, fed per verse by the fused walk. The pending
/// terminal is carried across verse seams; the book-initial word is forced.
pub(crate) struct CasingAcc {
    /// Cased word-start observations (the emergent-gate input).
    cased_starts: u32,
    /// Per-book word-type interner: folded key → id, and id → key. The walk
    /// tallies into the id-indexed `word_stats` (one hash probe per word)
    /// instead of a `BTreeMap<String, _>` entry walk (log n string memcmps
    /// per word) — the stats' pinned sorted shape is rebuilt once in
    /// `finish`.
    intern: FxHashMap<String, u32>,
    keys: Vec<String>,
    word_stats: Vec<WordStats>,
    sites: Vec<LowerSite>,
    pending: Option<Pending>,
    book_initial: bool,
    /// Reusable `compound_words` scratch buffer (ADR 0057 allocation-diet
    /// follow-up) — cleared and refilled each verse instead of allocating.
    words_buf: Vec<Span>,
}

impl CasingAcc {
    pub(crate) fn new() -> Self {
        CasingAcc {
            cased_starts: 0,
            intern: FxHashMap::default(),
            keys: Vec::new(),
            word_stats: Vec::new(),
            sites: Vec::new(),
            pending: None,
            book_initial: true,
            words_buf: Vec::new(),
        }
    }

    pub(crate) fn verse(&mut self, v: &stream::VerseInputs<'_, '_>) {
        let text = v.text;
        compound_words(text, v.tokens, &mut self.words_buf);
        let mut prev_letter = false;
        let mut cursor = 0usize;

        // Indexed (not `for w in &self.words_buf`): `w` is a `Copy` value
        // read out per iteration, so the loop body is free to mutate other
        // `self` fields without holding a borrow of `words_buf` open.
        for i in 0..self.words_buf.len() {
            let w = self.words_buf[i];
            advance_gap(
                &text[cursor..w.start as usize],
                &mut self.pending,
                &mut prev_letter,
            );

            let first = text[w.start as usize..w.end as usize]
                .chars()
                .next()
                .unwrap();
            let fcl = class_of(first);
            let case = if fcl.is_uppercase() {
                Case::Upper
            } else if fcl.is_lowercase() {
                Case::Lower
            } else {
                Case::Uncased
            };
            let pos = pos_of(self.book_initial, self.pending.take());
            self.book_initial = false;

            if case != Case::Uncased {
                self.cased_starts += 1;
            }
            // The fold is deliberately the exact `str::to_lowercase` of the
            // compound-word span (context-sensitive: final sigma etc.), same
            // as it always was — no fast-path gate, so no drift.
            let key = text[w.start as usize..w.end as usize].to_lowercase();
            let id = match self.intern.get(&key) {
                Some(&id) => id,
                None => {
                    let id = self.keys.len() as u32;
                    self.intern.insert(key.clone(), id);
                    self.keys.push(key);
                    self.word_stats.push(WordStats::default());
                    id
                }
            };
            self.word_stats[id as usize].record(pos, case);
            // Boundary predicate (ADR 0055): an OtherMixed token (`asÍ`,
            // `word-wOrd`) is `case.mixed-case-word`'s to report — its interior
            // capital is the defect, not its incidental lowercase initial. Skip
            // the lowercase site so the phenomenon is reported once, not twice.
            // The word still tallied above, so it keeps contributing to the
            // lexicon/habit; only the flag candidate is suppressed. (An
            // OtherMixed word whose first letter is uppercase never reaches here
            // — `case` would be `Upper` — so this only touches the first-lower
            // overlap class.)
            if case == Case::Lower
                && case_shape(&text[w.start as usize..w.end as usize])
                    != Some(CaseShape::OtherMixed)
            {
                self.sites.push(LowerSite {
                    local_idx: v.local_idx,
                    start: w.start,
                    end: w.end,
                    key: id,
                    pos,
                });
            }

            prev_letter = text[w.start as usize..w.end as usize]
                .chars()
                .next_back()
                .is_some_and(is_letter);
            cursor = w.end as usize;
        }
        advance_gap(&text[cursor..], &mut self.pending, &mut prev_letter);
    }

    pub(crate) fn finish(self) -> (BookCasing, CasingSites) {
        // Rebuild the stats' pinned sorted word table (dropping caseless
        // words, exactly the old `retain`); the interner strings live on in
        // the sites half so ids stay resolvable at judge.
        let words: BTreeMap<String, WordStats> = self
            .keys
            .iter()
            .zip(&self.word_stats)
            .filter(|(_, w)| w.has_case())
            .map(|(k, w)| (k.clone(), w.clone()))
            .collect();
        (
            BookCasing {
                words,
                cased_starts: self.cased_starts,
            },
            CasingSites {
                keys: self.keys,
                sites: self.sites,
            },
        )
    }
}

/// Scan one book's verses — the standalone driver over [`CasingAcc`], used by
/// the judge's re-scan path (a prior-carried book has no forwarded sites) and
/// the calibration API.
fn walk_book(group: &BookGroup<'_>) -> (BookCasing, CasingSites) {
    stream::drive_book(
        group,
        stream::Needs {
            tokens: true,
            ..Default::default()
        },
        CasingAcc::new(),
        |a, v| a.verse(v),
        CasingAcc::finish,
    )
}

/// Shared reduce for both casing rules: walk each book once.
fn reduce_casing(books: &Books<'_>) -> (CasingStats, BTreeMap<Box<str>, CasingSites>) {
    let mut per_book = BTreeMap::new();
    let mut sites = BTreeMap::new();
    for (group, (bc, book_sites)) in books.iter().zip(rule::map_books(books, walk_book)) {
        per_book.insert(Box::from(group.slug), bc);
        sites.insert(Box::from(group.slug), book_sites);
    }
    (CasingStats { per_book }, sites)
}

/// True iff the merged corpus has any cased word-start — the emergent gate.
fn any_cased(stats: &CasingStats) -> bool {
    stats.per_book.values().any(|b| b.cased_starts > 0)
}

/// Shared judge skeleton: build the corpus model, recover each book's lowercase
/// sites, and turn each into at most one finding via a memoized two-step
/// evaluation. `verdict` is the expensive Wilson-bound math — a pure function
/// of `(key, pos)`, never the individual occurrence — so it is computed once
/// per distinct `(key, pos)` seen in a book and cached in a per-book memo.
/// `materialize` is the cheap per-site step that turns a cached verdict into
/// a `Finding` at the site's own span.
fn judge_casing<V: Clone + Sync + Send>(
    stats: &RuleStats,
    books: &Books<'_>,
    sites: Option<&rule::RuleSites>,
    cfg: &CasingConfig,
    verdict: impl Fn(&str, PosClass, &Model) -> Option<V> + Sync,
    materialize: impl Fn(&LowerSite, &str, &V, crate::corpus::KeyIdx) -> Finding + Sync,
) -> Vec<Finding> {
    let RuleStats::Casing(stats) = stats else {
        return Vec::new();
    };
    // Emergent gate: no cased word-starts, no convention to violate.
    if !any_cased(stats) {
        return Vec::new();
    }
    let model = Model::build(stats, cfg);

    let forwarded = match sites {
        Some(rule::RuleSites::Casing(m)) => Some(m),
        _ => None,
    };
    // Per-site loop over one book's sites: the memo hashes the Copy
    // `(id, PosClass)` pair — the folded string is resolved through the
    // book's interner only on a memo miss (once per distinct pair).
    let emit = |base: crate::corpus::KeyIdx, book_sites: &CasingSites, found: &mut Vec<Finding>| {
        let keys = &book_sites.keys;
        let mut memo: FxHashMap<(u32, PosClass), Option<V>> = FxHashMap::default();
        for site in &book_sites.sites {
            let v = memo
                .entry((site.key, site.pos))
                .or_insert_with(|| verdict(&keys[site.key as usize], site.pos, &model));
            if let Some(v) = v {
                found.push(materialize(
                    site,
                    &keys[site.key as usize],
                    v,
                    rebase(base, site.local_idx),
                ));
            }
        }
    };
    let mut out: Vec<Finding> = rule::map_books(books, |group| {
        let mut found = Vec::new();
        if let Some(book_sites) = forwarded.and_then(|m| m.get(group.slug)) {
            emit(group.base, book_sites, &mut found);
        } else {
            let (_, walked) = walk_book(group);
            emit(group.base, &walked, &mut found);
        }
        found
    })
    .into_iter()
    .flatten()
    .collect();
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

fn site_span(site: &LowerSite) -> Span {
    Span {
        start: site.start,
        end: site.end,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// case.sentence-initial-lowercase — the positional rule.
// ─────────────────────────────────────────────────────────────────────────

pub struct SentenceInitialLowercase {
    pub cfg: CasingConfig,
}

impl StatefulRule for SentenceInitialLowercase {
    fn id(&self) -> RuleId {
        SENTENCE_INITIAL_LOWERCASE
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        let (stats, sites) = reduce_casing(books);
        (RuleStats::Casing(stats), rule::RuleSites::Casing(sites))
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let k = clamp_count(self.cfg.recurrence_k);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        judge_casing(
            stats,
            books,
            sites,
            &self.cfg,
            |key, pos, model| {
                let f = model.positional(key, pos)?;
                let score = f.dominance * rarity(f.minority, k);
                if score < floor {
                    return None;
                }
                let (glyph, quoted) = pos.habit_glyph();
                Some((
                    score,
                    glyph,
                    quoted,
                    f.raw_major.min(u64::from(u32::MAX)) as u32,
                    f.raw_total.min(u64::from(u32::MAX)) as u32,
                ))
            },
            |site, _key, &(score, glyph, quoted, upper, total), key_idx| Finding {
                key_idx,
                code: SENTENCE_INITIAL_LOWERCASE,
                severity: Severity::Info,
                range: site_span(site),
                score: Some(score as f32),
                args: Some(FindingArgs::CasingConvention {
                    glyph,
                    quoted,
                    upper,
                    total,
                }),
            },
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// case.inconsistent-word-casing — the intrinsic rule.
// ─────────────────────────────────────────────────────────────────────────

pub struct InconsistentWordCasing {
    pub cfg: CasingConfig,
}

impl StatefulRule for InconsistentWordCasing {
    fn id(&self) -> RuleId {
        INCONSISTENT_WORD_CASING
    }

    fn reduce(
        &self,
        books: &Books<'_>,
        _source: Option<&Corpus>,
        _tokens: Option<&TokenCache>,
    ) -> (RuleStats, rule::RuleSites) {
        let (stats, sites) = reduce_casing(books);
        (RuleStats::Casing(stats), rule::RuleSites::Casing(sites))
    }

    fn judge(
        &self,
        stats: &RuleStats,
        books: &Books<'_>,
        _tokens: Option<&TokenCache>,
        sites: Option<&rule::RuleSites>,
    ) -> Vec<Finding> {
        let k = clamp_count(self.cfg.recurrence_k);
        let floor = f64::from(clamp_unit(self.cfg.emit_score_min));
        judge_casing(
            stats,
            books,
            sites,
            &self.cfg,
            |key, _pos, model| {
                let f = model.intrinsic(key)?;
                let score = f.dominance * rarity(f.minority, k);
                if score < floor {
                    return None;
                }
                Some((
                    score,
                    f.raw_major.min(u64::from(u32::MAX)) as u32,
                    f.raw_total.min(u64::from(u32::MAX)) as u32,
                ))
            },
            |site, key, &(score, upper, total), key_idx| Finding {
                key_idx,
                code: INCONSISTENT_WORD_CASING,
                severity: Severity::Info,
                range: site_span(site),
                score: Some(score as f32),
                args: Some(FindingArgs::WordCasing {
                    word: key.to_owned(),
                    upper,
                    total,
                }),
            },
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Calibration API (ADR 0051/0052). The `--casing` harness in calibrate.rs
// consumes this to sweep floor/k and track review anchors over the real walk,
// model, trust map, and gate; it is not used by the shipped rules' judge.
// ─────────────────────────────────────────────────────────────────────────

/// One lowercase site evaluated against the corpus model: its position and the
/// two channels' factors (either may be absent). The positional channel already
/// reflects the trust gate (`None` when gated or folded to mid-flow); the
/// intrinsic channel already reflects the trust-weighted censoring discount.
pub struct SiteEval {
    pub key_idx: crate::corpus::KeyIdx,
    pub start: u32,
    pub end: u32,
    pub pos: PosClass,
    pub intrinsic: Option<Factors>,
    pub positional: Option<Factors>,
}

/// Build the corpus model and classify every lowercase site at the config's
/// knobs (including `trust_gate`) — the calibration entry point.
pub fn evaluate(books: &Books<'_>, cfg: &CasingConfig) -> Vec<SiteEval> {
    let (stats, sites_map) = reduce_casing(books);
    let model = Model::build(&stats, cfg);
    let mut out = Vec::new();
    for (slug, book_sites) in &sites_map {
        let group = books
            .iter()
            .find(|g| g.slug == slug.as_ref())
            .expect("sites keyed by a book in this corpus");
        let keys = &book_sites.keys;
        for site in &book_sites.sites {
            out.push(SiteEval {
                key_idx: rebase(group.base, site.local_idx),
                start: site.start,
                end: site.end,
                pos: site.pos,
                intrinsic: model.intrinsic(&keys[site.key as usize]),
                positional: model.positional(&keys[site.key as usize], site.pos),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;

    /// Full config with an explicit trust gate (ADR 0052).
    fn cfg_g(
        emit_score_min: f32,
        recurrence_k: f32,
        confidence_z: f32,
        trust_gate: f32,
    ) -> CasingConfig {
        CasingConfig {
            emit_score_min,
            recurrence_k,
            confidence_z,
            trust_gate,
        }
    }

    /// Config at the default trust gate (0.90) — most ADR 0051 tests.
    fn cfg(emit_score_min: f32, recurrence_k: f32, confidence_z: f32) -> CasingConfig {
        cfg_g(emit_score_min, recurrence_k, confidence_z, 0.90)
    }

    /// The wire-format key for one book/verse (chapter is always 1 — these
    /// tests never need a second chapter).
    fn key_at(book: &str, v: u16) -> String {
        format!("{book} 1:{v}")
    }

    fn book(book: &str, verses: &[(u16, &str)]) -> Corpus {
        let keys = verses.iter().map(|&(v, _)| key_at(book, v)).collect();
        let texts = verses.iter().map(|&(_, t)| t.to_string()).collect();
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn run(corpus: &Corpus, r: &dyn StatefulRule) -> Vec<Finding> {
        let books = corpus::by_book(corpus);
        let (stats, sites) = r.reduce(&books, None, None);
        r.judge(&stats, &books, None, Some(&sites))
    }

    fn intrinsic(cfg: CasingConfig) -> InconsistentWordCasing {
        InconsistentWordCasing { cfg }
    }
    fn positional(cfg: CasingConfig) -> SentenceInitialLowercase {
        SentenceInitialLowercase { cfg }
    }

    fn slice<'a>(corpus: &'a Corpus, f: &Finding) -> &'a str {
        &corpus.text(f.key_idx)[f.range.start as usize..f.range.end as usize]
    }

    /// True iff `f` addresses `book 1:v` — the resolved-key analogue of the
    /// old direct `f.sid == sid(book, v)` comparison.
    fn at(corpus: &Corpus, f: &Finding, book: &str, v: u16) -> bool {
        corpus.key(f.key_idx) == key_at(book, v)
    }

    /// Build a corpus by cycling `templates`, one verse each, `reps` cycles.
    fn cycle(book_code: &str, templates: &[&str], reps: u16) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..reps {
            for t in templates {
                keys.push(key_at(book_code, v));
                texts.push((*t).to_string());
                v += 1;
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Append one more verse to an existing corpus. `Corpus` is an immutable,
    /// validated structure-of-arrays (no in-place insert), so "inserting" is
    /// rebuilding with the extra entry appended — the functional analogue of
    /// the old `VerseMap::insert`. Appending (rather than splicing to a
    /// numeric position) is what the tests need: book-local walk order is the
    /// corpus's *presented* order, so an appended verse always lands after
    /// every verse already in its book's block.
    fn push_verse(corpus: Corpus, book: &str, v: u16, text: &str) -> Corpus {
        let mut keys = corpus.keys().to_vec();
        let mut texts = corpus.texts().to_vec();
        keys.push(key_at(book, v));
        texts.push(text.to_string());
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// Append another corpus's entries — the functional analogue of the old
    /// `VerseMap::extend`.
    fn extend_corpus(corpus: Corpus, other: Corpus) -> Corpus {
        let mut keys = corpus.keys().to_vec();
        let mut texts = corpus.texts().to_vec();
        keys.extend(other.keys().iter().cloned());
        texts.extend(other.texts().iter().cloned());
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// The corpus-wide trust for one class (test introspection over the model).
    fn class_trust(corpus: &Corpus, mark: char, quoted: bool) -> f64 {
        let books = corpus::by_book(corpus);
        let (stats, _) = reduce_casing(&books);
        let model = Model::build(&stats, &CasingConfig::default());
        model.trust_class(ClassKey { mark, quoted })
    }

    // ── ADR 0051 behaviours (schema-updated only where forced). ──────────────

    /// INTRINSIC fires on a lowercased capital word; positional stays silent.
    #[test]
    fn intrinsic_flags_a_lowercased_capital_word() {
        let vm = cycle("GEN", &["we saw Jesus"], 20);
        let vm = push_verse(vm, "GEN", 100, "we saw jesus");
        let f = run(&vm, &intrinsic(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&vm, &f[0]), "jesus");
        match &f[0].args {
            Some(FindingArgs::WordCasing { word, upper, total }) => {
                assert_eq!(word, "jesus");
                assert_eq!((*upper, *total), (20, 21));
            }
            other => panic!("expected WordCasing, got {other:?}"),
        }
        assert!(run(&vm, &positional(cfg(0.5, 32.0, 0.0))).is_empty());
    }

    /// POSITIONAL fires after a strong terminal, and the args carry the class.
    #[test]
    fn positional_flags_lowercase_after_a_strong_terminal() {
        let vm = cycle("GEN", &["The men saw the gate."], 40);
        let vm = push_verse(vm, "GEN", 100, "He fell. the men ran.");
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        let hit: Vec<_> = f.iter().filter(|f| slice(&vm, f) == "the").collect();
        assert_eq!(hit.len(), 1, "{f:?}");
        match &hit[0].args {
            Some(FindingArgs::CasingConvention {
                glyph,
                quoted,
                upper,
                total,
            }) => {
                assert_eq!(*glyph, Some('.'));
                assert!(!*quoted, "a bare terminal is not a quote-context class");
                assert!(*upper > 0 && *upper <= *total);
            }
            other => panic!("expected CasingConvention, got {other:?}"),
        }
    }

    #[test]
    fn recurrence_silences_a_recurring_minority() {
        let one = {
            let vm = cycle("GEN", &["we saw Jesus"], 100);
            push_verse(vm, "GEN", 200, "we saw jesus")
        };
        let many = {
            let mut vm = cycle("GEN", &["we saw Jesus"], 100);
            for i in 0..40 {
                vm = push_verse(vm, "GEN", 200 + i, "we saw jesus");
            }
            vm
        };
        let r = intrinsic(cfg(0.5, 32.0, 0.0));
        assert_eq!(run(&one, &r).len(), 1);
        assert!(run(&many, &r).is_empty());

        let p_one = {
            let vm = cycle("GEN", &["The men saw the gate."], 100);
            push_verse(vm, "GEN", 300, "He fell. the men ran.")
        };
        let p_many = {
            let mut vm = cycle("GEN", &["The men saw the gate."], 100);
            for i in 0..40 {
                vm = push_verse(vm, "GEN", 300 + i, "He fell. the men ran.");
            }
            vm
        };
        let pr = positional(cfg(0.5, 32.0, 0.0));
        assert!(run(&p_one, &pr).iter().any(|f| slice(&p_one, f) == "the"));
        assert!(!run(&p_many, &pr).iter().any(|f| slice(&p_many, f) == "the"));
    }

    #[test]
    fn caseless_script_is_silent() {
        let vm = book("GEN", &[(1, "उसने कहा। वे चले गए।"), (2, "फिर वह चला गया।")]);
        assert!(run(&vm, &intrinsic(cfg(0.0, 32.0, 0.0))).is_empty());
        assert!(run(&vm, &positional(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    #[test]
    fn positional_carries_across_a_verse_seam() {
        let vm = cycle("GEN", &["There we go there.", "There it is there."], 30);
        let vm = push_verse(vm, "GEN", 200, "he stops.");
        let vm = push_verse(vm, "GEN", 201, "there he goes");
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(
            f.iter()
                .any(|f| at(&vm, f, "GEN", 201) && slice(&vm, f) == "there")
        );
    }

    #[test]
    fn verse_initial_without_a_terminal_is_not_forced() {
        let vm = cycle("GEN", &["There we go there.", "There it is there."], 30);
        let vm = push_verse(vm, "GEN", 200, "he walks");
        let vm = push_verse(vm, "GEN", 201, "there he goes");
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(!f.iter().any(|f| at(&vm, f, "GEN", 201)));
    }

    #[test]
    fn hyphen_compound_is_one_word() {
        let compound = cycle("GEN", &["we saw Jesus"], 20);
        let compound = push_verse(compound, "GEN", 100, "he met Bar-jesus");
        assert!(run(&compound, &intrinsic(cfg(0.5, 32.0, 0.0))).is_empty());

        let bare = cycle("GEN", &["we saw Jesus"], 20);
        let bare = push_verse(bare, "GEN", 100, "he met jesus");
        assert_eq!(run(&bare, &intrinsic(cfg(0.5, 32.0, 0.0))).len(), 1);
    }

    #[test]
    fn both_quadrant_fires_both_rules() {
        let vm = cycle("GEN", &["The men praise God near the gate."], 40);
        let vm = push_verse(vm, "GEN", 100, "He wept. god is near.");
        let fi = run(&vm, &intrinsic(cfg(0.5, 32.0, 0.0)));
        let fp = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(
            fi.iter()
                .any(|f| at(&vm, f, "GEN", 100) && slice(&vm, f) == "god")
        );
        assert!(
            fp.iter()
                .any(|f| at(&vm, f, "GEN", 100) && slice(&vm, f) == "god")
        );
    }

    #[test]
    fn book_supersede_via_merge_and_remove() {
        let r = intrinsic(cfg(0.5, 32.0, 0.0));
        let dirty = cycle("GEN", &["we saw Jesus"], 20);
        let dirty = push_verse(dirty, "GEN", 100, "we saw jesus");
        let dirty_books = corpus::by_book(&dirty);
        let (prior, _) = r.reduce(&dirty_books, None, None);
        assert_eq!(r.judge(&prior, &dirty_books, None, None).len(), 1);

        let fixed = cycle("GEN", &["we saw Jesus"], 20);
        let fixed = push_verse(fixed, "GEN", 100, "we saw Jesus");
        let fixed_books = corpus::by_book(&fixed);
        let (fresh, _) = r.reduce(&fixed_books, None, None);
        let merged = prior.merge(fresh);
        assert!(r.judge(&merged, &fixed_books, None, None).is_empty());

        let two = cycle("GEN", &["we saw Jesus"], 20);
        let two = extend_corpus(two, book("EXO", &[(1, "we saw jesus")]));
        let (mut stats2, _) = r.reduce(&corpus::by_book(&two), None, None);
        assert_eq!(
            r.judge(&stats2, &corpus::by_book(&two), None, None).len(),
            1
        );
        let RuleStats::Casing(ref mut c) = stats2 else {
            unreachable!()
        };
        c.remove_book("GEN");
        let exo = book("EXO", &[(1, "we saw jesus")]);
        assert!(
            r.judge(&stats2, &corpus::by_book(&exo), None, None)
                .is_empty()
        );
    }

    #[test]
    fn floor_and_knee_config_are_respected() {
        let vm = cycle("GEN", &["we saw Jesus"], 100);
        let vm = push_verse(vm, "GEN", 200, "we saw jesus");
        assert_eq!(run(&vm, &intrinsic(cfg(0.95, 32.0, 0.0))).len(), 1);
        assert!(run(&vm, &intrinsic(cfg(0.999, 32.0, 0.0))).is_empty());

        let two = cycle("GEN", &["we saw Jesus"], 100);
        let two = push_verse(two, "GEN", 200, "we saw jesus");
        let two = push_verse(two, "GEN", 201, "we saw jesus");
        assert_eq!(run(&two, &intrinsic(cfg(0.5, 32.0, 0.0))).len(), 2);
        assert!(run(&two, &intrinsic(cfg(0.5, 1.0, 0.0))).is_empty());
    }

    // ── ADR 0052 behaviours. ─────────────────────────────────────────────────

    /// A quote-context (`."`) class earns trust where the corpus reliably
    /// capitalizes a lexicon-lowercase word after a quoted sentence, and a
    /// lowercase slip there flags positionally with the quoted class in its args.
    #[test]
    fn quote_context_class_earns_trust_and_flags() {
        // Every verse opens `The` (forced across the seam and after `."`), and
        // `the` recurs mid-flow several times a verse so it stays lexicon-lower
        // even with the quote-opening `The` folded into its baseline profile.
        // `.` and `."` share the `The` aftermath ⇒ high agreement, high trust.
        let vm = cycle(
            "GEN",
            &["The voice spoke to the man.\" The people saw the gate by the sea."],
            60,
        );
        // One slip: lowercase `the` right after a `."` boundary.
        let vm = push_verse(
            vm,
            "GEN",
            500,
            "He wept.\" the men saw the gate by the sea.",
        );

        assert!(
            class_trust(&vm, '.', true) >= 0.90,
            "`.\"` trust {} should clear the gate",
            class_trust(&vm, '.', true)
        );
        let f = run(&vm, &positional(cfg(0.5, 32.0, 1.96)));
        let hit: Vec<_> = f
            .iter()
            .filter(|f| at(&vm, f, "GEN", 500) && slice(&vm, f) == "the")
            .collect();
        assert_eq!(hit.len(), 1, "the post-quote slip flags: {f:?}");
        match &hit[0].args {
            Some(FindingArgs::CasingConvention { glyph, quoted, .. }) => {
                assert_eq!(*glyph, Some('.'));
                assert!(*quoted, "the flagged class is the quote-context `.\"`");
            }
            other => panic!("expected CasingConvention, got {other:?}"),
        }
    }

    /// A genealogy-style list comma: it capitalizes a lexicon-lowercase word
    /// often enough to build a moderate habit, but its aftermath is its own list
    /// vocabulary (never a terminal's), so the agreement guard denies it trust.
    /// The would-be positional site is gated to silence — yet the capitalized
    /// names after the comma still count as mid-flow lexicon evidence, so a
    /// lowercase slip of a name surfaces intrinsically.
    #[test]
    fn untrusted_comma_gates_positional_but_keeps_lexicon_evidence() {
        // `.` is the reference terminal (opens `The`, `the` recurs mid). The
        // comma is followed by list names (`Enosh`, `Kenan`) never seen after a
        // period, plus `The`/`the` in a ~2:1 mix (moderate case-witness).
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        let mut v = 1u16;
        for _ in 0..40 {
            // two verses give the comma an upper `The` after it …
            keys.push(key_at("GEN", v));
            texts.push("Enosh, Kenan, The men saw the gate.".to_string());
            v += 1;
            // … and one gives it a lowercase `the`, holding the habit under 1.0.
            keys.push(key_at("GEN", v));
            texts.push("Enosh, Kenan, the men saw the gate.".to_string());
            v += 1;
        }
        let vm = Corpus::try_from_parts(keys, texts).unwrap();
        // A lowercase slip of the name `enosh` mid-flow (its only lowercase).
        let vm = push_verse(vm, "GEN", 900, "we saw enosh today.");

        // The comma is distrusted (agreement guard) — below the gate.
        assert!(
            class_trust(&vm, ',', false) < 0.90,
            "list-comma trust {} must be gated",
            class_trust(&vm, ',', false)
        );
        // Positional: no `the`-after-comma finding (gated), even though the comma
        // capitalizes `The` most of the time.
        let fp = run(&vm, &positional(cfg(0.5, 32.0, 1.96)));
        assert!(
            !fp.iter().any(|f| {
                matches!(
                    &f.args,
                    Some(FindingArgs::CasingConvention {
                        glyph: Some(','),
                        ..
                    })
                )
            }),
            "the distrusted comma flags nothing positionally: {fp:?}"
        );
        // Intrinsic: `Enosh` is capitalized only after the (untrusted) comma, so
        // those capitals fold to mid-flow evidence — the lone lowercase `enosh`
        // surfaces as inconsistent casing. (Under a *trusted* mark those capitals
        // would be censored and this would stay silent.)
        let fi = run(&vm, &intrinsic(cfg(0.5, 32.0, 1.96)));
        assert!(
            fi.iter()
                .any(|f| at(&vm, f, "GEN", 900) && slice(&vm, f) == "enosh"),
            "the name's post-comma capitals remain lexicon evidence: {fi:?}"
        );
    }

    /// The `trust_gate` knob is respected: a moderate-trust class surfaces at a
    /// low gate and vanishes when the gate is raised above its trust. The comma
    /// here capitalizes the lexicon-lowercase `the` a *handful* of times (Wilson
    /// shrinks that small sample to a moderate habit ≈ its own trust), padded to
    /// the event floor by list names, with list-like aftermath ⇒ low agreement.
    #[test]
    fn trust_gate_knob_is_respected() {
        // `.` reference: opens `The`, `the` recurs mid ⇒ strongly case-trusted.
        let mut vm = cycle("GEN", &["Enosh, Kenan lived. The people saw the gate."], 40);
        // Six comma→`The` (upper) events: a small lexicon-lower sample ⇒ a
        // Wilson-shrunk, moderate comma habit/trust.
        for i in 0..6u16 {
            vm = push_verse(vm, "GEN", 800 + i, "We met, The men there.");
        }
        // One comma→`the` slip: the flag candidate (forced-lowercase count 1).
        let vm = push_verse(vm, "GEN", 900, "We met, the men there.");
        let t = class_trust(&vm, ',', false);
        assert!(
            (0.10..0.90).contains(&t),
            "comma trust {t} must be moderate for the gate to bite either way"
        );

        let hits = |gate: f32| {
            run(&vm, &positional(cfg_g(0.4, 32.0, 1.96, gate)))
                .iter()
                .filter(|f| at(&vm, f, "GEN", 900) && slice(&vm, f) == "the")
                .count()
        };
        assert_eq!(hits(0.10), 1, "below the comma's trust the site surfaces");
        assert_eq!(
            hits(0.95),
            0,
            "raising the gate above its trust silences it"
        );
    }

    /// Caseless corpora self-silence, so the reshuffle witness cannot corrupt a
    /// casing verdict even when it cannot identify the terminal.
    #[test]
    fn caseless_corpus_stays_silent_regardless_of_trust() {
        let keys: Vec<String> = (1..=60u16).map(|v| key_at("GEN", v)).collect();
        let texts = vec![
            "उसने कहा। वे चले गए। फिर वह चला गया।".to_string();
            keys.len()
        ];
        let vm = Corpus::try_from_parts(keys, texts).unwrap();
        assert!(run(&vm, &positional(cfg(0.0, 32.0, 1.96))).is_empty());
        assert!(run(&vm, &intrinsic(cfg(0.0, 32.0, 1.96))).is_empty());
    }
}
