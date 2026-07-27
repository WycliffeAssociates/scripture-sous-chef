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
//! ## Stats shape (raw, per book)
//!
//! Per book, the substrate's corpus aggregate stores a word→[`WordStats`] table
//! of **raw** tallies (mid-flow upper/lower; forced upper/lower split by the
//! bare terminal glyph, by the quote-context glyph, and book-initial
//! separately). Nothing is censored and no trust is computed at reduce: the
//! lexicon classification, the per-class habit, and the two witnesses are
//! corpus-wide, so they are all **judge-time** arithmetic over the merged
//! table. The W2 aggregates the ADR calls for — per-class following-word counts
//! and the baseline word-start distribution — are *reindexed at judge from
//! these same per-word tallies* (the reshuffle witness is case-free, so a
//! word's forced upper+lower is its occurrence count after that class); no
//! second stored table, no size cost beyond the quote-context split. A book
//! carries its own counts and is replaced wholesale on edit.
//!
//! **Pruning.** As ADR 0051: the sole per-book-safe drop is an *uncased-only*
//! word (a caseless-script token) — it yields no candidate site and never enters
//! the lexicon-lowercase habit or the (bicameral) witnesses. Every cased word is
//! kept with raw tallies.

use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::analysis::association::Table2;
use crate::charclass::class_of;
use crate::config::CasingConfig;
use crate::corpus::{Corpus, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::interner::{WordInterner, WordSym};
use crate::signals::case_shape::{CaseShape, case_shape};
use crate::span::Span;

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
/// punctuation (`...`) — is [`PosKind::Midflow`]. Verse-initial is NOT forced
/// (`CLAUDE.md`).
///
/// **One deterministic `u32`, not a tagged enum.** This is a *retained* field:
/// it is the one bit of a [`LowerSite`] that genuinely needs discourse context
/// to recompute (plan §11's retain-vs-rederive principle), so it is stored on
/// every one of an English Bible's ~668k lowercase sites. As
/// `enum { BookInitial, ForcedAfterTerminal(ClassKey { char, bool }), Midflow }`
/// it cost 8 bytes — a 4-byte scalar/tag word plus a `bool` plus 3 bytes of
/// padding; packed it costs 4, which is what lets a site record close at 12
/// bytes instead of 16.
///
/// The encoding is a direct injection of the **complete** accepted domain, with
/// no table, no interner and no lifecycle — deliberately, because an id assigned
/// out of a side table would need a bound on how many distinct boundary classes
/// a long-lived resident engine can ever see, and no such bound is measurable
/// from a corpus fleet (that error class is what the 2026-07-26 review caught in
/// the earlier mark-table design, and the same one WP7a's `ord: u8` stop clause
/// hit). A Unicode scalar is 21 bits; `quoted` is one more; the two structural
/// classes take explicit sentinel words above every representable scalar:
///
/// ```text
///  bits  0..=20   Unicode scalar value of the terminal mark (0..=0x10FFFF)
///  bit      21    quoted (a close-quote intervened before the next word)
///  0xFFFF_FFFF    Midflow      (sentinel; no forced encoding can reach it)
///  0xFFFF_FFFE    BookInitial  (sentinel)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PosClass(u32);

/// Ordered by the *semantic* three-way class — `BookInitial` < a forced class
/// (by `(mark, quoted)`) < `Midflow` — which is what the equivalent tagged enum
/// derived before the packing. `Ord` exists only to satisfy the substrate
/// contract's `Key: Ord` bound, and casing's stats-delta is always empty so no
/// shipped order depends on it today; it is written out rather than derived so
/// that if a key list ever *is* sorted, packing the field cannot silently have
/// changed the order.
impl Ord for PosClass {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(p: PosClass) -> (u8, Option<(char, bool)>) {
            match p.kind() {
                PosKind::BookInitial => (0, None),
                PosKind::ForcedAfterTerminal(ck) => (1, Some((ck.mark, ck.quoted))),
                PosKind::Midflow => (2, None),
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

impl PartialOrd for PosClass {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The decoded shape of a [`PosClass`] — the three-way case callers match on.
/// Exhaustive, so the compiler still forces every consumer to handle all three;
/// only the *storage* is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosKind {
    /// The first word of the book — forced with no terminal glyph.
    BookInitial,
    /// A word whose first letter consumed an attached terminal (carried across
    /// verse seams). The [`ClassKey`] is the positional habit / trust key.
    ForcedAfterTerminal(ClassKey),
    /// Not position-forced: uppercase here is intrinsic to the word.
    Midflow,
}

impl PosClass {
    /// Bit 21 — set when a close-quote intervened between the terminal and the
    /// next word (ADR 0052's distinct boundary class).
    const QUOTED_BIT: u32 = 1 << 21;
    /// Sentinels, chosen above `QUOTED_BIT | 0x10FFFF` so no forced class can
    /// ever encode to either.
    const MIDFLOW_BITS: u32 = u32::MAX;
    const BOOK_INITIAL_BITS: u32 = u32::MAX - 1;

    /// Not position-forced.
    pub const MIDFLOW: Self = Self(Self::MIDFLOW_BITS);
    /// The book's first word.
    pub const BOOK_INITIAL: Self = Self(Self::BOOK_INITIAL_BITS);

    /// The forced class after `mark`, with or without an intervening
    /// close-quote. Total over the accepted domain: every `char` is a valid
    /// Unicode scalar, so every `(mark, quoted)` pair encodes.
    pub fn forced(ck: ClassKey) -> Self {
        let mut bits = ck.mark as u32;
        if ck.quoted {
            bits |= Self::QUOTED_BIT;
        }
        Self(bits)
    }

    /// Decode. The `char` conversion cannot fail: bits 0..=20 are only ever
    /// written from a `char` in [`PosClass::forced`], and neither sentinel is
    /// reachable by that path.
    pub fn kind(self) -> PosKind {
        match self.0 {
            Self::MIDFLOW_BITS => PosKind::Midflow,
            Self::BOOK_INITIAL_BITS => PosKind::BookInitial,
            bits => PosKind::ForcedAfterTerminal(ClassKey {
                mark: char::from_u32(bits & !Self::QUOTED_BIT)
                    .expect("PosClass forced bits always hold a Unicode scalar"),
                quoted: bits & Self::QUOTED_BIT != 0,
            }),
        }
    }

    pub(crate) fn is_forced(self) -> bool {
        self.0 != Self::MIDFLOW_BITS
    }

    /// Descriptive `(glyph, quoted)` for the finding args (ADR 0048/0052).
    fn habit_glyph(self) -> (Option<char>, bool) {
        match self.kind() {
            PosKind::ForcedAfterTerminal(ck) => (Some(ck.mark), ck.quoted),
            _ => (None, false),
        }
    }
}

/// Forced-position first-letter tallies after one key. Raw and mergeable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ForcedTally {
    upper: u32,
    lower: u32,
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

/// One boundary class's forced first-letter tallies for one word: the class's
/// `(mark, quoted)` identity beside its counts. 16 bytes, `Copy`, no interior
/// allocation — the flat-list element that replaced two
/// `BTreeMap<char, ForcedTally>`s per word (see [`WordStats`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Forced {
    mark: char,
    quoted: bool,
    tally: ForcedTally,
}

/// One word's raw case tallies within one book. Mid-flow upper/lower (the
/// intrinsic profile), forced upper/lower per boundary class — split by the
/// *bare* terminal glyph and by the *quote-context* glyph (the `."` classes
/// ADR 0051 discarded to mid-flow) — and book-initial. All raw — no censoring,
/// no trust — so book-supersede holds.
///
/// The forced tallies are **one flat list sorted by `(quoted, mark)`**, not two
/// maps. Two reasons, both measured:
///
/// - *Bytes.* A word is seen at ~2 boundary classes and the overwhelming
///   majority of word types are seen at none at all (forced positions occur
///   once per sentence, not once per word), so two empty `BTreeMap`s were 48
///   bytes of dead inline weight on every one of an English Bible's 265,207
///   per-chapter word entries, plus a full B-tree leaf node (~152 bytes) for
///   each of the ~48,000 that did have one.
/// - *Order.* The sort key is `(quoted, mark)` precisely because `false < true`
///   makes the list iterate bare-glyph classes in mark order and *then*
///   quote-context classes in mark order — byte-for-byte the sequence the two
///   maps produced. That is load-bearing, not cosmetic:
///   [`Model::effective_upper`] sums `f64` discounts in this order and float
///   addition is not associative (see [`Model::build`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct WordStats {
    mid_upper: u32,
    mid_lower: u32,
    book_initial: ForcedTally,
    forced: Vec<Forced>,
}

impl WordStats {
    /// This word's tally slot for one boundary class, inserted in
    /// `(quoted, mark)` order on first sight. Linear/binary over a handful of
    /// entries — the fleet's widest chapter holds 26 distinct classes in total
    /// and a single word sees ~2 — so a sorted flat list beats any map here.
    fn forced_slot(&mut self, mark: char, quoted: bool) -> &mut ForcedTally {
        match self
            .forced
            .binary_search_by(|f| (f.quoted, f.mark).cmp(&(quoted, mark)))
        {
            Ok(i) => &mut self.forced[i].tally,
            Err(i) => {
                self.forced.insert(
                    i,
                    Forced {
                        mark,
                        quoted,
                        tally: ForcedTally::default(),
                    },
                );
                &mut self.forced[i].tally
            }
        }
    }

    /// The bare-terminal classes, in mark order — the old `after_glyph`.
    fn bare(&self) -> impl Iterator<Item = (char, &ForcedTally)> + '_ {
        self.forced
            .iter()
            .filter(|f| !f.quoted)
            .map(|f| (f.mark, &f.tally))
    }

    /// The quote-context classes, in mark order — the old `after_quote`.
    fn quoted(&self) -> impl Iterator<Item = (char, &ForcedTally)> + '_ {
        self.forced
            .iter()
            .filter(|f| f.quoted)
            .map(|f| (f.mark, &f.tally))
    }

    /// Release the list's growth slack. Called at the two points a table stops
    /// growing and starts being *retained* — a chapter's observation and a
    /// book's folded table — because a warm session holds hundreds of thousands
    /// of these and a `Vec`'s doubling slack would otherwise be resident for
    /// the whole session (the same accounting that boxed the outer tables).
    /// The judge model's own table is deliberately left unsealed: it is rebuilt
    /// whenever the aggregate moves, so its slack never accumulates and its
    /// merge wants the amortized growth.
    fn seal(&mut self) {
        self.forced.shrink_to_fit();
    }

    /// Sum another book's counts for the same word into this (corpus-wide
    /// aggregation at judge).
    fn add(&mut self, o: &WordStats) {
        self.mid_upper += o.mid_upper;
        self.mid_lower += o.mid_lower;
        self.book_initial.add(&o.book_initial);
        for f in &o.forced {
            self.forced_slot(f.mark, f.quoted).add(&f.tally);
        }
    }

    fn record(&mut self, pos: PosClass, case: Case) {
        match (pos.kind(), case) {
            (_, Case::Uncased) => {}
            (PosKind::Midflow, Case::Upper) => self.mid_upper += 1,
            (PosKind::Midflow, Case::Lower) => self.mid_lower += 1,
            (PosKind::BookInitial, Case::Upper) => self.book_initial.upper += 1,
            (PosKind::BookInitial, Case::Lower) => self.book_initial.lower += 1,
            (PosKind::ForcedAfterTerminal(ck), c) => {
                let t = self.forced_slot(ck.mark, ck.quoted);
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
            || self.forced.iter().any(|f| f.tally.total() > 0)
    }

    // ── Fold-invariant raw sums (position labels don't affect these). ────────
    fn all_upper(&self) -> u64 {
        u64::from(self.mid_upper)
            + self.book_initial.upper()
            + self.forced.iter().map(|f| f.tally.upper()).sum::<u64>()
    }
    fn all_lower(&self) -> u64 {
        u64::from(self.mid_lower)
            + self.book_initial.lower()
            + self.forced.iter().map(|f| f.tally.lower()).sum::<u64>()
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
        for (_, t) in self.quoted() {
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
    after: &FxHashMap<&str, u64>,
    base: &FxHashMap<&str, u64>,
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
fn tv_distance(p: &FxHashMap<&str, u64>, q: &FxHashMap<&str, u64>, jurors: &[&str]) -> f64 {
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
fn build_trust(words: &FxHashMap<Arc<str>, WordStats>, z: f64) -> FxHashMap<ClassKey, f64> {
    // Baseline word-start distribution + per-class aftermath (reindex).
    let mut word_start_total: FxHashMap<&str, u64> = FxHashMap::default();
    let mut after: FxHashMap<ClassKey, FxHashMap<&str, u64>> = FxHashMap::default();
    for (key, w) in words {
        let key: &str = key;
        let total = w.all_total();
        if total == 0 {
            continue;
        }
        *word_start_total.entry(key).or_default() += total;
        // Bare classes then quote classes, each in mark order — the flat list's
        // own order, which is exactly what the two maps produced, so `after`'s
        // insertion sequence (and every hash iteration derived from it) is
        // unchanged.
        for f in &w.forced {
            if f.tally.total() > 0 {
                *after
                    .entry(ClassKey {
                        mark: f.mark,
                        quoted: f.quoted,
                    })
                    .or_default()
                    .entry(key)
                    .or_default() += f.tally.total();
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
        return FxHashMap::default();
    }

    // W1 case-follow per class: capitalize dominance over lexicon-lowercase
    // followers — exactly ADR 0051's per-glyph habit, re-derived per class.
    let mut w1: FxHashMap<ClassKey, (u64, u64)> = FxHashMap::default();
    for w in words.values() {
        if !w.is_lexicon_lower(z) {
            continue;
        }
        for f in &w.forced {
            let e = w1
                .entry(ClassKey {
                    mark: f.mark,
                    quoted: f.quoted,
                })
                .or_default();
            e.0 += f.tally.upper();
            e.1 += f.tally.total();
        }
    }

    struct Prelim {
        s_case: f64,
        case_seen: bool,
        diff: f64,
        events: u64,
    }
    let mut prelim: FxHashMap<ClassKey, Prelim> = FxHashMap::default();
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

    let mut trust = FxHashMap::with_capacity_and_hasher(prelim.len(), Default::default());
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
/// Owns its word table so the built model can be retained across analyze calls
/// independent of any borrow of the aggregate it was built from (see
/// [`CasingModel`]).
pub(crate) struct Model {
    /// Keyed by shared arena words, so this table's keys cost a refcount bump
    /// each instead of a fresh allocation per corpus word type per build. The
    /// hash is the word's bytes either way, so the insertion sequence — and with
    /// it the iteration order the trust math sums over — is exactly what an
    /// owned-`String` table produced.
    words: FxHashMap<Arc<str>, WordStats>,
    /// Per class trust; `None`-keyed book-initial is always fully trusted.
    trust: FxHashMap<ClassKey, f64>,
    /// Lexicon-restricted capitalize-after-class counts (up, total). `None` =
    /// book-initial. A quote class is present only when promoted.
    habit: FxHashMap<Option<ClassKey>, (u64, u64)>,
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

impl Model {
    fn build(stats: &CasingCorpusStats, cfg: &CasingConfig) -> Model {
        let z = clamp_z(cfg.confidence_z);
        let gate = f64::from(clamp_unit(cfg.trust_gate));
        // Corpus-wide word table: sum each book's raw tallies, books in slug
        // order and each book's words in sorted order. That insertion sequence
        // is load-bearing, not incidental: the reshuffle witness sums a
        // per-juror statistic over this map's iteration order, and float
        // addition is not associative — a different insertion sequence would
        // move trust, and with it every score, in its last bits.
        let mut words: FxHashMap<Arc<str>, WordStats> = FxHashMap::default();
        for (book_words, _) in stats.per_book.values() {
            // An uncased-only word is the sole per-book-safe prune (ADR 0051):
            // it yields no candidate site and enters neither the lexicon-
            // lowercase habit nor the (bicameral) witnesses.
            for (key, w) in book_words.iter().filter(|(_, w)| w.has_case()) {
                words.entry(key.clone()).or_default().add(w);
            }
        }

        let trust = build_trust(&words, z);

        // Lexicon-restricted per-class habit over the words the (baseline)
        // lexicon calls intrinsically lowercase. Bare glyphs and book-initial
        // always contribute (structurally forced); a quote class contributes
        // only when promoted, so trust adds the quote channel without moving the
        // bare-terminal convention.
        let mut habit: FxHashMap<Option<ClassKey>, (u64, u64)> = FxHashMap::default();
        for w in words.values() {
            if !w.is_lexicon_lower(z) {
                continue;
            }
            if w.book_initial.total() > 0 {
                let e = habit.entry(None).or_default();
                e.0 += w.book_initial.upper();
                e.1 += w.book_initial.total();
            }
            for (m, t) in w.bare() {
                let e = habit
                    .entry(Some(ClassKey {
                        mark: m,
                        quoted: false,
                    }))
                    .or_default();
                e.0 += t.upper();
                e.1 += t.total();
            }
            for (m, t) in w.quoted() {
                let ck = ClassKey { mark: m, quoted: true };
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
        for (m, t) in w.quoted() {
            if !self.quote_promoted(m) {
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
        // Bare classes in mark order, then promoted quote classes in mark order.
        // This `f64` accumulation order is the load-bearing one (see
        // [`WordStats`]): float addition is not associative, so the flat list's
        // `(quoted, mark)` sort is what keeps every score bit-identical.
        for (m, t) in w.bare() {
            if t.upper > 0 {
                let ck = ClassKey {
                    mark: m,
                    quoted: false,
                };
                let discount = 1.0 - self.trust_class(ck) * self.habit_dominance(Some(ck));
                up += discount * f64::from(t.upper);
            }
        }
        for (m, t) in w.quoted() {
            if t.upper > 0 && self.quote_promoted(m) {
                let ck = ClassKey { mark: m, quoted: true };
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
        lo += self.bare_sum(w, ForcedTally::lower);
        for (m, t) in w.quoted() {
            if self.quote_promoted(m) {
                lo += t.lower();
            }
        }
        lo
    }

    fn forced_total(&self, w: &WordStats) -> u64 {
        let mut n = w.book_initial.total();
        n += self.bare_sum(w, ForcedTally::total);
        for (m, t) in w.quoted() {
            if self.quote_promoted(m) {
                n += t.total();
            }
        }
        n
    }

    fn bare_sum(&self, w: &WordStats, f: fn(&ForcedTally) -> u64) -> u64 {
        w.bare().map(|(_, t)| f(t)).sum()
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
    fn intrinsic(&self, w: &WordStats) -> Option<Factors> {
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
    fn positional(&self, w: &WordStats, pos: PosClass) -> Option<Factors> {
        if !pos.is_forced() {
            return None;
        }
        let (habit_key, trust) = match pos.kind() {
            PosKind::BookInitial => (None, 1.0),
            PosKind::ForcedAfterTerminal(ck) => {
                if ck.quoted && !self.quote_promoted(ck.mark) {
                    return None; // folded back to mid-flow — not a forced site
                }
                (Some(ck), self.trust_class(ck))
            }
            PosKind::Midflow => return None,
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

/// A lowercase word-start observed by the chapter walk — a flag candidate for
/// either rule. Chapter-local: the address is the verse's index within its
/// chapter run plus a verse-relative byte range, so a hit in an untouched
/// chapter stays correctly addressed after any edit elsewhere and the global
/// `KeyIdx` is resolved once, at materialization.
///
/// **12 bytes**, down from 24. This is the highest-volume retained record in the
/// engine — 668,257 of them on WA-en-ulb — so every field is at its measured
/// width rather than a comfortable one:
///
/// - the span is a [`SiteAddr`] (the existing checked 6-byte packer), *retained*
///   rather than re-derived. Plan §11's principle defaults to re-deriving
///   verse-local offsets, and this row declines that default on measurement: a
///   word ordinal within a verse needs 16 bits on this fleet (WP7a measured
///   1,958 compound words in `hltmcsb`'s widest verse), so an ordinal buys
///   *nothing* over the packed span while costing a `tokenize` +
///   `compound_words` per emitted finding — and no cached segmentation exists at
///   materialization to make that a lookup instead of a re-walk (Entry 26). The
///   same reasoning the mixed-case row recorded, at 50x the population;
/// - `key` is the per-chapter word-type id, `u16` by the [`chapter_word_id`]
///   checked constructor;
/// - `pos` is the one genuinely context-dependent bit, packed into 4 bytes by
///   [`PosClass`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LowerSite {
    /// Chapter-local verse index + verse-relative byte range, packed.
    pub(crate) addr: SiteAddr,
    /// Interned word-type id — an index into the owning [`ChapterWords`]'
    /// `keys` table (per-chapter, first-sight order). A `Copy` id instead of a
    /// `String` so the judge resolves a verdict through an array index per site
    /// instead of hashing the folded word.
    pub(crate) key: ChapterWordId,
    pub(crate) pos: PosClass,
}

/// A word type's id within one chapter's first-sight table. `u16`: WP7a measured
/// the fleet maximum distinct word types in a chapter at **1,125** (`swe`), a
/// 58x margin under the ceiling, and [`chapter_word_id`] enforces the bound
/// rather than assuming it.
pub(crate) type ChapterWordId = u16;

/// Narrow a chapter word-table length to the next [`ChapterWordId`]. Called once
/// per *new word type* in a chapter (not per word), so the checked branch is
/// free. Panics rather than truncating: a chapter with more than 65,535 distinct
/// word types would be 58x the measured fleet maximum and is a stop-and-report
/// event, not something to wrap silently.
fn chapter_word_id(len: usize) -> ChapterWordId {
    ChapterWordId::try_from(len).expect(
        "distinct word types in one chapter fit u16 (fleet max 1,125 — a violation is a \
         stop-and-report, see granularity-spine Entry 26/28)",
    )
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        return PosClass::BOOK_INITIAL;
    }
    match taken {
        Some(p) if p.other => PosClass::MIDFLOW,
        Some(p) if p.quote => PosClass::forced(ClassKey {
            mark: p.mark,
            quoted: true,
        }),
        Some(p) => PosClass::forced(ClassKey {
            mark: p.mark,
            quoted: false,
        }),
        None => PosClass::MIDFLOW,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The casing observation substrate (plan §5.2 / §11 ledger row).
// ─────────────────────────────────────────────────────────────────────────

/// The effect of a run of gap text on the pending-terminal machine, recorded at
/// map time so reduction can resolve a chapter-initial position under **any**
/// entering state without re-reading the text.
///
/// [`advance_gap`] is monotone in a way that makes a two-field summary exact: a
/// pending state that is already live is never replaced by a gap (only its
/// `quote`/`other` flags can be set), and a gap can create a pending only when
/// none was live. So the whole transform is "what this gap produces from
/// nothing" plus "which flags it would OR into something".
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct GapEffect {
    /// The pending state this gap leaves when nothing was pending on entry.
    from_none: Option<Pending>,
    /// The gap contains a close-quote…
    saw_quote: bool,
    /// …and/or a non-quote mark, which collapses the boundary to mid-flow.
    saw_other: bool,
}

impl GapEffect {
    /// Apply the recorded transform to a concrete entering pending state.
    fn apply(self, entering: Option<Pending>) -> Option<Pending> {
        match entering {
            Some(mut p) => {
                p.quote |= self.saw_quote;
                p.other |= self.saw_other;
                Some(p)
            }
            None => self.from_none,
        }
    }

    /// Fold one gap segment in. `prev_letter` starts false for every segment
    /// because a segment is always either a whole word-less verse or a verse's
    /// leading run — and the machine resets `prev_letter` at each verse start
    /// (a terminal opening verse N is not attached to the last letter of verse
    /// N−1). The flag scan is a second pass over the same (short) run rather
    /// than a re-implementation, so the pending machine stays in one place.
    fn extend(&mut self, gap: &str) {
        let mut prev_letter = false;
        advance_gap(gap, &mut self.from_none, &mut prev_letter);
        for c in gap.chars() {
            let cl = class_of(c);
            if cl.is_whitespace() || cl.is_numeric() || cl.is_alphabetic() {
                continue;
            }
            if cl.is_quote() {
                self.saw_quote = true;
            } else {
                self.saw_other = true;
            }
        }
    }
}

/// Everything about one chapter that no entering state can change: its word-type
/// symbols, its per-word raw tallies (excluding the chapter's first word), and
/// its lowercase flag candidates after that first word.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct ChapterWords {
    /// chapter-local id → the word type's symbol in the cache's shared
    /// [`WordInterner`], in first-sight order during the chapter walk. A symbol,
    /// not an owned `String`: a chapter's table only ever needs to *reference* a
    /// word type, and 1,189 chapters of an English Bible hold 265,207 references
    /// to 13,096 distinct types. Symbol equality is string equality (the table is
    /// append-only), so this stays a sound `Eq` for the cached observation.
    keys: Box<[WordSym]>,
    /// Per-id raw tallies. The chapter's **first** word's own occurrence is
    /// absent here: its position class is the one thing the entering boundary
    /// state decides, so reduction records it.
    tallies: Box<[WordStats]>,
    /// Flag candidates after the first word, in scan order.
    sites: Box<[LowerSite]>,
    /// Cased word-starts in the chapter — the emergent-gate input. Position
    /// independent, so the first word counts here too.
    cased_starts: u32,
}

/// The chapter's first word, whose position class reduction resolves.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FirstWord {
    key: ChapterWordId,
    case: Case,
    /// The flag candidate this word contributes, if any. Its `pos` is a
    /// placeholder until reduction fills it in.
    site: Option<LowerSite>,
}

/// The same word with its position class resolved against the entering state.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ResolvedFirst {
    key: ChapterWordId,
    case: Case,
    pos: PosClass,
    site: Option<LowerSite>,
}

/// One chapter's input-independent casing observation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CasingChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book contribution — reduction
    /// changes one word's tally bucket, never the chapter's whole table, so the
    /// table is handed on by `Arc` instead of deep-copied per reduce.
    words: Arc<ChapterWords>,
    /// The gap before the chapter's first word — the whole chapter when it has
    /// no word at all.
    lead: GapEffect,
    first: Option<FirstWord>,
    /// The pending terminal left after the chapter's last word. Chapter-local
    /// by construction: the first word consumed whatever entered, so every
    /// later gap starts from nothing. `None` when the chapter has no word (the
    /// entering state passes through `lead` instead).
    tail: Option<Pending>,
}

/// The casing boundary state carried across chapters — the **complete** state
/// [`ChapterAcc`] carries across a verse seam, which is the same seam a chapter
/// boundary is (a chapter boundary is not a discourse reset).
///
/// Two fields, both necessary and together sufficient:
///
/// - `pending` — the live pending-terminal machine. The chapter's first word's
///   position class is a function of it, so dropping it (or resetting at `\c`)
///   would silently re-classify chapter-initial words: the pericope-adulterae
///   period at JHN 7:53 forces the capital at 8:1.
/// - `book_initial` — whether the book's first word is still ahead. The first
///   word of a book is forced with no terminal glyph (its own habit key, always
///   fully trusted), and a word-less opening chapter carries that fact forward,
///   so it cannot be inferred from the chapter's position alone.
///
/// Nothing else crosses: `prev_letter` (whether a letter immediately precedes)
/// is deliberately verse-local — the machine restarts it at every verse seam,
/// and a chapter seam is a verse seam — and every other input to a word's
/// tally (its fold, its case, its span) is inside its own chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CasingBoundary {
    pending: Option<Pending>,
    book_initial: bool,
}

impl Default for CasingBoundary {
    /// Book start: nothing pending, and the book's first word is still ahead.
    fn default() -> Self {
        CasingBoundary {
            pending: None,
            book_initial: true,
        }
    }
}

/// One chapter's reduced casing result: its shared word table plus its first
/// word with a resolved position class.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct CasingReduced {
    token: Box<str>,
    words: Arc<ChapterWords>,
    first: Option<ResolvedFirst>,
}

impl CasingReduced {
    /// The chapter's flag candidates in scan order — the first word's, then
    /// every later word's.
    fn sites(&self) -> impl Iterator<Item = LowerSite> + '_ {
        self.first
            .and_then(|f| f.site)
            .into_iter()
            .chain(self.words.sites.iter().copied())
    }
}

/// A book's ordered word table: `(folded key, raw tallies)` sorted by key,
/// including uncased-only words (the model prunes those as it folds). Sorted
/// because the corpus merge's insertion order is load-bearing — see
/// [`Model::build`].
///
/// The key is a shared `Arc<str>` from the cache's [`WordInterner`], so building
/// this table — and cloning its keys into the corpus model — copies no bytes.
/// It is deliberately NOT a symbol: the judge sums per-juror statistics over
/// this order and that order is the words' string order, which only owned (or
/// resolved) keys give for free. Making it dense-by-symbol and reconstructing
/// the string order with a permutation was measured and rejected
/// (`documentation/calibration/2026-07-24-word-interner-spike.md`: 60–80x worse
/// per key than a natively ordered table).
type BookWords = Vec<(Arc<str>, WordStats)>;

/// A book's folded casing contribution: its ordered word table (the corpus
/// aggregate's addend), its cased-word-start count, and its chapters' resolved
/// sites with a chapter-local-id → book-word-index map per chapter.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct CasingBookContribution {
    words: Arc<BookWords>,
    cased_starts: u32,
    chapters: Vec<(CasingReduced, Arc<[u32]>)>,
}

/// The casing corpus aggregate: each book's ordered word table and cased-start
/// count, keyed by slug. The tables are shared (`Arc`) with the book
/// contributions they were folded from, so a book replacement is a pointer swap
/// and the aggregate stores no second copy of the corpus vocabulary.
#[derive(Default)]
pub(crate) struct CasingCorpusStats {
    per_book: BTreeMap<Box<str>, (Arc<BookWords>, u32)>,
    /// Bumped whenever `per_book` changes. The judge model is a pure function of
    /// this aggregate and the judging knobs, so this counter is the whole memo
    /// key — no content fingerprint, no deep equality.
    generation: u64,
}

impl CasingCorpusStats {
    /// True iff the merged corpus has any cased word-start — the emergent gate.
    fn any_cased(&self) -> bool {
        self.per_book.values().any(|&(_, starts)| starts > 0)
    }
}

/// The retained judge model: the corpus model plus the aggregate generation and
/// judging knobs it was built from. Rebuilt only when one of those moved.
pub(crate) struct CasingModel {
    generation: u64,
    cfg: CasingConfig,
    model: Arc<Model>,
}

/// One chapter's casing map: the same per-verse walk the rules always ran, with
/// the chapter's first word left unresolved and the gap before it recorded as a
/// transform rather than applied.
struct ChapterAcc {
    cased_starts: u32,
    /// Chapter-local word-type interner: folded key → local id, and local id →
    /// key. The walk tallies into the id-indexed `tallies` (one hash probe per
    /// word) instead of a `BTreeMap<String, _>` entry walk (log n string memcmps
    /// per word). The keys become shared symbols once, in `finish` — the walk
    /// itself never touches the shared table, so the chapter-parallel map seam
    /// takes one lock per chapter rather than one per word.
    intern: FxHashMap<String, ChapterWordId>,
    keys: Vec<String>,
    tallies: Vec<WordStats>,
    sites: Vec<LowerSite>,
    lead: GapEffect,
    first: Option<FirstWord>,
    /// Live only once the first word has been seen; before that the gap goes
    /// into `lead`.
    pending: Option<Pending>,
    /// Reusable `compound_words` scratch buffer (ADR 0057 allocation-diet
    /// follow-up) — cleared and refilled each verse instead of allocating.
    words_buf: Vec<Span>,
    tokens_buf: Vec<crate::token::Token>,
}

impl ChapterAcc {
    fn new() -> Self {
        ChapterAcc {
            cased_starts: 0,
            intern: FxHashMap::default(),
            keys: Vec::new(),
            tallies: Vec::new(),
            sites: Vec::new(),
            lead: GapEffect::default(),
            first: None,
            pending: None,
            words_buf: Vec::new(),
            tokens_buf: Vec::new(),
        }
    }

    fn verse(&mut self, local_idx: LocalKeyIdx, text: &str) {
        self.tokens_buf.clear();
        crate::token::tokenize_into(text, &mut self.tokens_buf);
        compound_words(text, &self.tokens_buf, &mut self.words_buf);
        let mut prev_letter = false;
        let mut cursor = 0usize;

        // Indexed (not `for w in &self.words_buf`): `w` is a `Copy` value
        // read out per iteration, so the loop body is free to mutate other
        // `self` fields without holding a borrow of `words_buf` open.
        for i in 0..self.words_buf.len() {
            let w = self.words_buf[i];
            let gap = &text[cursor..w.start as usize];
            if self.first.is_none() {
                // Still before the chapter's first word: the gap's effect on an
                // arbitrary entering state is recorded, not applied.
                self.lead.extend(gap);
            } else {
                advance_gap(gap, &mut self.pending, &mut prev_letter);
            }

            let word = &text[w.start as usize..w.end as usize];
            let first_char = word.chars().next().unwrap();
            let fcl = class_of(first_char);
            let case = if fcl.is_uppercase() {
                Case::Upper
            } else if fcl.is_lowercase() {
                Case::Lower
            } else {
                Case::Uncased
            };

            if case != Case::Uncased {
                self.cased_starts += 1;
            }
            // The fold is deliberately the exact `str::to_lowercase` of the
            // compound-word span (context-sensitive: final sigma etc.), same
            // as it always was — no fast-path gate, so no drift.
            let key = word.to_lowercase();
            let id = match self.intern.get(&key) {
                Some(&id) => id,
                None => {
                    let id = chapter_word_id(self.keys.len());
                    self.intern.insert(key.clone(), id);
                    self.keys.push(key);
                    self.tallies.push(WordStats::default());
                    id
                }
            };
            // Boundary predicate (ADR 0055): an OtherMixed token (`asÍ`,
            // `word-wOrd`) is `case.mixed-case-word`'s to report — its interior
            // capital is the defect, not its incidental lowercase initial. Skip
            // the lowercase site so the phenomenon is reported once, not twice.
            // The word still tallied above, so it keeps contributing to the
            // lexicon/habit; only the flag candidate is suppressed. (An
            // OtherMixed word whose first letter is uppercase never reaches here
            // — `case` would be `Upper` — so this only touches the first-lower
            // overlap class.)
            let candidate = case == Case::Lower && case_shape(word) != Some(CaseShape::OtherMixed);

            if self.first.is_none() {
                // The chapter's first word: its position class is not knowable
                // here (it depends on what entered the chapter), so neither its
                // tally bucket nor its site's `pos` is decided yet.
                self.first = Some(FirstWord {
                    key: id,
                    case,
                    site: candidate.then_some(LowerSite {
                        addr: SiteAddr::pack(local_idx, w),
                        key: id,
                        pos: PosClass::MIDFLOW,
                    }),
                });
            } else {
                let pos = pos_of(false, self.pending.take());
                self.tallies[id as usize].record(pos, case);
                if candidate {
                    self.sites.push(LowerSite {
                        addr: SiteAddr::pack(local_idx, w),
                        key: id,
                        pos,
                    });
                }
            }

            prev_letter = word.chars().next_back().is_some_and(is_letter);
            cursor = w.end as usize;
        }
        let rest = &text[cursor..];
        if self.first.is_none() {
            self.lead.extend(rest);
        } else {
            advance_gap(rest, &mut self.pending, &mut prev_letter);
        }
    }

    fn finish(mut self, token: &str, symbols: &WordInterner) -> CasingChapterObs {
        // The chapter's tallies stop growing here and start being retained, so
        // each word's forced list releases its growth slack (see
        // `WordStats::seal`).
        for t in &mut self.tallies {
            t.seal();
        }
        CasingChapterObs {
            token: Box::from(token),
            // Boxed, not `Vec`: these three are built once by the walk and never
            // grow again, so a chapter would otherwise retain its `Vec`s' final
            // doubling slack for the whole session — measured at 12 MiB of the
            // site lists' 22 MiB and 6 MiB of the tallies' 23 MiB across an
            // English Bible's 1,189 chapters.
            words: Arc::new(ChapterWords {
                keys: symbols.intern_all(self.keys).into_boxed_slice(),
                tallies: self.tallies.into_boxed_slice(),
                sites: self.sites.into_boxed_slice(),
                cased_starts: self.cased_starts,
            }),
            lead: self.lead,
            // A chapter with no word carries no local pending: whatever entered
            // it flows on through `lead`.
            tail: self.first.is_some().then_some(self.pending).flatten(),
            first: self.first,
        }
    }
}

/// The casing observation substrate — one shared model, two consumer judges
/// (`case.sentence-initial-lowercase` and `case.inconsistent-word-casing`).
pub(crate) struct CasingSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <CasingSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// Casing's judge key: the word type plus the position class of the site being
/// judged. The positional channel reads both; the intrinsic channel reads only
/// the word, and shares the key so one outcome serves both consumers.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CasingKey {
    word: Box<str>,
    pos: PosClass,
}

/// Both consumers' verdicts for one key, computed together from the one shared
/// model. `None` in a channel is that rule staying silent at this key.
#[derive(Clone, Copy, Default)]
pub(crate) struct CasingOutcome {
    /// `case.sentence-initial-lowercase`: score, then its finding args.
    positional: Option<(f32, Option<char>, bool, u32, u32)>,
    /// `case.inconsistent-word-casing`: score, then its count pair. The word
    /// itself comes from the site's own chapter interner at materialization.
    intrinsic: Option<(f32, u32, u32)>,
}

/// The judging half: the corpus model plus the knobs, with the two clamped
/// scalars hoisted out of the per-key path.
pub(crate) struct CasingJudge {
    model: Arc<Model>,
    k: f64,
    floor: f64,
}

impl Clone for CasingJudge {
    fn clone(&self) -> Self {
        CasingJudge {
            model: Arc::clone(&self.model),
            k: self.k,
            floor: self.floor,
        }
    }
}

impl CasingJudge {
    fn new(model: Arc<Model>, cfg: &CasingConfig) -> Self {
        CasingJudge {
            model,
            k: clamp_count(cfg.recurrence_k),
            floor: f64::from(clamp_unit(cfg.emit_score_min)),
        }
    }

    /// Both channels for one `(word, position)` key. The word's tallies are
    /// looked up once and handed to both channels — a word absent from the model
    /// (uncased-only everywhere) is silent in both.
    fn outcome(&self, word: &str, pos: PosClass) -> CasingOutcome {
        let Some(w) = self.model.words.get(word) else {
            return CasingOutcome::default();
        };
        let score_of = |f: &Factors| f.dominance * rarity(f.minority, self.k);
        let positional = self.model.positional(w, pos).and_then(|f| {
            let score = score_of(&f);
            if score < self.floor {
                return None;
            }
            let (glyph, quoted) = pos.habit_glyph();
            Some((score as f32, glyph, quoted, clamp_u32(f.raw_major), clamp_u32(f.raw_total)))
        });
        let intrinsic = self.model.intrinsic(w).and_then(|f| {
            let score = score_of(&f);
            if score < self.floor {
                return None;
            }
            Some((score as f32, clamp_u32(f.raw_major), clamp_u32(f.raw_total)))
        });
        CasingOutcome {
            positional,
            intrinsic,
        }
    }
}

fn clamp_u32(n: u64) -> u32 {
    n.min(u64::from(u32::MAX)) as u32
}

impl crate::substrate::ObservationSubstrate for CasingSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::Casing;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;

    type Key = CasingKey;
    type BoundaryState = CasingBoundary;
    type ChapterObservation = CasingChapterObs;
    type ReducedChapter = CasingReduced;
    type BookContribution = CasingBookContribution;
    type CorpusStats = CasingCorpusStats;
    // Casing has NO extraction knobs: every `CasingConfig` field (the score
    // floor, the recurrence knee, the confidence z, the trust gate) is read at
    // judge. So a knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // The shared folded-word table. Casing's chapter observations store word
    // symbols; the fold resolves them back to words for the book table's
    // canonical (string) order. `MixedCase` is the next consumer of the same
    // table.
    type Symbols = WordInterner;
    type JudgeConfig = CasingJudge;
    type EntryOutcome = CasingOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        symbols: &WordInterner,
    ) -> CasingChapterObs {
        let mut acc = ChapterAcc::new();
        for (vi, text) in chapter.texts.iter().enumerate() {
            acc.verse(LocalKeyIdx::from_usize(vi), text);
        }
        acc.finish(chapter.chapter, symbols)
    }

    fn pending_owner(_state: &CasingBoundary) -> Option<&str> {
        // Nothing is ever deposited backwards for casing: a chapter's own
        // reduction consumes the state that entered it (to classify its first
        // word) and produces its own. A successor never writes into a
        // predecessor's contribution, which is why the replay window always
        // starts at the chapter that changed.
        None
    }

    fn reduce_chapter(
        observation: &CasingChapterObs,
        entering: &CasingBoundary,
        _carry_out: &mut CasingReduced,
    ) -> (CasingReduced, CasingBoundary) {
        let at_first = observation.lead.apply(entering.pending);
        let (first, leaving) = match observation.first {
            None => (
                None,
                // A word-less chapter is a pass-through: the entering state
                // flows on, modified only by the chapter's own gap text.
                CasingBoundary {
                    pending: at_first,
                    book_initial: entering.book_initial,
                },
            ),
            Some(f) => {
                let pos = pos_of(entering.book_initial, at_first);
                (
                    Some(ResolvedFirst {
                        key: f.key,
                        case: f.case,
                        pos,
                        site: f.site.map(|s| LowerSite { pos, ..s }),
                    }),
                    // Everything after the first word is this chapter's own
                    // text, so the leaving state is independent of what entered
                    // — which is why a changed carry converges within one
                    // chapter of the edit.
                    CasingBoundary {
                        pending: observation.tail,
                        book_initial: false,
                    },
                )
            }
        };
        (
            CasingReduced {
                token: observation.token.clone(),
                words: Arc::clone(&observation.words),
                first,
            },
            leaving,
        )
    }

    fn finish_book(_leaving: &CasingBoundary, _carry_out: &mut CasingReduced) {
        // A pending terminal live at the book's end has no following word to
        // force, so it resolves to nothing — there is no book-edge contribution
        // to fold back.
    }

    fn fold_book(reduced: &[CasingReduced], symbols: &WordInterner) -> CasingBookContribution {
        // Book-level interner over the chapters' own id spaces, so the judge can
        // memoize a verdict once per book-word (as it always did) while the
        // sites stay chapter-local. Keyed by the shared SYMBOL, so this whole
        // pass hashes 4-byte integers instead of words and never touches the
        // arena; the words are resolved once, below, for the sort.
        let mut intern: FxHashMap<WordSym, u32> = FxHashMap::default();
        let mut syms: Vec<WordSym> = Vec::new();
        let mut tallies: Vec<WordStats> = Vec::new();
        let mut per_chapter: Vec<Vec<u32>> = Vec::with_capacity(reduced.len());
        let mut cased_starts: u32 = 0;
        for r in reduced {
            cased_starts += r.words.cased_starts;
            let mut ids = Vec::with_capacity(r.words.keys.len());
            for (i, &sym) in r.words.keys.iter().enumerate() {
                let id = match intern.get(&sym) {
                    Some(&id) => id,
                    None => {
                        let id = syms.len() as u32;
                        intern.insert(sym, id);
                        syms.push(sym);
                        tallies.push(WordStats::default());
                        id
                    }
                };
                ids.push(id);
                tallies[id as usize].add(&r.words.tallies[i]);
            }
            if let Some(f) = r.first {
                tallies[ids[f.key as usize] as usize].record(f.pos, f.case);
            }
            per_chapter.push(ids);
        }
        // Sort into the pinned order and remap the chapters' id maps onto it.
        // The order is the keys' STRING order — symbols are assigned in
        // first-sight (map-completion) order and carry no meaning here beyond
        // identity, so the resolved words are what the sort compares.
        let resolved = symbols.resolve_all(syms.iter().copied());
        let mut order: Vec<u32> = (0..resolved.len() as u32).collect();
        order.sort_unstable_by(|&a, &b| resolved[a as usize].cmp(&resolved[b as usize]));
        let mut rank = vec![0u32; resolved.len()];
        for (r, &i) in order.iter().enumerate() {
            rank[i as usize] = r as u32;
        }
        let words: BookWords = order
            .iter()
            .map(|&i| {
                let mut w = tallies[i as usize].clone();
                // Retained in the corpus aggregate from here on — seal its
                // forced list (see `WordStats::seal`).
                w.seal();
                (Arc::clone(&resolved[i as usize]), w)
            })
            .collect();
        let chapters = reduced
            .iter()
            .cloned()
            .zip(per_chapter)
            .map(|(r, ids)| {
                let ids: Arc<[u32]> = ids.into_iter().map(|id| rank[id as usize]).collect();
                (r, ids)
            })
            .collect();
        CasingBookContribution {
            words: Arc::new(words),
            cased_starts,
            chapters,
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut CasingCorpusStats,
        slug: &str,
        old: Option<&CasingBookContribution>,
        new: Option<&CasingBookContribution>,
    ) -> Vec<CasingKey> {
        let moved = match (old, new) {
            (Some(o), Some(n)) => !Arc::ptr_eq(&o.words, &n.words) && o.words != n.words,
            (None, None) => false,
            _ => true,
        };
        match new {
            Some(n) => {
                stats
                    .per_book
                    .insert(Box::from(slug), (Arc::clone(&n.words), n.cased_starts));
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        if moved {
            stats.generation += 1;
        }
        // The stats delta is deliberately empty, and the driver derives
        // judge-dirtiness from `generation` instead. This substrate's judge is
        // corpus-global: the dominance, per-class habit, and trust a word is
        // judged against are functions of *every* word's tallies, so the exact
        // set of keys whose verdict inputs moved is either the empty set (the
        // aggregate did not change) or every key in the corpus. Returning a key
        // per word type to state the second case would allocate the whole
        // vocabulary to say what one counter already says — and returning only
        // the words whose own tallies moved would be a subset, which is the one
        // answer that is wrong.
        Vec::new()
    }

    fn judge(judge: &CasingJudge, key: &CasingKey, _stats: &CasingCorpusStats) -> CasingOutcome {
        judge.outcome(&key.word, key.pos)
    }
}

/// Which of the substrate's two consumers this analysis emits for. Either may be
/// off while the other keeps the shared substrate alive.
#[derive(Clone, Copy)]
pub(crate) struct Consumers {
    pub(crate) positional: bool,
    pub(crate) intrinsic: bool,
}

/// Per-book verdict memo: for each book-word id, the chain of position classes
/// judged for it so far. A frequent word genuinely appears at several position
/// classes (`the` sits mid-flow and after a terminal, interleaved), so a
/// one-entry-per-word cache thrashes on exactly the words that matter; a chain
/// keeps every pair while still costing an array index plus one or two `PosClass`
/// compares per site, instead of the hash probe per site that was ~half the
/// whole casing judge on an all-rules corpus.
struct VerdictMemo {
    /// book-word id → first node index, or `NIL`.
    head: Vec<u32>,
    nodes: Vec<(PosClass, CasingOutcome, u32)>,
}

const NIL: u32 = u32::MAX;

impl VerdictMemo {
    fn new(words: usize) -> Self {
        VerdictMemo {
            head: vec![NIL; words],
            nodes: Vec::new(),
        }
    }

    /// The verdict for `(book-word id, pos)`, computing and linking it on a miss.
    /// Returns `true` alongside it when it was computed (the `judged` probe).
    fn get(
        &mut self,
        id: usize,
        pos: PosClass,
        judged: &mut usize,
        compute: impl FnOnce() -> CasingOutcome,
    ) -> CasingOutcome {
        let mut n = self.head[id];
        while n != NIL {
            let node = &self.nodes[n as usize];
            if node.0 == pos {
                return node.1;
            }
            n = node.2;
        }
        let outcome = compute();
        self.nodes.push((pos, outcome, self.head[id]));
        self.head[id] = (self.nodes.len() - 1) as u32;
        *judged += 1;
        outcome
    }
}

impl CasingBookContribution {
    /// Emit both consumers' findings for this book, in one pass over its sites.
    /// `positional`/`intrinsic` are the two rules' enabled bits: either may be
    /// off while the other keeps the shared substrate alive.
    fn materialize(
        &self,
        layout: &[crate::corpus::ChapterLayout],
        judge: &CasingJudge,
        enabled: Consumers,
        out: &mut Vec<Finding>,
        judged: &mut usize,
    ) {
        let Consumers {
            positional,
            intrinsic,
        } = enabled;
        let mut memo = VerdictMemo::new(self.words.len());
        // Positional zip is truncating: a missing or extra trailing chapter
        // would silently DROP findings rather than fail. Chapter cardinality is
        // the alignment precondition; the token check at each pair (inside
        // `chapter_base`) proves the pairing, but only for pairs that exist.
        assert_eq!(
            self.chapters.len(),
            layout.len(),
            "materialize: contribution/layout chapter count mismatch"
        );
        for ((chapter, ids), block) in self.chapters.iter().zip(layout) {
            let base = crate::substrate::chapter_base(block, &chapter.token);
            for site in chapter.sites() {
                // Resolve the folded word through the BOOK's table, not the
                // chapter's: one contiguous allocation per book keeps the judge's
                // lookups on warm cache lines, where 1,189 per-chapter interners
                // would scatter them.
                let bid = ids[usize::from(site.key)] as usize;
                let word = &self.words[bid].0;
                let outcome = memo.get(bid, site.pos, judged, || judge.outcome(word, site.pos));
                if positional
                    && let Some((score, glyph, quoted, upper, total)) = outcome.positional
                {
                    out.push(Finding {
                        key_idx: rebase(base, site.addr.unpack().0),
                        code: SENTENCE_INITIAL_LOWERCASE,
                        severity: Severity::Info,
                        range: site_span(&site),
                        score: Some(score),
                        args: Some(FindingArgs::CasingConvention {
                            glyph,
                            quoted,
                            upper,
                            total,
                        }),
                    });
                }
                if intrinsic
                    && let Some((score, upper, total)) = outcome.intrinsic
                {
                    out.push(Finding {
                        key_idx: rebase(base, site.addr.unpack().0),
                        code: INCONSISTENT_WORD_CASING,
                        severity: Severity::Info,
                        range: site_span(&site),
                        score: Some(score),
                        args: Some(FindingArgs::WordCasing {
                            word: word.to_string(),
                            upper,
                            total,
                        }),
                    });
                }
            }
        }
    }
}

/// The resident state one casing drive reads and writes: the substrate's own
/// cache, the retained judge-model memo, and the shared word table. They are
/// three sibling fields of one cache section that are only ever handed over
/// together — the model is a pure function of the cache's aggregate, and the
/// word table names the words that aggregate is keyed by.
pub(crate) struct CasingState<'a> {
    pub(crate) cache: &'a mut crate::substrate::SubstrateCache<CasingSubstrate>,
    pub(crate) retained: &'a mut Option<CasingModel>,
    pub(crate) symbols: &'a WordInterner,
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct CasingMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the casing observation substrate and both its consumer judges for one
/// analysis: map the dirty chapters through the ordered chapter-map seam, replay
/// each book's ordered reduction to convergence, build (or reuse) the corpus
/// model, and materialize every book's findings for the enabled consumers.
///
/// Mapping fans out; **reduction does not**. Chapter `n + 1` consumes chapter
/// `n`'s boundary state, so a book's reduction is a sequential carry fold — it
/// walks compact cached observations, not text, and stays deterministic.
pub(crate) fn drive_casing(
    positional: bool,
    intrinsic: bool,
    state: CasingState<'_>,
    corpus: &Corpus,
    cfg: &CasingConfig,
    out: &mut Vec<Finding>,
) {
    let CasingState {
        cache,
        retained,
        symbols,
    } = state;
    use crate::substrate::{
        ChapterView, DrivePhase, DriveProbe, ObservationInputStamp, ObservationSubstrate,
    };
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !positional && !intrinsic {
        cache.clear();
        *retained = None;
        return;
    }
    let mut probe = DriveProbe::new(crate::substrate::SubstrateId::Casing);
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    // Planning pass. Stamps are built once and handed to both the seam and the
    // driver; the dirty question is put to the cache with the same predicate the
    // driver reuses by, so the two cannot disagree.
    // Borrowed chapter tokens: the layout owns them and outlives the drive, so
    // the planning pass never allocates. `update_book` takes ownership only
    // where it rebuilds a persistent cache entry.
    let mut stamped: Vec<Vec<(&str, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<CasingMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp {
                schema_stamp: CasingSubstrate::SCHEMA_STAMP,
                chapter_hash: c.hash,
                extractor_fp: CasingSubstrate::extractor_fp(&()),
            };
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(CasingMapWork {
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
        CasingSubstrate::map_chapter(&w.view, &(), symbols)
    });
    // Back into caller-order `(book, chapter)` slots. Reduction reads them in
    // corpus order, never completion order, so serial and parallel builds — and
    // any thread count — produce identical reductions.
    let mut slots: Vec<Vec<Option<CasingChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    probe.mark(DrivePhase::Map);
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], symbols, |i| {
            // Pre-mapped above. The planning pass asked the cache the same
            // question the driver asks, so this slot is filled whenever the
            // driver wants it; mapping in place is the correct answer if it ever
            // is not.
            slots[bi][i].take().unwrap_or_else(|| {
                let c = &book.chapters[i];
                CasingSubstrate::map_chapter(
                    &ChapterView {
                        chapter: &c.chapter,
                        texts: &texts[c.range.clone()],
                    },
                    &(),
                    symbols,
                )
            })
        });
    }

    probe.mark(DrivePhase::Reduce);
    let stats = cache.corpus_stats();
    // Emergent gate: no cased word-starts, no convention to violate.
    if !stats.any_cased() {
        *retained = None;
        // Complete the phase accounting even on the emergent early-out, so a
        // probe reading over an uncased corpus still sums to the drive's cost
        // (the probe is only exhaustive if every exit marks every phase).
        probe.mark(DrivePhase::Keys);
        probe.mark(DrivePhase::Judge);
        probe.mark(DrivePhase::Materialize);
        return;
    }
    // Judge-dirty keys, per the substrate's own derivation: the model is a pure
    // function of the corpus aggregate and the judging knobs, and every key's
    // verdict is a function of the whole model. So either nothing moved and
    // every retained verdict stands, or the model moved and every key is dirty.
    let reusable = retained
        .as_ref()
        .is_some_and(|m| m.generation == stats.generation && m.cfg == *cfg);
    if !reusable {
        *retained = Some(CasingModel {
            generation: stats.generation,
            cfg: *cfg,
            model: Arc::new(Model::build(stats, cfg)),
        });
    }
    let model = retained.as_ref().expect("just built or reused");
    let judge = CasingJudge::new(Arc::clone(&model.model), cfg);
    // Casing's judge key set is the whole model: building or reusing it above IS
    // the key phase, and the per-site verdicts are drawn inside materialization,
    // so `judge` stays zero here and materialization carries both.
    probe.mark(DrivePhase::Keys);
    let mut judged = 0usize;
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(
                &book.chapters,
                &judge,
                Consumers {
                    positional,
                    intrinsic,
                },
                out,
                &mut judged,
            );
        }
    }
    probe.mark(DrivePhase::Materialize);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = judged;
    }
    let _ = judged;
}

/// Casing findings for a whole corpus at a given config, via the observation
/// substrate over a fresh transient cache — the single casing implementation,
/// for tests and calibration callers. Findings are in the final stable order.
#[cfg(test)]
pub(crate) fn casing_findings(
    corpus: &Corpus,
    cfg: &CasingConfig,
    positional: bool,
    intrinsic: bool,
) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let mut retained = None;
    let symbols = WordInterner::default();
    let mut out = Vec::new();
    drive_casing(
        positional,
        intrinsic,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        corpus,
        cfg,
        &mut out,
    );
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

/// A site's verse-relative span, unpacked from its [`SiteAddr`].
fn site_span(site: &LowerSite) -> Span {
    site.addr.unpack().1
}

/// The widest field extents one corpus's casing segmentation produces:
/// `(max words in one verse, max distinct word types in one chapter, max
/// distinct boundary classes in one chapter)`. Measured through the exact
/// [`compound_words`] segmentation and fold the map walk uses, so it is the
/// segmentation's own answer rather than a proxy.
///
/// The site record's field widths are only sound while these stay inside their
/// integer ceilings, and no corpus statistic predicts them — so the fleet is
/// measured rather than assumed (`bench-probes` only; the shipped path proves
/// the same bounds with checked constructors).
#[cfg(feature = "bench-probes")]
pub fn field_extent_probe(corpus: &Corpus) -> (usize, usize, usize) {
    let texts = corpus.texts();
    let mut max_words = 0usize;
    let mut max_types = 0usize;
    let mut max_classes = 0usize;
    let mut words_buf = Vec::new();
    let mut tokens_buf = Vec::new();
    for book in corpus.book_layout() {
        let mut book_initial = true;
        for c in &book.chapters {
            let mut types: FxHashMap<String, ()> = FxHashMap::default();
            let mut classes: FxHashMap<PosClass, ()> = FxHashMap::default();
            let mut pending: Option<Pending> = None;
            for text in &texts[c.range.clone()] {
                tokens_buf.clear();
                crate::token::tokenize_into(text, &mut tokens_buf);
                compound_words(text, &tokens_buf, &mut words_buf);
                max_words = max_words.max(words_buf.len());
                let mut prev_letter = false;
                let mut cursor = 0usize;
                for w in words_buf.iter().copied() {
                    advance_gap(&text[cursor..w.start as usize], &mut pending, &mut prev_letter);
                    let word = &text[w.start as usize..w.end as usize];
                    types.insert(word.to_lowercase(), ());
                    classes.insert(pos_of(book_initial, pending.take()), ());
                    book_initial = false;
                    prev_letter = word.chars().next_back().is_some_and(is_letter);
                    cursor = w.end as usize;
                }
                advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
            }
            max_types = max_types.max(types.len());
            max_classes = max_classes.max(classes.len());
        }
    }
    (max_words, max_types, max_classes)
}

// ─────────────────────────────────────────────────────────────────────────
// Calibration API (ADR 0051/0052). The `--casing` harness in calibrate.rs
// consumes this to sweep floor/k and track review anchors over the real walk,
// model, trust map, and gate; it is not used by the shipped judges.
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
/// knobs (including `trust_gate`) — the calibration entry point. Drives the
/// substrate over a fresh transient cache, so it sees exactly the evidence the
/// shipped judges see.
pub fn evaluate(corpus: &Corpus, cfg: &CasingConfig) -> Vec<SiteEval> {
    use crate::substrate::ObservationSubstrate;
    let mut cache: crate::substrate::SubstrateCache<CasingSubstrate> =
        crate::substrate::SubstrateCache::new();
    let mut retained = None;
    let mut sink = Vec::new();
    let symbols = WordInterner::default();
    drive_casing(
        true,
        true,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        corpus,
        cfg,
        &mut sink,
    );
    let Some(retained) = retained else {
        return Vec::new();
    };
    let model = &retained.model;
    let mut out = Vec::new();
    for book in corpus.book_layout() {
        let Some(contrib) = cache.book_contribution(&book.slug) else {
            continue;
        };
        for (chapter, ids) in &contrib.chapters {
            let Some(range) = corpus.chapter_range(&book.slug, &chapter.token) else {
                continue;
            };
            let base = crate::corpus::KeyIdx::from_usize(range.start);
            for site in chapter.sites() {
                let word = &contrib.words[ids[usize::from(site.key)] as usize].0;
                let Some(w) = model.words.get(&**word) else {
                    continue;
                };
                let span = site_span(&site);
                out.push(SiteEval {
                    key_idx: rebase(base, site.addr.unpack().0),
                    start: span.start,
                    end: span.end,
                    pos: site.pos,
                    intrinsic: model.intrinsic(w),
                    positional: model.positional(w, site.pos),
                });
            }
        }
    }
    let _ = <CasingSubstrate as ObservationSubstrate>::ID;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PosClass` is a packed `u32` retained on every lowercase site, so its
    /// encoding is a storage contract: every accepted `(mark, quoted)` pair must
    /// survive a round trip, and neither structural sentinel may be reachable
    /// from a forced class. Astral-plane marks are included deliberately — a
    /// 16-bit or BMP-only assumption is exactly the bug this encoding exists to
    /// make impossible.
    #[test]
    fn pos_class_round_trips_every_variant_including_astral_marks() {
        assert_eq!(PosClass::MIDFLOW.kind(), PosKind::Midflow);
        assert_eq!(PosClass::BOOK_INITIAL.kind(), PosKind::BookInitial);
        assert!(!PosClass::MIDFLOW.is_forced());
        assert!(PosClass::BOOK_INITIAL.is_forced());

        // Boundary of the scalar range, both quote contexts: ASCII terminals,
        // non-BMP marks, and the two ends of the Unicode scalar space.
        let marks = [
            '\u{0}',
            '.',
            '?',
            '\u{0589}',       // Armenian full stop
            '\u{3002}',       // ideographic full stop
            '\u{D7FF}',       // last scalar before the surrogate hole
            '\u{E000}',       // first scalar after it
            '\u{FFFF}',       // last BMP scalar
            '\u{1F600}',      // astral plane
            '\u{10FFFF}',     // the highest Unicode scalar
        ];
        for mark in marks {
            for quoted in [false, true] {
                let ck = ClassKey { mark, quoted };
                let p = PosClass::forced(ck);
                assert_eq!(
                    p.kind(),
                    PosKind::ForcedAfterTerminal(ck),
                    "{mark:?}/{quoted} must round-trip"
                );
                assert!(p.is_forced());
                assert_eq!(p.habit_glyph(), (Some(mark), quoted));
                // A forced class can never collide with a structural sentinel.
                assert_ne!(p, PosClass::MIDFLOW);
                assert_ne!(p, PosClass::BOOK_INITIAL);
                // The quote bit is the ONLY difference between the two contexts.
                assert_ne!(p, PosClass::forced(ClassKey { mark, quoted: !quoted }));
            }
        }
    }

    /// The site record's width is the point of the WP7b storage rework, so it is
    /// pinned: 668,257 of these are retained on WA-en-ulb, and a field silently
    /// widening back to 16 or 24 bytes is a multi-MiB regression that no
    /// behavioral test would notice.
    #[test]
    fn the_lowercase_site_record_stays_twelve_bytes() {
        assert_eq!(std::mem::size_of::<LowerSite>(), 12);
        assert_eq!(std::mem::size_of::<PosClass>(), 4);
    }

    /// The `u16` chapter word-id bound is enforced, not assumed. WP7a measured
    /// the fleet maximum at 1,125 distinct word types in a chapter — a 58x
    /// margin — and a corpus that broke it must stop rather than wrap.
    #[test]
    #[should_panic(expected = "distinct word types in one chapter fit u16")]
    fn the_chapter_word_id_bound_panics_instead_of_truncating() {
        chapter_word_id(usize::from(u16::MAX) + 1);
    }

    /// The last representable id is not itself rejected — an off-by-one in the
    /// bound would silently cap chapters one word type early.
    #[test]
    fn the_chapter_word_id_bound_admits_the_last_representable_id() {
        assert_eq!(chapter_word_id(usize::from(u16::MAX)), u16::MAX);
    }

    /// The packed form keeps the semantic ordering the tagged enum derived:
    /// `BookInitial` < forced (by `(mark, quoted)`) < `Midflow`.
    #[test]
    fn pos_class_ordering_is_semantic_not_bitwise() {
        let bare = PosClass::forced(ClassKey { mark: '.', quoted: false });
        let quoted = PosClass::forced(ClassKey { mark: '.', quoted: true });
        let later = PosClass::forced(ClassKey { mark: '?', quoted: false });
        assert!(PosClass::BOOK_INITIAL < bare);
        assert!(bare < quoted);
        assert!(quoted < later, "mark orders before the quote flag");
        assert!(later < PosClass::MIDFLOW);
    }

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

    /// One consumer of the shared substrate, at a config — the test handle that
    /// replaced the two retired rule structs.
    struct Consumer {
        cfg: CasingConfig,
        positional: bool,
        intrinsic: bool,
    }

    fn run(corpus: &Corpus, r: &Consumer) -> Vec<Finding> {
        casing_findings(corpus, &r.cfg, r.positional, r.intrinsic)
    }

    fn intrinsic(cfg: CasingConfig) -> Consumer {
        Consumer {
            cfg,
            positional: false,
            intrinsic: true,
        }
    }
    fn positional(cfg: CasingConfig) -> Consumer {
        Consumer {
            cfg,
            positional: true,
            intrinsic: false,
        }
    }

    /// Drive one corpus through a resident substrate cache, returning both
    /// consumers' findings in the final stable order.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<CasingSubstrate>,
        retained: &mut Option<CasingModel>,
        symbols: &WordInterner,
        corpus: &Corpus,
        cfg: &CasingConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_casing(
            true,
            true,
            CasingState {
                cache,
                retained,
                symbols,
            },
            corpus,
            cfg,
            &mut out,
        );
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));
        out
    }

    /// Comparable rendering: key string, code, span text, score, args.
    fn render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                format!(
                    "{}|{}|{}|{:?}|{:?}",
                    corpus.key(f.key_idx),
                    f.code.code(),
                    f.range.slice(corpus.text(f.key_idx)),
                    f.score,
                    f.args
                )
            })
            .collect()
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
        let mut cache: crate::substrate::SubstrateCache<CasingSubstrate> =
            crate::substrate::SubstrateCache::new();
        let mut retained = None;
        let cfg = CasingConfig::default();
        let mut sink = Vec::new();
        let symbols = WordInterner::default();
        drive_casing(
        true,
        true,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        corpus,
        &cfg,
        &mut sink,
    );
        retained
            .expect("a cased corpus builds a model")
            .model
            .trust_class(ClassKey { mark, quoted })
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

    /// Book supersede over a resident substrate cache: replacing a book's
    /// content replaces its whole contribution to the corpus aggregate, and
    /// dropping a book removes it.
    #[test]
    fn book_supersede_over_a_resident_cache() {
        let c = cfg(0.5, 32.0, 0.0);
        let mut cache = crate::substrate::SubstrateCache::new();
        let mut retained = None;
        let symbols = WordInterner::default();

        let dirty = cycle("GEN", &["we saw Jesus"], 20);
        let dirty = push_verse(dirty, "GEN", 100, "we saw jesus");
        assert_eq!(
            resident(&mut cache, &mut retained, &symbols, &dirty, &c)
                .iter()
                .filter(|f| f.code == INCONSISTENT_WORD_CASING)
                .count(),
            1
        );

        // Same book, fixed content: the old contribution must not survive.
        let fixed = cycle("GEN", &["we saw Jesus"], 20);
        let fixed = push_verse(fixed, "GEN", 100, "we saw Jesus");
        assert!(resident(&mut cache, &mut retained, &symbols, &fixed, &c).is_empty());

        // A second book carries the slip; dropping the first leaves it alone.
        let two = cycle("GEN", &["we saw Jesus"], 20);
        let two = extend_corpus(two, book("EXO", &[(1, "we saw jesus")]));
        assert_eq!(
            resident(&mut cache, &mut retained, &symbols, &two, &c)
                .iter()
                .filter(|f| f.code == INCONSISTENT_WORD_CASING)
                .count(),
            1
        );
        cache.remove_book("GEN");
        let exo = book("EXO", &[(1, "we saw jesus")]);
        // GEN carried the capital-dominant evidence; without it `jesus` has no
        // convention to violate.
        assert!(resident(&mut cache, &mut retained, &symbols, &exo, &c).is_empty());
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

    // ── The observation substrate: boundary state, replay, and work. ─────────

    use crate::substrate::{ChapterView, ObservationSubstrate, SubstrateCache};

    /// A multi-chapter single-book corpus: `chapters[i]` holds chapter `i + 1`'s
    /// verse texts, numbered from 1 within the chapter.
    fn chaptered(slug: &str, chapters: &[Vec<String>]) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for (ci, verses) in chapters.iter().enumerate() {
            for (vi, text) in verses.iter().enumerate() {
                keys.push(format!("{slug} {}:{}", ci + 1, vi + 1));
                texts.push(text.clone());
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn lines(templates: &[&str], reps: usize) -> Vec<String> {
        (0..reps)
            .flat_map(|_| templates.iter().map(|t| (*t).to_string()))
            .collect()
    }

    fn map_one(token: &str, verses: &[&str]) -> CasingChapterObs {
        map_one_with(token, verses, &WordInterner::default())
    }

    /// The same, against a caller-owned symbol table — for a test that maps two
    /// chapters and needs their symbols to be comparable.
    fn map_one_with(token: &str, verses: &[&str], symbols: &WordInterner) -> CasingChapterObs {
        let texts: Vec<String> = verses.iter().map(|v| (*v).to_string()).collect();
        CasingSubstrate::map_chapter(
            &ChapterView {
                chapter: token,
                texts: &texts,
            },
            &(),
            symbols,
        )
    }

    /// Symbols are *naming*, not evidence: the same chapter text mapped against a
    /// word table that already holds other words gets different symbol numbers
    /// and still folds to the byte-identical **book contribution**. This is the
    /// invariant that lets the shared table be append-only and lets chapter
    /// mapping fan out (symbols are assigned in map-completion order, which is
    /// not deterministic across thread counts — so nothing downstream may read
    /// them as anything but identity).
    ///
    /// Scope is exactly the book fold, which is what the name says: the equality
    /// asserted here is `fold_book`'s output, not the emitted findings. The
    /// end-to-end egress version of this claim is
    /// `mixed_case::tests::a_prefilled_interner_changes_no_finding` (and the
    /// oracle, which drives every corpus through one shared table).
    #[test]
    fn symbol_numbering_never_reaches_the_book_fold() {
        let verses = ["There we go there.", "there he goes"];
        let fresh = WordInterner::default();
        let a = map_one_with("1", &verses, &fresh);

        let warm = WordInterner::default();
        let _ = map_one_with("0", &["Wholly different vocabulary appears first."], &warm);
        let b = map_one_with("1", &verses, &warm);

        assert_ne!(
            a.words.keys, b.words.keys,
            "the two tables must have numbered these words differently"
        );

        let fold = |obs: &CasingChapterObs, symbols: &WordInterner| {
            let mut sink = CasingReduced::default();
            let (reduced, _) =
                CasingSubstrate::reduce_chapter(obs, &CasingBoundary::default(), &mut sink);
            CasingSubstrate::fold_book(&[reduced], symbols)
        };
        assert_eq!(
            fold(&a, &fresh).words,
            fold(&b, &warm).words,
            "the folded book table is keyed by words in string order, so it \
             cannot see which integers named them"
        );
    }

    /// The flat forced list iterates **every bare class in mark order, then
    /// every quote class in mark order** — the exact sequence the two
    /// `BTreeMap`s it replaced produced. `Model::effective_upper` sums `f64`
    /// discounts in this order, and float addition is not associative, so this
    /// is a correctness property and not a tidiness one.
    #[test]
    fn the_forced_list_iterates_bare_then_quote_each_in_mark_order() {
        let mut w = WordStats::default();
        // Recorded in a deliberately scrambled order.
        for (mark, quoted) in [
            ('.', true),
            ('!', false),
            ('?', true),
            ('.', false),
            ('!', true),
            ('?', false),
        ] {
            w.record(
                PosClass::forced(ClassKey { mark, quoted }),
                Case::Upper,
            );
        }
        assert_eq!(
            w.forced
                .iter()
                .map(|f| (f.mark, f.quoted))
                .collect::<Vec<_>>(),
            vec![
                ('!', false),
                ('.', false),
                ('?', false),
                ('!', true),
                ('.', true),
                ('?', true)
            ]
        );
        assert_eq!(w.bare().map(|(m, _)| m).collect::<Vec<_>>(), vec!['!', '.', '?']);
        assert_eq!(w.quoted().map(|(m, _)| m).collect::<Vec<_>>(), vec!['!', '.', '?']);
        // A merge preserves it, and does not duplicate a class already present.
        let mut other = WordStats::default();
        other.record(
            PosClass::forced(ClassKey {
                mark: ',',
                quoted: false,
            }),
            Case::Lower,
        );
        other.record(
            PosClass::forced(ClassKey {
                mark: '.',
                quoted: false,
            }),
            Case::Lower,
        );
        w.add(&other);
        assert_eq!(w.bare().map(|(m, _)| m).collect::<Vec<_>>(), vec!['!', ',', '.', '?']);
        assert_eq!(w.bare().map(|(_, t)| t.total()).sum::<u64>(), 5);
    }

    fn reduce_one(
        obs: &CasingChapterObs,
        entering: &CasingBoundary,
    ) -> (Option<PosClass>, CasingBoundary) {
        let mut sink = CasingReduced::default();
        let (reduced, leaving) = CasingSubstrate::reduce_chapter(obs, entering, &mut sink);
        (reduced.first.map(|f| f.pos), leaving)
    }

    /// The boundary state after a chapter ending in a bare terminal.
    fn after_terminal(mark: char) -> CasingBoundary {
        CasingBoundary {
            pending: Some(Pending {
                mark,
                quote: false,
                other: false,
            }),
            book_initial: false,
        }
    }

    /// The chapter-initial word's position class is decided entirely by the
    /// entering boundary state — this is the whole reason the state has these two
    /// fields and no others.
    #[test]
    fn the_entering_state_decides_the_chapter_initial_position() {
        let obs = map_one("8", &["there he goes", "and on"]);

        // A pending terminal from the previous chapter forces the position: the
        // pericope-adulterae shape, where the period ending JHN 7:53 forces the
        // capital opening 8:1.
        assert_eq!(
            reduce_one(&obs, &after_terminal('.')).0,
            Some(PosClass::forced(ClassKey {
                mark: '.',
                quoted: false,
            })),
            "a pending terminal must cross the chapter seam"
        );
        // Nothing pending: not forced. A chapter start is not a sentence start.
        assert_eq!(
            reduce_one(
                &obs,
                &CasingBoundary {
                    pending: None,
                    book_initial: false
                }
            )
            .0,
            Some(PosClass::MIDFLOW),
            "a chapter boundary is not a discourse reset — and not a forced position"
        );
        // The book's first word is forced with no glyph, whatever else is
        // pending — so `book_initial` is not derivable from the pending state.
        assert_eq!(
            reduce_one(&obs, &CasingBoundary::default()).0,
            Some(PosClass::BOOK_INITIAL)
        );
        assert_eq!(
            reduce_one(
                &obs,
                &CasingBoundary {
                    pending: Some(Pending {
                        mark: '.',
                        quote: false,
                        other: false
                    }),
                    book_initial: true,
                }
            )
            .0,
            Some(PosClass::BOOK_INITIAL)
        );
    }

    /// The chapter's own leading gap transforms the entering state exactly as
    /// the streaming machine would: a close quote promotes the class, any other
    /// intervening mark collapses it to mid-flow.
    #[test]
    fn the_leading_gap_transforms_the_entering_state() {
        let quoted = map_one("8", &["\u{201d} there he goes"]);
        assert_eq!(
            reduce_one(&quoted, &after_terminal('.')).0,
            Some(PosClass::forced(ClassKey {
                mark: '.',
                quoted: true,
            })),
        );
        let dotted = map_one("8", &[".. there he goes"]);
        assert_eq!(
            reduce_one(&dotted, &after_terminal('.')).0,
            Some(PosClass::MIDFLOW),
            "non-quote intervening punctuation collapses the boundary"
        );
    }

    /// The recorded transform is the exact one the streaming machine applies: a
    /// live pending is only ever added to (never replaced) by a gap, and a gap
    /// creates one only when nothing was pending.
    #[test]
    fn the_gap_transform_matches_the_streaming_machine() {
        let mut e = GapEffect::default();
        e.extend("x. ");
        assert_eq!(
            e.from_none,
            Some(Pending {
                mark: '.',
                quote: false,
                other: false
            }),
            "a letter then a mark creates the pending"
        );
        assert_eq!(
            e.apply(Some(Pending {
                mark: '!',
                quote: false,
                other: false
            })),
            Some(Pending {
                mark: '!',
                quote: false,
                other: true
            }),
            "an entering pending keeps its mark and takes the gap's flags"
        );
        let mut q = GapEffect::default();
        q.extend(" \u{201d} ");
        assert_eq!(q.from_none, None, "no letter precedes, so nothing is created");
        assert_eq!(
            q.apply(Some(Pending {
                mark: '.',
                quote: false,
                other: false
            })),
            Some(Pending {
                mark: '.',
                quote: true,
                other: false
            })
        );
        assert_eq!(GapEffect::default().apply(None), None);
    }

    /// A word-less chapter passes the entering state through — the empty /
    /// nonletter-verse case. A chapter WITH a word leaves a state that does not
    /// depend on what entered it, which is exactly why the ordered replay
    /// converges within one chapter of an edit.
    #[test]
    fn a_wordless_chapter_passes_state_through_and_a_worded_one_absorbs_it() {
        let empty = map_one("2", &["", "   ", "\u{2014}"]);
        for entering in [
            CasingBoundary::default(),
            after_terminal('.'),
            CasingBoundary {
                pending: None,
                book_initial: false,
            },
        ] {
            let (first, leaving) = reduce_one(&empty, &entering);
            assert!(first.is_none(), "a word-less chapter classifies nothing");
            assert_eq!(
                leaving.book_initial, entering.book_initial,
                "book-initial survives a word-less chapter"
            );
        }
        // The em-dash verse is non-quote punctuation, so it collapses a carried
        // terminal on its way through.
        assert_eq!(
            reduce_one(&empty, &after_terminal('.')).1.pending,
            Some(Pending {
                mark: '.',
                quote: false,
                other: true,
            })
        );

        let worded = map_one("2", &["he stops."]);
        let a = reduce_one(&worded, &after_terminal('!')).1;
        let b = reduce_one(&worded, &CasingBoundary::default()).1;
        assert_eq!(a, b, "a chapter with a word leaves its own state, not a carry");
    }

    /// End to end (plan §12.3): a terminal at the end of one chapter forces the
    /// position of the first word of the next, across a word-less chapter too.
    #[test]
    fn positional_carries_across_a_chapter_seam() {
        let bulk = lines(&["There we go there.", "There it is there."], 15);
        for filler in [None, Some(vec!["\u{2014}".to_string()])] {
            let mut chapters = vec![bulk.clone(), bulk.clone()];
            chapters.push(vec!["he stops.".to_string()]);
            if let Some(f) = filler.clone() {
                chapters.push(f);
            }
            chapters.push(vec!["there he goes".to_string()]);
            let vm = chaptered("GEN", &chapters);
            let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
            let want = format!("GEN {}:1", chapters.len());
            let hit = f
                .iter()
                .any(|f| vm.key(f.key_idx) == want && slice(&vm, f) == "there");
            // A word-less chapter carries non-quote punctuation, which collapses
            // the boundary to mid-flow — the same answer the streaming walk gives
            // for an em-dash verse mid-chapter.
            assert_eq!(
                hit,
                filler.is_none(),
                "chapters={}, filler={:?}",
                chapters.len(),
                filler
            );
        }
    }

    /// A chapter edit maps exactly its own chapter and the replay converges at
    /// it or at its successor — casing's leaving state is its own trailing
    /// terminal context, so the carry resolves locally.
    #[test]
    fn an_edit_maps_one_chapter_and_converges_locally() {
        let bulk = lines(&["There we go there.", "There it is there."], 8);
        let mut chapters: Vec<Vec<String>> = (0..10).map(|_| bulk.clone()).collect();
        // A forced-lowercase slip in chapter 8, so the fixture has findings to
        // move: the previous verse's `.` forces this verse's first word.
        chapters[7].push("there he goes".to_string());
        let vm = chaptered("GEN", &chapters);
        let c = cfg(0.5, 32.0, 0.0);
        let mut cache = SubstrateCache::new();
        let mut retained = None;
        let symbols = WordInterner::default();
        let cold = resident(&mut cache, &mut retained, &symbols, &vm, &c);
        assert_eq!(cache.mapped, 10, "cold maps every chapter");
        assert_eq!(cache.reduced, 10);

        let mut edited = chapters.clone();
        edited[5][0] = "There we went there.".to_string();
        let ev = chaptered("GEN", &edited);
        let inc = resident(&mut cache, &mut retained, &symbols, &ev, &c);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert!(
            cache.reduced <= 2,
            "the changed chapter leaves its own trailing context, so the replay \
             converges at it or its successor; reduced={}",
            cache.reduced
        );
        assert!(!cold.is_empty(), "the fixture must have findings to preserve");
        assert_eq!(render(&ev, &inc), render(&ev, &run_both(&ev, &c)));

        // Unchanged re-drive: no map, no reduce, and the model is reused.
        let before = retained.as_ref().map(|m| m.generation);
        let again = resident(&mut cache, &mut retained, &symbols, &ev, &c);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert_eq!(retained.as_ref().map(|m| m.generation), before);
        assert_eq!(render(&ev, &again), render(&ev, &inc));
    }

    /// Both consumers' findings for a corpus, cold.
    fn run_both(corpus: &Corpus, cfg: &CasingConfig) -> Vec<Finding> {
        let mut out = casing_findings(corpus, cfg, true, true);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));
        out
    }

    /// A judging-knob change maps and reduces nothing: no `CasingConfig` field
    /// is an extraction input, so every observation and reduction stays valid.
    #[test]
    fn a_knob_change_maps_and_reduces_nothing() {
        let bulk = lines(&["There we go there.", "There it is there."], 8);
        let chapters: Vec<Vec<String>> = (0..4).map(|_| bulk.clone()).collect();
        let vm = chaptered("GEN", &chapters);
        let mut cache = SubstrateCache::new();
        let mut retained = None;
        let symbols = WordInterner::default();
        let _ = resident(&mut cache, &mut retained, &symbols, &vm, &cfg(0.5, 32.0, 0.0));

        let loose = cfg(0.0, 32.0, 0.0);
        let after = resident(&mut cache, &mut retained, &symbols, &vm, &loose);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert!(cache.judged >= 1, "the knob change re-judges");
        assert_eq!(render(&vm, &after), render(&vm, &run_both(&vm, &loose)));
    }

    /// Disabling one consumer leaves the shared substrate — and the other
    /// consumer — untouched (plan §12.4).
    #[test]
    fn either_consumer_may_be_disabled_without_dropping_the_substrate() {
        let vm = {
            let base = cycle("GEN", &["There we go there."], 30);
            push_verse(base, "GEN", 200, "there we go there")
        };
        let c = cfg(0.5, 32.0, 0.0);
        let mut cache = SubstrateCache::new();
        let mut retained = None;
        let mut both = Vec::new();
        let symbols = WordInterner::default();
        drive_casing(
        true,
        true,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        &vm,
        &c,
        &mut both,
    );
        assert!(
            both.iter().any(|f| f.code == SENTENCE_INITIAL_LOWERCASE),
            "{both:?}"
        );

        // Positional only: the substrate is reused (nothing re-mapped) and the
        // intrinsic consumer's findings are simply not materialized.
        let mut only_pos = Vec::new();
        drive_casing(
        true,
        false,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        &vm,
        &c,
        &mut only_pos,
    );
        assert_eq!(cache.mapped, 0, "a consumer toggle re-maps nothing");
        assert!(only_pos.iter().all(|f| f.code == SENTENCE_INITIAL_LOWERCASE));
        assert_eq!(
            only_pos.len(),
            both.iter()
                .filter(|f| f.code == SENTENCE_INITIAL_LOWERCASE)
                .count()
        );

        // Both off: the last consumer leaving drops the substrate's products.
        let mut off = Vec::new();
        drive_casing(
        false,
        false,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        &vm,
        &c,
        &mut off,
    );
        assert!(off.is_empty());
        assert!(retained.is_none(), "the model memo goes with the substrate");
        let mut back = Vec::new();
        drive_casing(
        true,
        true,
        CasingState {
            cache: &mut cache,
            retained: &mut retained,
            symbols: &symbols,
        },
        &vm,
        &c,
        &mut back,
    );
        assert!(cache.mapped > 0, "re-enabling rebuilds the substrate");
        assert_eq!(render(&vm, &back), render(&vm, &both));
    }

    /// Property test (plan §12.6 shape): a resident cache driven through a
    /// pseudo-random edit sequence over a multi-chapter, multi-book corpus equals
    /// a cold whole-corpus run at every step, and never maps more than the
    /// chapter that changed.
    #[test]
    fn resident_casing_equals_cold_under_randomized_edits() {
        // Shapes that move the pending terminal at chapter edges: a trailing
        // terminal, a trailing quote-terminal, a bare word, an empty verse, and
        // a lowercase opener that only flags when something forces it.
        let shapes = [
            "There we go there.",
            "There it is there.\u{201d}",
            "there he goes",
            "",
            "he stops.",
            "\u{2014}",
            "There we go there",
        ];
        let mut layout: Vec<(&str, usize, usize)> = Vec::new(); // (slug, chapter, verses)
        for slug in ["GEN", "EXO"] {
            for ch in 1..=4 {
                layout.push((slug, ch, 6));
            }
        }
        let mut texts: Vec<usize> = (0..layout.len() * 6).map(|i| i % shapes.len()).collect();
        let build = |texts: &[usize]| {
            let mut keys = Vec::new();
            let mut out = Vec::new();
            let mut i = 0;
            for &(slug, ch, n) in &layout {
                for v in 1..=n {
                    keys.push(format!("{slug} {ch}:{v}"));
                    out.push(shapes[texts[i]].to_string());
                    i += 1;
                }
            }
            Corpus::try_from_parts(keys, out).unwrap()
        };
        let c = cfg(0.0, 32.0, 0.0);
        let mut cache = SubstrateCache::new();
        let mut retained = None;
        let symbols = WordInterner::default();
        let _ = resident(&mut cache, &mut retained, &symbols, &build(&texts), &c);
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        let next = |rng: &mut u64| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            *rng
        };
        for step in 0..100 {
            let which = (next(&mut rng) % texts.len() as u64) as usize;
            texts[which] = (next(&mut rng) % shapes.len() as u64) as usize;
            let corpus = build(&texts);
            let inc = resident(&mut cache, &mut retained, &symbols, &corpus, &c);
            assert!(
                cache.mapped <= 1,
                "step {step}: one edited verse maps at most one chapter"
            );
            assert_eq!(
                render(&corpus, &inc),
                render(&corpus, &run_both(&corpus, &c)),
                "step {step}: resident differs from cold"
            );
        }
    }
}
