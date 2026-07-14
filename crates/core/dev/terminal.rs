//! `terminal_strength` SPIKE — DEV TOOLING ONLY, nothing ships (shortlist 2/3).
//!
//! Per-mark boundary validation, two witnesses combined noisy-OR:
//! `trust(c) = 1 − (1 − s_case)(1 − s_reshuffle)`. The trust is then wired into
//! the ADR 0051 casing scoring (harness-side reimplementation) to measure
//! whether mark-trust changes fleet verdicts. All modelling lives here; core is
//! untouched. Knobs are NOT frozen — this is a measurement harness.
//!
//! ## Walk
//!
//! A faithful re-derivation of the ADR 0051 casing walk (`compound_words` +
//! the pending-terminal machine, carried across verse seams), extended so each
//! forced position records its **class**: the candidate terminal glyph plus a
//! context bit — whether a quote glyph intervened between the mark and the next
//! word (`."`, `said: "`). The shipped walk collapses any intervening
//! punctuation to mid-flow; the split lets the spike ask whether
//! terminal+quote contexts earn trust (shortlist item 7).
//!
//! ## Witnesses (per class)
//!
//! - **W1 case-follow** (bicameral only): Wilson-shrunk capitalize rate of
//!   lexicon-lowercase words immediately after the class. This is exactly the
//!   ADR 0051 per-glyph habit dominance, re-derived over the class's forced
//!   pool. Absent (caseless, or no lexicon-lowercase followers) ⇒ 0.
//! - **W2 word-reshuffle** (case-free): does the following-word distribution
//!   differ from the corpus baseline, over Zipf-gated jurors (word-starts seen
//!   ≥ 10×), via the ported `association::Table2` (G² / Fisher). Two variants:
//!   - **A (plain differentness)**: standardized multinomial-G² deviate of the
//!     aftermath vs the corpus word-start baseline, through a fleet-refit
//!     sigmoid. High for real terminals AND for genealogy list-commas — the
//!     guard failure the spec names.
//!   - **B (guarded)**: A × cross-mark **signature agreement** (cosine of the
//!     per-juror enrichment vector against the corpus's most case-trusted
//!     class). A list separator enriches its own vocabulary, not the
//!     sentence-start signature, so agreement pulls it down.
//!
//!   A before/after **asymmetry** deviate is computed and reported alongside.
//!
//! ## Wiring
//!
//! Casing is scored twice over the same observations: **baseline** (trust ≡ 1,
//! quote-context forced positions kept mid-flow — reproduces shipped ADR 0051)
//! and **trust-wired** (positional score `×= trust(class)`; censoring discount
//! `1 − trust(class)·habit(class)`; quote-context sites promoted to forced when
//! their class is trusted). Deltas at the frozen knobs (floor 0.95, k = 32) are
//! the payload.

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};

use ssc_core::charclass::class_of;
use ssc_core::token::tokenize;
use ssc_core::verse::{VerseMap, by_book};
use ssc_core::{Sid, Span};

use ssc_core::analysis::association::Table2;

// ── Knobs (spike; not frozen) ───────────────────────────────────────────────

/// Casing frozen knobs (ADR 0051) — held fixed so deltas isolate trust.
pub const FLOOR: f64 = 0.95;
pub const K: f64 = 32.0;
pub const Z: f64 = 1.96;
/// Zipf gate: a word must appear as a word-start at least this often to be a
/// reshuffle juror. Shortlist: "words seen ≥ 10×".
const JUROR_MIN: u64 = 10;
/// Candidate-class gate: a class needs at least this many boundary events to be
/// validated. Below it, both witnesses are too thin; the class is dropped (and
/// counted — no silent cap).
const CLASS_MIN_EVENTS: u64 = 30;
/// W2 sigmoid refit (see `doc`): `s = logistic((zG2 − THR)/SCALE)` on the
/// standardized multinomial-G² deviate. Refit on the fleet; labs' scale-30 on a
/// raw G² does not transfer to the standardized statistic.
const W2_SIGMOID_THR: f64 = 8.0;
const W2_SIGMOID_SCALE: f64 = 6.0;
/// Quote-context promotion bar — a `Quote(mark)` slot is promoted to a forced
/// position when its class trust clears this. Held **fixed across wirings** so
/// the promoted-site *population* (the 237) is identical for the multiplier and
/// every gate threshold; the gate then decides how many *survive*.
const PROMOTE_BAR: f64 = 0.5;
/// Gate-threshold sweep (ADR 0051-follow-on; 2026-07-10). Under the gate wiring
/// a forced site's positional score is the unchanged two-factor `habit × rarity`
/// iff `trust(class) ≥ T`, and the site is not scored positionally below it.
pub const GATE_SWEEP: &[f64] = &[0.50, 0.60, 0.70, 0.80, 0.90, 0.95];

// ── Walk ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    Upper,
    Lower,
    Uncased,
}

/// Where a word-start sits, with the terminal's context preserved. `Ambiguous`
/// (non-quote intervening punctuation) collapses to mid-flow exactly as the
/// shipped walk does; `Quote` is the split the shipped walk cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Slot {
    Mid,
    BookInit,
    /// Bare attached terminal (no intervening glyph). `char` is the mark.
    Bare(char),
    /// Terminal then a quote glyph before the next word (`."`, `: "`).
    Quote(char),
}

/// A boundary class as validated by the witnesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassKey {
    pub mark: char,
    pub quoted: bool,
}

impl ClassKey {
    fn of(slot: Slot) -> Option<ClassKey> {
        match slot {
            Slot::Bare(m) => Some(ClassKey {
                mark: m,
                quoted: false,
            }),
            Slot::Quote(m) => Some(ClassKey {
                mark: m,
                quoted: true,
            }),
            _ => None,
        }
    }
    pub fn label(&self) -> String {
        if self.quoted {
            format!("{:?}+\"", self.mark)
        } else {
            format!("{:?}", self.mark)
        }
    }
}

/// One lowercase word-start: a scoring candidate.
struct Site {
    sid: Sid,
    span: Span,
    key: String,
    slot: Slot,
}

/// Per-word case tallies keyed by slot (raw, corpus-wide — book supersede is
/// irrelevant to a single-pass spike).
#[derive(Default)]
struct WordObs {
    slots: HashMap<Slot, (u32, u32)>, // (upper, lower)
}

impl WordObs {
    fn add(&mut self, slot: Slot, case: Case) {
        let e = self.slots.entry(slot).or_default();
        match case {
            Case::Upper => e.0 += 1,
            Case::Lower => e.1 += 1,
            Case::Uncased => {}
        }
    }
}

/// Everything one corpus walk produces.
struct Walk {
    words: HashMap<String, WordObs>,
    sites: Vec<Site>,
    /// juror frequency and baseline word-start distribution.
    word_start_total: HashMap<String, u64>,
    n_word_starts: u64,
    /// per class: following-word counts and total.
    after: HashMap<ClassKey, HashMap<String, u64>>,
    /// per class: preceding-word counts (for before/after asymmetry).
    before: HashMap<ClassKey, HashMap<String, u64>>,
    cased_starts: u64,
    dropped_classes: u64,
}

fn is_letter(c: char) -> bool {
    class_of(c).is_alphabetic()
}

/// ADR 0051 `compound_words`, copied: UAX #29 tokens, then letter-flanked
/// single-hyphen joins, then drop letter-free tokens.
fn compound_words(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for t in tokenize(text) {
        if let Some(prev) = out.last_mut() {
            let gap = &text[prev.end as usize..t.span.start as usize];
            let mut g = gap.chars();
            let hyphen = matches!(g.next(), Some('\u{002D}' | '\u{2010}')) && g.next().is_none();
            if hyphen
                && text[..prev.end as usize].chars().next_back().is_some_and(is_letter)
                && text[t.span.start as usize..].chars().next().is_some_and(is_letter)
            {
                prev.end = t.span.end;
                continue;
            }
        }
        out.push(t.span);
    }
    out.retain(|s| text[s.start as usize..s.end as usize].chars().any(is_letter));
    out
}

#[derive(Clone)]
struct Pending {
    mark: char,
    quote: bool,
    other: bool,
    prev: Option<String>,
}

fn walk_corpus(map: &VerseMap) -> Walk {
    let mut w = Walk {
        words: HashMap::new(),
        sites: Vec::new(),
        word_start_total: HashMap::new(),
        n_word_starts: 0,
        after: HashMap::new(),
        before: HashMap::new(),
        cased_starts: 0,
        dropped_classes: 0,
    };
    let books = by_book(map);
    for verses in books.values() {
        let mut pending: Option<Pending> = None;
        let mut book_initial = true;
        let mut last_word: Option<String> = None;

        for (sid, text) in verses {
            let words = compound_words(text);
            let mut prev_letter = false;
            let mut cursor = 0usize;

            for span in &words {
                // Advance the gap, tracking the candidate terminal + context.
                for c in text[cursor..span.start as usize].chars() {
                    let cl = class_of(c);
                    if cl.is_whitespace() || cl.is_numeric() {
                        prev_letter = false;
                    } else if cl.is_alphabetic() {
                        prev_letter = true;
                    } else {
                        match &mut pending {
                            Some(p) => {
                                if cl.is_quote() {
                                    p.quote = true;
                                } else {
                                    p.other = true;
                                }
                            }
                            None if prev_letter => {
                                pending = Some(Pending {
                                    mark: c,
                                    quote: false,
                                    other: false,
                                    prev: last_word.clone(),
                                });
                            }
                            None => {}
                        }
                        prev_letter = false;
                    }
                }

                let first = text[span.start as usize..span.end as usize].chars().next().unwrap();
                let fcl = class_of(first);
                let case = if fcl.is_uppercase() {
                    Case::Upper
                } else if fcl.is_lowercase() {
                    Case::Lower
                } else {
                    Case::Uncased
                };

                let taken = pending.take();
                let slot = if book_initial {
                    Slot::BookInit
                } else if let Some(p) = &taken {
                    if p.other {
                        Slot::Mid // ambiguous — matches shipped midflow collapse
                    } else if p.quote {
                        Slot::Quote(p.mark)
                    } else {
                        Slot::Bare(p.mark)
                    }
                } else {
                    Slot::Mid
                };
                book_initial = false;

                let key = text[span.start as usize..span.end as usize].to_lowercase();
                if case != Case::Uncased {
                    w.cased_starts += 1;
                }
                w.words.entry(key.clone()).or_default().add(slot, case);
                *w.word_start_total.entry(key.clone()).or_default() += 1;
                w.n_word_starts += 1;

                // Reshuffle event: record following/preceding word by class.
                if let Some(cls) = ClassKey::of(slot) {
                    *w.after
                        .entry(cls)
                        .or_default()
                        .entry(key.clone())
                        .or_default() += 1;
                    if let Some(prev) = taken.as_ref().and_then(|p| p.prev.clone()) {
                        *w.before.entry(cls).or_default().entry(prev).or_default() += 1;
                    }
                }

                if case == Case::Lower {
                    w.sites.push(Site {
                        sid: *sid,
                        span: *span,
                        key: key.clone(),
                        slot,
                    });
                }

                last_word = Some(key);
                prev_letter = text[span.start as usize..span.end as usize]
                    .chars()
                    .next_back()
                    .is_some_and(is_letter);
                cursor = span.end as usize;
            }
            // Trailing gap of the verse (pending carries across the seam).
            for c in text[cursor..].chars() {
                let cl = class_of(c);
                if cl.is_whitespace() || cl.is_numeric() {
                    prev_letter = false;
                } else if cl.is_alphabetic() {
                    prev_letter = true;
                } else {
                    match &mut pending {
                        Some(p) => {
                            if cl.is_quote() {
                                p.quote = true;
                            } else {
                                p.other = true;
                            }
                        }
                        None if prev_letter => {
                            pending = Some(Pending {
                                mark: c,
                                quote: false,
                                other: false,
                                prev: last_word.clone(),
                            });
                        }
                        None => {}
                    }
                    prev_letter = false;
                }
            }
        }
    }
    w
}

// ── Wilson / rarity (copied from evidence.rs + casing.rs) ────────────────────

fn wilson_lb(k: f64, n: f64, z: f64) -> f64 {
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

fn rarity(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

fn logistic(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

// ── Witnesses ────────────────────────────────────────────────────────────

/// Per-class witness measurements and the combined trust.
#[derive(Clone)]
pub struct ClassTrust {
    pub class: ClassKey,
    pub events: u64,
    pub s_case: f64, // W1 (0 if absent)
    pub s_case_seen: bool,
    pub diff: f64,  // W2 variant A (plain differentness)
    pub agree: f64, // cross-mark signature agreement
    pub asym: f64,  // before/after asymmetry deviate → [0,1]
    pub s_reshuffle_a: f64,
    pub s_reshuffle_b: f64,
    pub jurors: u64,
    pub g2_after: f64,
    pub df: u64,
    /// trust from variant A and variant B respectively.
    pub trust_a: f64,
    pub trust_b: f64,
}

/// Aggregate per-juror 2×2 association (the ported `Table2` — G² fast path,
/// Fisher on sparse jurors) of the aftermath vs the corpus baseline, then
/// standardize: under the null each juror's statistic is ~χ²₁ (mean 1), so the
/// sum is ~χ²_df and `(Σ − df)/sqrt(2·df)` is comparable across corpus sizes.
/// `base` is the corpus word-start distribution and **includes** the after-c
/// occurrences, so the "elsewhere" column subtracts them out.
/// Returns (deviate, Σ association, df).
fn reshuffle_deviate(
    after: &HashMap<String, u64>,
    base: &HashMap<String, u64>,
    jurors: &[String],
) -> (f64, f64, u64) {
    let n_after: u64 = jurors
        .iter()
        .map(|w| after.get(w).copied().unwrap_or(0))
        .sum();
    let n_base: u64 = jurors
        .iter()
        .map(|w| base.get(w).copied().unwrap_or(0))
        .sum();
    if n_after == 0 || n_base <= n_after {
        return (0.0, 0.0, 0);
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
        let c = total_w.saturating_sub(a); // w elsewhere
        let d = n_else.saturating_sub(c);
        sum += Table2::new(a, b, c, d).association_score();
    }
    let df = df.max(1);
    let dev = (sum - df as f64) / (2.0 * df as f64).sqrt();
    (dev, sum, df)
}

/// Same standardized aggregate, but comparing two disjoint populations
/// (`after` vs `before` juror counts) — the before/after asymmetry witness.
fn two_pop_deviate(
    after: &HashMap<String, u64>,
    before: &HashMap<String, u64>,
    jurors: &[String],
) -> f64 {
    let n_a: u64 = jurors
        .iter()
        .map(|w| after.get(w).copied().unwrap_or(0))
        .sum();
    let n_b: u64 = jurors
        .iter()
        .map(|w| before.get(w).copied().unwrap_or(0))
        .sum();
    if n_a == 0 || n_b == 0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut df = 0u64;
    for w in jurors {
        let a = after.get(w).copied().unwrap_or(0);
        let c = before.get(w).copied().unwrap_or(0);
        if a + c == 0 {
            continue;
        }
        df += 1;
        sum += Table2::new(a, n_a - a, c, n_b - c).association_score();
    }
    let df = df.max(1);
    (sum - df as f64) / (2.0 * df as f64).sqrt()
}

/// The per-class trust table for a corpus. `jurors` are word-starts ≥ JUROR_MIN.
pub struct TrustTable {
    pub classes: BTreeMap<ClassKey, ClassTrust>,
    pub reference: Option<ClassKey>,
    pub n_jurors: u64,
    pub dropped_classes: u64,
}

fn build_trust(w: &Walk, lex_lower: &HashMap<String, bool>) -> TrustTable {
    let jurors: Vec<String> = w
        .word_start_total
        .iter()
        .filter(|&(_, &n)| n >= JUROR_MIN)
        .map(|(k, _)| k.clone())
        .collect();

    // Candidate classes gated by event count.
    let mut dropped = 0u64;
    let mut kept: Vec<ClassKey> = Vec::new();
    for (cls, counts) in &w.after {
        let n: u64 = counts.values().sum();
        if n >= CLASS_MIN_EVENTS {
            kept.push(*cls);
        } else {
            dropped += 1;
        }
    }

    // W1 per class: capitalize dominance over lexicon-lowercase followers —
    // exactly ADR 0051's per-glyph habit, re-derived over the class's pool.
    let mut w1: HashMap<ClassKey, (u64, u64)> = HashMap::new(); // (upper, total)
    for (key, obs) in &w.words {
        if !lex_lower.get(key).copied().unwrap_or(false) {
            continue;
        }
        for (slot, &(up, lo)) in &obs.slots {
            if let Some(cls) = ClassKey::of(*slot) {
                let e = w1.entry(cls).or_default();
                e.0 += up as u64;
                e.1 += (up + lo) as u64;
            }
        }
    }

    // Reshuffle differentness + asymmetry per class.
    let mut prelim: HashMap<ClassKey, ClassTrust> = HashMap::new();
    for &cls in &kept {
        let after = &w.after[&cls];
        let (dev, g2, df) = reshuffle_deviate(after, &w.word_start_total, &jurors);
        let diff = logistic((dev - W2_SIGMOID_THR) / W2_SIGMOID_SCALE);
        // before/after asymmetry
        let empty = HashMap::new();
        let before = w.before.get(&cls).unwrap_or(&empty);
        let adev = two_pop_deviate(after, before, &jurors);
        let asym = logistic((adev - W2_SIGMOID_THR) / W2_SIGMOID_SCALE);
        let (s_case, seen) = match w1.get(&cls) {
            Some(&(up, total)) if total > 0 => (wilson_lb(up as f64, total as f64, Z), true),
            _ => (0.0, false),
        };
        let events: u64 = after.values().sum();
        prelim.insert(
            cls,
            ClassTrust {
                class: cls,
                events,
                s_case,
                s_case_seen: seen,
                diff,
                agree: 0.0,
                asym,
                s_reshuffle_a: diff,
                s_reshuffle_b: diff,
                jurors: jurors.len() as u64,
                g2_after: g2,
                df,
                trust_a: 0.0,
                trust_b: 0.0,
            },
        );
    }

    // Reference class for agreement: the canonical terminal — the highest
    // case-trusted BARE class (ties by event count). Preferring bare over a
    // quote-context refinement keeps the reference the plain sentence
    // terminator, so bare '.' does not score low agreement against '."' and
    // erode its own (trust ≈ 1). Caseless fallback: highest-differentness bare
    // class, else any highest-differentness class.
    let pick = |pred: &dyn Fn(&&ClassTrust) -> bool, key: &dyn Fn(&ClassTrust) -> f64| {
        prelim
            .values()
            .filter(|c| pred(c))
            .max_by(|a, b| key(a).partial_cmp(&key(b)).unwrap())
            .map(|c| c.class)
    };
    // The canonical terminal is the highest-VOLUME strongly case-trusted bare
    // class (`.` beats a thin-margin `?`), so the reference is the dominant
    // sentence terminator and doesn't erode itself. Fallbacks for caseless
    // corpora: the highest-differentness bare class, then any class.
    let reference = pick(
        &|c| c.s_case_seen && !c.class.quoted && c.s_case >= 0.5,
        &|c| c.events as f64,
    )
    .or_else(|| pick(&|c| !c.class.quoted, &|c| c.diff))
    .or_else(|| pick(&|_| true, &|c| c.diff));

    // Signature agreement: 1 − total-variation distance between this class's
    // aftermath and the reference terminal's aftermath (a size-robust effect
    // size, unlike the significance deviates). A real terminal resets to the
    // same sentence-start distribution as '.'; a list separator's aftermath is
    // its own (conjunctions, names), so it diverges and agreement drops — the
    // genealogy guard that plain differentness cannot supply.
    // Normalize against how period-like a *random* word-start is: a mark earns
    // agreement only for being closer to the reference aftermath than the
    // baseline draw. `agree = 1 − TV(after_c, ref) / TV(baseline, ref)`.
    let ref_after = reference.map(|r| w.after[&r].clone());
    let ref_base_tv = ref_after
        .as_ref()
        .map(|ra| tv_distance(&w.word_start_total, ra, &jurors).max(1e-6));
    for (cls, ct) in prelim.iter_mut() {
        if Some(*cls) == reference {
            ct.agree = 1.0;
        } else if let (Some(ra), Some(rbt)) = (&ref_after, ref_base_tv) {
            let tv = tv_distance(&w.after[cls], ra, &jurors);
            ct.agree = (1.0 - tv / rbt).clamp(0.0, 1.0);
        }
        ct.s_reshuffle_a = ct.diff;
        ct.s_reshuffle_b = ct.diff * ct.agree;
        ct.trust_a = 1.0 - (1.0 - ct.s_case) * (1.0 - ct.s_reshuffle_a);
        ct.trust_b = 1.0 - (1.0 - ct.s_case) * (1.0 - ct.s_reshuffle_b);
    }

    TrustTable {
        classes: prelim.into_iter().collect(),
        reference,
        n_jurors: jurors.len() as u64,
        dropped_classes: dropped,
    }
}

/// Total-variation distance `½·Σ|p_w − q_w|` between two juror distributions.
/// Size-independent effect size in `[0, 1]`; 0 iff the distributions match.
fn tv_distance(p: &HashMap<String, u64>, q: &HashMap<String, u64>, jurors: &[String]) -> f64 {
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

// ── Casing model with trust wiring ──────────────────────────────────────────

/// How trust enters the positional score.
#[derive(Clone, Copy, PartialEq)]
pub enum Wiring {
    /// Spike wiring: positional score `×= trust(class)` (continuous haircut).
    /// Quadrant presence is trust-independent. Reproduced by `Baseline` at
    /// trust ≡ 1.
    Multiplier,
    /// Gate wiring: the positional score is the **unchanged** `habit × rarity`
    /// iff `trust(class) ≥ T`; below `T` the site is not scored positionally
    /// (no presence, no score). The censoring discount is untouched — it keeps
    /// the multiplicative `trust × habit` form in both wirings.
    Gate(f64),
}

/// Which trust to apply, whether quote-context sites may be promoted to forced,
/// and how trust enters the positional channel. `Baseline` reproduces shipped
/// ADR 0051 (trust ≡ 1, quote-context kept mid-flow).
#[derive(Clone, Copy)]
pub enum Scenario<'a> {
    Baseline,
    Trust {
        trust: &'a HashMap<ClassKey, f64>,
        promote_quote: bool,
        wiring: Wiring,
    },
}

/// Aggregated per-word tallies under a scenario's slot→position mapping.
#[derive(Default, Clone)]
struct WStats {
    mid_upper: u64,
    mid_lower: u64,
    // forced tallies keyed by the scenario's habit class (None = book-initial).
    forced: BTreeMap<Option<ClassKey>, (u64, u64)>, // (upper, lower)
}

impl WStats {
    fn forced_lower(&self) -> u64 {
        self.forced.values().map(|t| t.1).sum()
    }
    fn forced_total(&self) -> u64 {
        self.forced.values().map(|t| t.0 + t.1).sum()
    }
    fn mid_total(&self) -> u64 {
        self.mid_upper + self.mid_lower
    }
    fn is_lex_lower(&self) -> bool {
        self.mid_total() > 0 && wilson_lb(self.mid_lower as f64, self.mid_total() as f64, Z) > 0.5
    }
}

/// Map a raw slot to a scenario position: `None` = mid-flow, `Some(None)` =
/// book-initial, `Some(Some(class))` = forced under that habit class.
fn slot_position(
    slot: Slot,
    sc: Scenario,
    trust: &dyn Fn(ClassKey) -> f64,
) -> Option<Option<ClassKey>> {
    match slot {
        Slot::Mid => None,
        Slot::BookInit => Some(None),
        Slot::Bare(m) => Some(Some(ClassKey {
            mark: m,
            quoted: false,
        })),
        Slot::Quote(m) => {
            let cls = ClassKey {
                mark: m,
                quoted: true,
            };
            match sc {
                Scenario::Baseline => None, // shipped: quote-context is mid-flow
                Scenario::Trust { promote_quote, .. } => {
                    // Promote only when the class is meaningfully trusted. The
                    // bar is wiring-independent, so the promoted population is
                    // shared by the multiplier and every gate threshold.
                    if promote_quote && trust(cls) > PROMOTE_BAR {
                        Some(Some(cls))
                    } else {
                        None
                    }
                }
            }
        }
    }
}

struct Model {
    words: HashMap<String, WStats>,
    /// per habit class: (upper, total) over lexicon-lowercase words.
    habit: HashMap<Option<ClassKey>, (u64, u64)>,
}

impl Model {
    /// Build the scenario model. `lex_lower` fixes the lexicon used to restrict
    /// the habit: pass `None` for the baseline (derive it internally), and the
    /// baseline lexicon for the trust scenario so promoting quote-context
    /// positions cannot perturb the *bare*-terminal habit or the lexicon — trust
    /// only rescales and adds the quote channel (a fresh habit key), it never
    /// moves the '.' convention off the floor.
    fn build(w: &Walk, sc: Scenario, lex_lower: Option<&HashMap<String, bool>>) -> Model {
        let trust_fn = |cls: ClassKey| match sc {
            Scenario::Baseline => 1.0,
            Scenario::Trust { trust, .. } => trust.get(&cls).copied().unwrap_or(0.0),
        };
        let mut words: HashMap<String, WStats> = HashMap::new();
        for (key, obs) in &w.words {
            let ws = words.entry(key.clone()).or_default();
            for (slot, &(up, lo)) in &obs.slots {
                match slot_position(*slot, sc, &trust_fn) {
                    None => {
                        ws.mid_upper += up as u64;
                        ws.mid_lower += lo as u64;
                    }
                    Some(cls) => {
                        let e = ws.forced.entry(cls).or_default();
                        e.0 += up as u64;
                        e.1 += lo as u64;
                    }
                }
            }
        }
        // Lexicon-restricted habit. The bare-terminal forced tallies are
        // identical across scenarios and the lexicon is fixed, so the bare
        // habit reproduces baseline exactly; only quote-class keys are new.
        let mut habit: HashMap<Option<ClassKey>, (u64, u64)> = HashMap::new();
        for (key, ws) in &words {
            let is_low = match lex_lower {
                Some(m) => m.get(key).copied().unwrap_or(false),
                None => ws.is_lex_lower(),
            };
            if !is_low {
                continue;
            }
            for (cls, &(up, lo)) in &ws.forced {
                let e = habit.entry(*cls).or_default();
                e.0 += up;
                e.1 += up + lo;
            }
        }
        Model { words, habit }
    }

    fn habit_dom(&self, cls: Option<ClassKey>) -> f64 {
        match self.habit.get(&cls) {
            Some(&(up, total)) => wilson_lb(up as f64, total as f64, Z),
            None => 0.0,
        }
    }

    /// Soft-censored effective upper, with the trust-scaled discount.
    fn effective_upper(&self, ws: &WStats, trust: &dyn Fn(Option<ClassKey>) -> f64) -> f64 {
        let mut up = ws.mid_upper as f64;
        for (cls, &(u, _)) in &ws.forced {
            if u > 0 {
                let discount = 1.0 - trust(*cls) * self.habit_dom(*cls);
                up += discount * u as f64;
            }
        }
        up
    }
}

// ── Scoring ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Quad {
    Intrinsic,
    Positional,
    Both,
}

pub struct Scored {
    pub sid: Sid,
    pub span: Span,
    pub quad: Quad,
    pub score: f64,
    pub intr_score: f64,
    pub pos_score: f64,
    pub class: Option<ClassKey>,
    // decomposition for review
    pub trust: f64,
    pub habit: f64,
    pub dominance: f64,
    pub minority: u64,
    pub rarity: f64,
    pub promoted_quote: bool,
}

/// Score every lowercase site under a scenario at the frozen knobs.
fn score(w: &Walk, sc: Scenario, lex: Option<&HashMap<String, bool>>) -> Vec<Scored> {
    let model = Model::build(w, sc, lex);
    score_with_model(w, &model, sc)
}

/// Score against a prebuilt model. The `Model` is wiring-independent (it depends
/// only on the trust map and the promotion bar), so the trust-scenario model can
/// be built once and reused across the multiplier and every gate threshold.
fn score_with_model(w: &Walk, model: &Model, sc: Scenario) -> Vec<Scored> {
    let trust_class = |cls: ClassKey| match sc {
        Scenario::Baseline => 1.0,
        Scenario::Trust { trust, .. } => trust.get(&cls).copied().unwrap_or(0.0),
    };
    let trust_pos = |cls: Option<ClassKey>| cls.map_or(1.0, trust_class);

    let mut out = Vec::new();
    for site in &w.sites {
        let Some(ws) = model.words.get(&site.key) else {
            continue;
        };
        let position = slot_position(site.slot, sc, &|c| trust_class(c));

        // Intrinsic channel (soft-censored cap dominance × rarity of lower).
        let eff_up = model.effective_upper(ws, &trust_pos);
        let n_intr = eff_up + ws.mid_lower as f64;
        let is_cap = n_intr > 0.0 && wilson_lb(eff_up, n_intr, Z) > 0.5;
        let mut intr_score = 0.0;
        let (mut i_dom, mut i_min) = (0.0, 0u64);
        if is_cap {
            let total_lower = ws.mid_lower + ws.forced_lower();
            let total = eff_up as u64 + total_lower; // opportunities ~ eff
            let _ = total;
            i_dom = wilson_lb(eff_up, eff_up + ws.mid_lower as f64, Z);
            i_min = ws.mid_lower + ws.forced_lower();
            intr_score = i_dom * rarity(i_min, K);
        }

        // Positional channel (forced sites only). "Present" == the site is a
        // classifiable forced position (ADR 0051's `positional` returns factors)
        // — this is what defines the quadrant, matching the shipped harness.
        let mut pos_score = 0.0;
        let mut pos_present = false;
        let (mut p_dom, mut p_min, mut p_trust, mut p_habit) = (0.0, 0u64, 1.0, 0.0);
        let mut promoted = false;
        if let Some(Some(cls)) = position {
            // Soft-censored lower dominance (ADR 0051 `is_lower_soft`), over the
            // same eff_up+mid_lower denominator as `is_cap`.
            let is_lower_soft = n_intr > 0.0 && wilson_lb(ws.mid_lower as f64, n_intr, Z) > 0.5;
            if is_cap || is_lower_soft {
                let habit = model.habit_dom(Some(cls));
                let t = trust_class(cls);
                let minority = ws.forced_lower();
                // A promoted quote-context site is tagged regardless of whether
                // the gate lets its positional channel fire — so the promoted
                // *population* is stable and survival is measurable per T.
                if matches!(site.slot, Slot::Quote(_)) {
                    promoted = true;
                }
                let wiring = match sc {
                    Scenario::Baseline => Wiring::Multiplier, // trust ≡ 1
                    Scenario::Trust { wiring, .. } => wiring,
                };
                let fire = match wiring {
                    // Continuous haircut: score ×= trust; presence unconditional.
                    Wiring::Multiplier => {
                        pos_score = habit * rarity(minority, K) * t;
                        true
                    }
                    // Gate: unchanged two-factor iff trusted, else not scored.
                    Wiring::Gate(thr) => {
                        if t >= thr {
                            pos_score = habit * rarity(minority, K);
                            true
                        } else {
                            false
                        }
                    }
                };
                if fire {
                    pos_present = true;
                    p_dom = habit;
                    p_min = minority;
                    p_trust = t;
                    p_habit = habit;
                }
            }
        }

        let surf = intr_score.max(pos_score);
        if surf < FLOOR {
            continue;
        }
        // Quadrant by which channels have factors present (shipped semantics),
        // not by which clears the floor.
        let quad = match (is_cap, pos_present) {
            (true, true) => Quad::Both,
            (true, false) => Quad::Intrinsic,
            (false, true) => Quad::Positional,
            (false, false) => {
                if intr_score >= pos_score {
                    Quad::Intrinsic
                } else {
                    Quad::Positional
                }
            }
        };
        let (trust, habit, dominance, minority, rar) = if pos_score >= intr_score {
            (p_trust, p_habit, p_dom, p_min, rarity(p_min, K))
        } else {
            (1.0, 0.0, i_dom, i_min, rarity(i_min, K))
        };
        out.push(Scored {
            sid: site.sid,
            span: site.span,
            quad,
            score: surf,
            intr_score,
            pos_score,
            class: position.flatten(),
            trust,
            habit,
            dominance,
            minority,
            rarity: rar,
            promoted_quote: promoted,
        });
    }
    out
}

// ── Corpus driver ────────────────────────────────────────────────────────

pub struct TermCorpus {
    pub id: String,
    pub verses: usize,
    pub bicameral: bool,
    pub trust: TrustTable,
    pub base_i: u64,
    pub base_p: u64,
    pub base_b: u64,
    pub tr_i: u64,
    pub tr_p: u64,
    pub tr_b: u64,
    /// sites promoted from quote-context that now surface.
    pub promoted_surfaced: u64,
    /// verdict changes (either direction): (sid, word, ctx, kind).
    pub changes: Vec<Change>,
    /// pool recovery: words that gain intrinsic-capitalized status under trust.
    pub pool_gained: u64,
    pub pool_lost: u64,
    pub intrinsic_flips: i64,
    pub samples_promoted: Vec<PromotedSample>,
    pub anchors: Vec<AnchorFate>,
    /// positional-channel change magnitude vs baseline (|Δ| births + deaths).
    pub pos_delta: i64,
    /// gate-threshold sweep (2026-07-10) — one entry per `GATE_SWEEP` value.
    pub gate: GateStats,
}

/// Per-corpus gate-threshold sweep aggregates. Every `Vec` is indexed by the
/// position in `GATE_SWEEP`; the `step_*` vectors are indexed by adjacent pair
/// (length `GATE_SWEEP.len() − 1`).
pub struct GateStats {
    /// surfaced (intrinsic, positional, both) per threshold.
    pub counts: Vec<(u64, u64, u64)>,
    /// promoted quote-context sites that survive (clear the floor) per threshold.
    pub promoted_survived: Vec<u64>,
    /// readmissions vs the multiplier wiring per threshold: sites the gate
    /// surfaces that the multiplier eroded below the floor.
    pub readmitted: Vec<u64>,
    /// does the corpus retain any positional/both coverage at each threshold?
    pub pos_alive: Vec<bool>,
    /// baseline positional coverage (base_p + base_b) — the coverage at stake.
    pub base_pos: u64,
    /// "middle population" per adjacent step: forced sites whose class trust lies
    /// in [T_i, T_{i+1}) — surfacing at T_i, gated off at T_{i+1}.
    pub step_lost: Vec<u64>,
    /// the classes (marks) involved in each step's middle population.
    pub step_classes: Vec<BTreeMap<ClassKey, u64>>,
    /// readmitted sites (from the lowest threshold, the maximal readmit set),
    /// capped, for major-corpus adjudication.
    pub readmit_samples: Vec<ReadmitSample>,
}

/// A finding the multiplier eroded below the floor that the gate readmits.
pub struct ReadmitSample {
    pub sid: String,
    pub word: String,
    pub ctx: String,
    pub class: String,
    /// class trust — the site is readmitted at every gate threshold T ≤ trust.
    pub trust: f64,
    /// gate (ungated) positional score `habit × rarity`.
    pub score: f64,
    /// baseline score (> floor if the multiplier truly eroded a shipped finding).
    pub base_score: f64,
}

pub struct Change {
    pub sid: String,
    pub word: String,
    pub ctx: String,
    pub base_score: f64,
    pub tr_score: f64,
    pub quad: &'static str,
    pub trust: f64,
    pub habit: f64,
    pub dominance: f64,
    pub minority: u64,
    pub rarity: f64,
    pub direction: &'static str, // "born" | "died"
}

pub struct PromotedSample {
    pub sid: String,
    pub word: String,
    pub ctx: String,
    pub class: String,
    pub trust: f64,
    pub score: f64,
}

/// The 12 ADR 0051 review anchors (same set the `--casing` harness tracks).
pub const ANCHORS: &[(&str, &str, &str)] = &[
    ("swhulb", "LUK 8:44", "yesu"),        // TP intrinsic
    ("WA-fr-ulb", "JHN 13:2", "jésus"),    // TP intrinsic
    ("spaRV1909", "1SA 7:8", "filisteos"), // TP intrinsic
    ("vie1934", "MAT 24:24", "christ"),    // TP intrinsic (min 2)
    ("eng-web", "3MA 6:9", "gentiles"),    // TP-ish intrinsic
    ("eng-kjv", "SIR 7:5", "justify"),     // TP positional (cross-seam)
    ("WA-en-ulb", "LAM 1:22", "deal"),     // TP positional (min 2)
    ("fraLSG", "ACT 19:13", "juifs"),      // FP intrinsic (French adjective)
    ("porblt", "MAT 24:24", "messias"),    // FP intrinsic (generic plural)
    ("deu1912", "PHM 1:9", "alter"),       // FP intrinsic (adj/noun homograph)
    ("ind", "DEU 14:12", "rajawali"),      // FP positional (list colon)
    ("nld", "GEN 6:19", "mannetje"),       // FP positional (list colon)
];

pub struct AnchorFate {
    pub corpus: String,
    pub sid: String,
    pub word: String,
    pub base_score: f64,
    pub tr_score: f64,
    pub base_alive: bool,
    pub tr_alive: bool,
    pub quad: &'static str,
    pub class: String,
    pub trust: f64,
    pub habit: f64,
    /// alive under the gate wiring at each `GATE_SWEEP` threshold.
    pub gate_alive: Vec<bool>,
    /// gate score at each `GATE_SWEEP` threshold (0.0 when dead).
    pub gate_score: Vec<f64>,
}

fn quad_str(q: Quad) -> &'static str {
    match q {
        Quad::Intrinsic => "intrinsic",
        Quad::Positional => "positional",
        Quad::Both => "both",
    }
}

fn ctx(text: &str, span: Span) -> String {
    let span_start = span.start as usize;
    let span_end = span.end as usize;
    let start = text[..span_start]
        .char_indices()
        .rev()
        .nth(23)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let end = text[span_end..]
        .char_indices()
        .nth(24)
        .map(|(i, _)| span_end + i)
        .unwrap_or(text.len());
    text[start..end].replace(['\t', '\n'], " ")
}

pub fn analyze_corpus(id: String, map: &VerseMap, variant_b: bool) -> TermCorpus {
    let w = walk_corpus(map);
    let bicameral = w.cased_starts > 0;

    // Lexicon classification for W1 (own derivation; same definition as ADR
    // 0051's mid-flow lower dominance, over the baseline slot mapping). This is
    // the FIXED lexicon: reused for the trust scenario's habit so quote
    // promotion never moves the bare-terminal convention.
    let base_model = Model::build(&w, Scenario::Baseline, None);
    let lex_lower: HashMap<String, bool> = base_model
        .words
        .iter()
        .map(|(k, ws)| (k.clone(), ws.is_lex_lower()))
        .collect();

    let tt = build_trust(&w, &lex_lower);
    let trust_map: HashMap<ClassKey, f64> = tt
        .classes
        .iter()
        .map(|(k, c)| (*k, if variant_b { c.trust_b } else { c.trust_a }))
        .collect();

    let base = score(&w, Scenario::Baseline, None);
    // The trust-scenario model is wiring-independent, so build it once and reuse
    // it for the multiplier and every gate threshold.
    let trust_model = Model::build(
        &w,
        Scenario::Trust {
            trust: &trust_map,
            promote_quote: true,
            wiring: Wiring::Multiplier,
        },
        Some(&lex_lower),
    );
    let mult_sc = Scenario::Trust {
        trust: &trust_map,
        promote_quote: true,
        wiring: Wiring::Multiplier,
    };
    let wired = score_with_model(&w, &trust_model, mult_sc);

    let count = |v: &[Scored]| {
        let (mut i, mut p, mut b) = (0u64, 0u64, 0u64);
        for s in v {
            match s.quad {
                Quad::Intrinsic => i += 1,
                Quad::Positional => p += 1,
                Quad::Both => b += 1,
            }
        }
        (i, p, b)
    };
    let (base_i, base_p, base_b) = count(&base);
    let (tr_i, tr_p, tr_b) = count(&wired);

    // Verdict changes keyed by (sid, span).
    let base_set: HashMap<(Sid, u32, u32), &Scored> = base
        .iter()
        .map(|s| ((s.sid, s.span.start, s.span.end), s))
        .collect();
    let wired_set: HashMap<(Sid, u32, u32), &Scored> = wired
        .iter()
        .map(|s| ((s.sid, s.span.start, s.span.end), s))
        .collect();

    let mut changes = Vec::new();
    let mut promoted_surfaced = 0u64;
    let mut samples_promoted = Vec::new();
    for (k, sw) in &wired_set {
        if sw.promoted_quote {
            promoted_surfaced += 1;
            if samples_promoted.len() < 40 {
                let text = &map[&sw.sid];
                samples_promoted.push(PromotedSample {
                    sid: sw.sid.to_string(),
                    word: text[sw.span.start as usize..sw.span.end as usize].to_string(),
                    ctx: ctx(text, sw.span),
                    class: sw.class.map(|c| c.label()).unwrap_or_default(),
                    trust: sw.trust,
                    score: sw.score,
                });
            }
        }
        if !base_set.contains_key(k) {
            let text = &map[&sw.sid];
            changes.push(Change {
                sid: sw.sid.to_string(),
                word: text[sw.span.start as usize..sw.span.end as usize].to_string(),
                ctx: ctx(text, sw.span),
                base_score: base_set.get(k).map(|b| b.score).unwrap_or(0.0),
                tr_score: sw.score,
                quad: quad_str(sw.quad),
                trust: sw.trust,
                habit: sw.habit,
                dominance: sw.dominance,
                minority: sw.minority,
                rarity: sw.rarity,
                direction: "born",
            });
        }
    }
    for (k, sb) in &base_set {
        if !wired_set.contains_key(k) {
            let text = &map[&sb.sid];
            changes.push(Change {
                sid: sb.sid.to_string(),
                word: text[sb.span.start as usize..sb.span.end as usize].to_string(),
                ctx: ctx(text, sb.span),
                base_score: sb.score,
                tr_score: wired_set.get(k).map(|s| s.score).unwrap_or(0.0),
                quad: quad_str(sb.quad),
                trust: sb.trust,
                habit: sb.habit,
                dominance: sb.dominance,
                minority: sb.minority,
                rarity: sb.rarity,
                direction: "died",
            });
        }
    }

    // Pool recovery: intrinsic-capitalized classification per word under the
    // two censoring discounts.
    let trust_pos_base = |_c: Option<ClassKey>| 1.0;
    let trust_map2 = trust_map.clone();
    let trust_pos_wired =
        |c: Option<ClassKey>| c.map_or(1.0, |k| trust_map2.get(&k).copied().unwrap_or(0.0));
    let wired_model = &trust_model;
    let (mut gained, mut lost) = (0u64, 0u64);
    for (key, ws_b) in &base_model.words {
        let eff_b = base_model.effective_upper(ws_b, &trust_pos_base);
        let cap_b = (eff_b + ws_b.mid_lower as f64) > 0.0
            && wilson_lb(eff_b, eff_b + ws_b.mid_lower as f64, Z) > 0.5;
        let cap_w = wired_model
            .words
            .get(key)
            .map(|ws_w| {
                let eff_w = wired_model.effective_upper(ws_w, &trust_pos_wired);
                (eff_w + ws_w.mid_lower as f64) > 0.0
                    && wilson_lb(eff_w, eff_w + ws_w.mid_lower as f64, Z) > 0.5
            })
            .unwrap_or(cap_b);
        if cap_w && !cap_b {
            gained += 1;
        } else if cap_b && !cap_w {
            lost += 1;
        }
    }
    let intrinsic_flips = (tr_i + tr_b) as i64 - (base_i + base_b) as i64;
    let pos_delta = (tr_p as i64 - base_p as i64).abs();

    // ── Gate-threshold sweep (2026-07-10). ─────────────────────────────────
    // Re-score the trust scenario under the gate wiring at every threshold. The
    // model is reused; only the positional formula changes.
    let n_t = GATE_SWEEP.len();
    let gate_scored: Vec<Vec<Scored>> = GATE_SWEEP
        .iter()
        .map(|&thr| {
            let sc = Scenario::Trust {
                trust: &trust_map,
                promote_quote: true,
                wiring: Wiring::Gate(thr),
            };
            score_with_model(&w, &trust_model, sc)
        })
        .collect();

    let key_of = |s: &Scored| (s.sid, s.span.start, s.span.end);
    let mut g_counts = Vec::with_capacity(n_t);
    let mut g_promoted = Vec::with_capacity(n_t);
    let mut g_readmit = Vec::with_capacity(n_t);
    let mut g_pos_alive = Vec::with_capacity(n_t);
    for out in &gate_scored {
        let (gi, gp, gb) = count(out);
        g_counts.push((gi, gp, gb));
        g_promoted.push(out.iter().filter(|s| s.promoted_quote).count() as u64);
        // Readmissions: gate-surfaced sites the multiplier eroded below floor.
        let r = out
            .iter()
            .filter(|s| !wired_set.contains_key(&key_of(s)))
            .count() as u64;
        g_readmit.push(r);
        g_pos_alive.push(gp + gb > 0);
    }

    // Middle population per adjacent step: forced sites whose class trust lies in
    // [T_i, T_{i+1}) — surfacing (positionally) at T_i, gated off at T_{i+1}.
    let mut step_lost = Vec::with_capacity(n_t.saturating_sub(1));
    let mut step_classes: Vec<BTreeMap<ClassKey, u64>> = Vec::with_capacity(n_t.saturating_sub(1));
    for i in 0..n_t.saturating_sub(1) {
        let (lo, hi) = (GATE_SWEEP[i], GATE_SWEEP[i + 1]);
        let mut cnt = 0u64;
        let mut classes: BTreeMap<ClassKey, u64> = BTreeMap::new();
        for s in &gate_scored[i] {
            if let Some(cls) = s.class {
                let t = trust_map.get(&cls).copied().unwrap_or(0.0);
                if t >= lo && t < hi {
                    cnt += 1;
                    *classes.entry(cls).or_default() += 1;
                }
            }
        }
        step_lost.push(cnt);
        step_classes.push(classes);
    }

    // Readmit samples from the lowest threshold (the maximal readmit set).
    let mut readmit_samples = Vec::new();
    if let Some(out) = gate_scored.first() {
        for s in out {
            if wired_set.contains_key(&key_of(s)) {
                continue;
            }
            if readmit_samples.len() >= 40 {
                break;
            }
            let text = &map[&s.sid];
            let t = s
                .class
                .and_then(|c| trust_map.get(&c).copied())
                .unwrap_or(s.trust);
            readmit_samples.push(ReadmitSample {
                sid: s.sid.to_string(),
                word: text[s.span.start as usize..s.span.end as usize].to_string(),
                ctx: ctx(text, s.span),
                class: s.class.map(|c| c.label()).unwrap_or_default(),
                trust: t,
                score: s.score,
                base_score: base_set.get(&key_of(s)).map(|b| b.score).unwrap_or(0.0),
            });
        }
    }

    let gate = GateStats {
        counts: g_counts,
        promoted_survived: g_promoted,
        readmitted: g_readmit,
        pos_alive: g_pos_alive,
        base_pos: base_p + base_b,
        step_lost,
        step_classes,
        readmit_samples,
    };

    // Anchor fates for anchors belonging to this corpus. An anchor "alive" iff
    // some scored site in that verse slices to the anchor word.
    let mut anchors = Vec::new();
    for &(ac, asid, aw) in ANCHORS {
        if ac != id {
            continue;
        }
        fn find_site<'a>(
            v: &'a [Scored],
            map: &VerseMap,
            asid: &str,
            aw: &str,
        ) -> Option<&'a Scored> {
            v.iter()
                .filter(|s| s.sid.to_string() == asid)
                .find(|s| map[&s.sid][s.span.start as usize..s.span.end as usize].to_lowercase() == aw)
        }
        let find = |v: &[Scored]| -> Option<(f64, &'static str, f64, f64, String)> {
            find_site(v, map, asid, aw).map(|s| {
                (
                    s.score,
                    quad_str(s.quad),
                    s.trust,
                    s.habit,
                    s.class.map(|c| c.label()).unwrap_or_default(),
                )
            })
        };
        let b = find(&base);
        let t = find(&wired);
        let mut gate_alive = Vec::with_capacity(n_t);
        let mut gate_score = Vec::with_capacity(n_t);
        for out in &gate_scored {
            match find_site(out, map, asid, aw) {
                Some(s) => {
                    gate_alive.push(true);
                    gate_score.push(s.score);
                }
                None => {
                    gate_alive.push(false);
                    gate_score.push(0.0);
                }
            }
        }
        anchors.push(AnchorFate {
            corpus: ac.to_string(),
            sid: asid.to_string(),
            word: aw.to_string(),
            base_score: b.as_ref().map(|x| x.0).unwrap_or(0.0),
            tr_score: t.as_ref().map(|x| x.0).unwrap_or(0.0),
            base_alive: b.is_some(),
            tr_alive: t.is_some(),
            quad: t.as_ref().or(b.as_ref()).map(|x| x.1).unwrap_or("-"),
            class: t
                .as_ref()
                .or(b.as_ref())
                .map(|x| x.4.clone())
                .unwrap_or_default(),
            trust: t.as_ref().or(b.as_ref()).map(|x| x.2).unwrap_or(0.0),
            habit: t.as_ref().or(b.as_ref()).map(|x| x.3).unwrap_or(0.0),
            gate_alive,
            gate_score,
        });
    }

    TermCorpus {
        id,
        verses: map.len(),
        bicameral,
        trust: tt,
        base_i,
        base_p,
        base_b,
        tr_i,
        tr_p,
        tr_b,
        promoted_surfaced,
        changes,
        pool_gained: gained,
        pool_lost: lost,
        intrinsic_flips,
        samples_promoted,
        anchors,
        pos_delta,
        gate,
    }
}
