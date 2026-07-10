//! Casing — two rules over one word lexicon, corpus-observed then judged.
//!
//! ADR 0051 rebuilds casing on a per-word case table (superseding ADR 0035's
//! per-glyph dominance). An occurrence's case is modelled as the OR of two
//! causes: the position forces uppercase, or the word is intrinsically
//! capitalized. Censoring is one-directional — uppercase at a forced position
//! is uninformative about the word; lowercase is informative everywhere. From
//! that model two rules judge the 2×2 of (position, word's intrinsic class):
//!
//! - [`SentenceInitialLowercase`] (`case.sentence-initial-lowercase`) — a
//!   **forced-position** lowercase site: `score = habit(glyph) × rarity(this
//!   word's forced-lowercase recurrence)`, where `habit` is the lexicon-
//!   restricted capitalize-after-terminal dominance (the decontaminated ADR
//!   0035 number — restricted to words the lexicon calls intrinsically
//!   lowercase, so proper nouns at sentence starts don't inflate it).
//! - [`InconsistentWordCasing`] (`case.inconsistent-word-casing`) — a lowercase
//!   site of an **intrinsically-capitalized** word: `score = dominance(word's
//!   soft-censored capitalized share) × rarity(word's lowercase recurrence)`.
//!   The first casing coverage of mid-flow text.
//!
//! A both-quadrant site (forced-position lowercase of an intrinsically-
//! capitalized word) breaks both conventions, so **both** rules may fire —
//! corroboration across observables, not double-counting.
//!
//! Emergent gates carry over: with no cased word-starts no convention exists,
//! so both rules stay silent (caseless scripts, by construction not a list).
//! The pending-terminal machine crosses verse seams (a verse start is not a
//! sentence start — `CLAUDE.md`); the **book-initial** word is forced, but
//! verse-initial is not. Both rules ship default-off — ~24% of cased languages
//! don't reliably capitalize after a period, so enabling is a per-project
//! language question.
//!
//! ## Stats shape and merge semantics (raw, per book)
//!
//! Per book, [`CasingStats`] stores a word→[`WordStats`] table of **raw**
//! tallies (mid-flow upper/lower, and forced upper/lower split by the terminal
//! glyph that forced them, book-initial kept separately). Nothing is censored
//! and no habit is computed at reduce: the lexicon classification and the
//! per-glyph habit only exist **corpus-wide**, so they are judge-time
//! arithmetic over the merged table. This keeps book-supersede sound — a book
//! carries exactly its own counts, replaced wholesale on edit — and keeps
//! `reduce` one walk.
//!
//! **Pruning (the ADR 0051 stats-tension resolution).** The lexicon
//! classification and the recurrence are corpus-wide, but `reduce` is per book,
//! so *any* per-book pruning of case mass is unsound at book granularity: drop
//! a word's lowercase mass in a book and a cross-book homograph (capitalized in
//! book A, lowercase-consistent in book B — `Word`/`word`) loses the recurrence
//! mass that keeps its many correct lowercase occurrences silent, storming
//! false intrinsic positives; drop its uppercase mass and the same homograph
//! loses the dominance evidence that flags its lone lowercase slip, a false
//! negative. The ADR's "persist words seen in both cases" cannot be applied
//! per book without one of these divergences from the corpus-wide model. The
//! sole per-book-safe drop is an **uncased-only** word (a caseless-script
//! token): it yields no candidate site and, being uncased, never enters the
//! lexicon-lowercase habit — it changes no verdict in any book. So that is all
//! that is pruned; every cased word is kept with raw tallies, matching the
//! spike's (unpruned) fleet numbers exactly. This keeps the table at the ADR's
//! measured cased-word cardinality; if that regresses, frequency-gating (drop
//! hapax types — the spike doc's suggested lever) is the deferred next step,
//! not a lossy per-book case prune.

use std::collections::{BTreeMap, HashMap};

use crate::charclass::class_of;
use crate::config::CasingConfig;
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::rule::{self, StatefulRule, TokenCache};
use crate::sid::{BookId, Sid};
use crate::span::Span;
use crate::stats::RuleStats;
use crate::token::tokenize;
use crate::verse::{Books, VerseMap};

pub const SENTENCE_INITIAL_LOWERCASE: RuleId = RuleId::SentenceInitialLowercase;
pub const INCONSISTENT_WORD_CASING: RuleId = RuleId::InconsistentWordCasing;

/// First-letter case of a word, from its first grapheme's base scalar.
/// `Uncased` is a caseless letter (no upper/lower distinction) — evidence for
/// neither convention, the emergent silence of caseless scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    Uncased,
}

/// The structural position class of a word, fixed at its first letter and
/// defined *before any casing knowledge* (the censoring model's generative
/// side). A position is "forced" — uppercase conventionally expected — right
/// after a bare attached terminal glyph, or book-initial. Everything else is
/// [`Midflow`](PosClass::Midflow), including the token after an *intervening*-
/// punctuation boundary (`."`, `...`). Verse-initial is NOT forced (verses are
/// reference plumbing; `CLAUDE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosClass {
    /// The first word of the book — forced with no terminal glyph.
    BookInitial,
    /// A word whose first letter consumed a *bare* attached terminal glyph
    /// (carried across verse seams by the pending-terminal machine). The glyph
    /// is the positional habit key.
    ForcedAfterTerminal(char),
    /// Not position-forced: uppercase here is intrinsic to the word.
    Midflow,
}

impl PosClass {
    fn is_forced(self) -> bool {
        !matches!(self, PosClass::Midflow)
    }

    /// The per-glyph habit key: `None` for book-initial (no terminal), the
    /// glyph for a terminal-forced position. Midflow has no habit key.
    fn habit_key(self) -> Option<char> {
        match self {
            PosClass::ForcedAfterTerminal(g) => Some(g),
            _ => None,
        }
    }
}

/// Forced-position first-letter tallies after one key (a terminal glyph, or —
/// stored separately — book-initial): how often the word appeared uppercase
/// vs lowercase there. Raw and mergeable.
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
/// tallies (a common word is one case in one position), so omitting them from
/// the wire is a large, lossless size win (ADR 0051 stats sizing).
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
}

/// One word's raw case tallies within one book: mid-flow upper/lower (the
/// intrinsic profile) and forced upper/lower, the latter split by the terminal
/// glyph so the lexicon-restricted per-glyph habit is derivable at judge time
/// (`book_initial` is the no-glyph forced key). All raw — no censoring, no
/// habit — so book-supersede holds.
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
    #[cfg_attr(feature = "wasm", tsify(optional, type = "Record<string, ForcedTally>"))]
    after_glyph: BTreeMap<char, ForcedTally>,
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
    }

    fn record(&mut self, pos: PosClass, case: Case) {
        match (pos, case) {
            (_, Case::Uncased) => {}
            (PosClass::Midflow, Case::Upper) => self.mid_upper += 1,
            (PosClass::Midflow, Case::Lower) => self.mid_lower += 1,
            (PosClass::BookInitial, Case::Upper) => self.book_initial.upper += 1,
            (PosClass::BookInitial, Case::Lower) => self.book_initial.lower += 1,
            (PosClass::ForcedAfterTerminal(g), Case::Upper) => {
                self.after_glyph.entry(g).or_default().upper += 1;
            }
            (PosClass::ForcedAfterTerminal(g), Case::Lower) => {
                self.after_glyph.entry(g).or_default().lower += 1;
            }
        }
    }

    /// True iff the word has ≥1 cased (upper or lower) word-start. The pruning
    /// predicate: uncased-only words are the sole per-book-safe drop.
    fn has_case(&self) -> bool {
        self.mid_upper > 0
            || self.mid_lower > 0
            || self.book_initial.upper > 0
            || self.book_initial.lower > 0
            || self.after_glyph.values().any(|t| t.upper > 0 || t.lower > 0)
    }

    fn forced_upper(&self) -> u64 {
        u64::from(self.book_initial.upper)
            + self.after_glyph.values().map(|t| u64::from(t.upper)).sum::<u64>()
    }
    fn forced_lower(&self) -> u64 {
        u64::from(self.book_initial.lower)
            + self.after_glyph.values().map(|t| u64::from(t.lower)).sum::<u64>()
    }
    fn forced_total(&self) -> u64 {
        self.forced_upper() + self.forced_lower()
    }
    fn mid_total(&self) -> u64 {
        u64::from(self.mid_upper) + u64::from(self.mid_lower)
    }
    fn total_upper(&self) -> u64 {
        u64::from(self.mid_upper) + self.forced_upper()
    }
    fn total_lower(&self) -> u64 {
        u64::from(self.mid_lower) + self.forced_lower()
    }
    fn total(&self) -> u64 {
        self.total_upper() + self.total_lower()
    }

    /// Hard lexicon class: mid-flow-lower-dominant. The habit's lexicon
    /// restriction uses this (midflow only — Step 1's censoring, no habit
    /// dependency, so no circularity).
    fn is_lexicon_lower(&self, z: f64) -> bool {
        self.mid_total() > 0
            && wilson_lower_bound(u64::from(self.mid_lower), self.mid_total(), z) > 0.5
    }

    /// Soft-censored effective uppercase count: mid-flow uppercase plus each
    /// forced-position uppercase re-entering at weight `1 − habit(key)` (ADR
    /// 0051 step 3 — in a no-habit corpus the forced pool returns; in a strong-
    /// habit corpus it is honestly near-worthless). One re-estimate, no EM.
    fn effective_upper(&self, habit: &Habit) -> f64 {
        let mut up = f64::from(self.mid_upper);
        if self.book_initial.upper > 0 {
            up += (1.0 - habit.dominance(None)) * f64::from(self.book_initial.upper);
        }
        for (g, t) in &self.after_glyph {
            if t.upper > 0 {
                up += (1.0 - habit.dominance(Some(*g))) * f64::from(t.upper);
            }
        }
        up
    }

    /// Soft-censored capitalized dominance: Wilson lower bound of `effective
    /// upper / (effective upper + mid-flow lower)`. Forced-lowercase is *not*
    /// in the intrinsic denominator — the intrinsic profile is mid-flow (Step
    /// 1); forced-lowercase feeds the positional channel and the recurrence.
    fn cap_dominance_soft(&self, habit: &Habit, z: f64) -> f64 {
        let up = self.effective_upper(habit);
        wilson_lower_bound_f(up, up + f64::from(self.mid_lower), z)
    }

    fn is_cap_soft(&self, habit: &Habit, z: f64) -> bool {
        let up = self.effective_upper(habit);
        let n = up + f64::from(self.mid_lower);
        n > 0.0 && self.cap_dominance_soft(habit, z) > 0.5
    }

    fn is_lower_soft(&self, habit: &Habit, z: f64) -> bool {
        let up = self.effective_upper(habit);
        let n = up + f64::from(self.mid_lower);
        n > 0.0 && wilson_lower_bound_f(f64::from(self.mid_lower), n, z) > 0.5
    }
}

/// Cached casing statistics, keyed by book so an edit supersedes only its book
/// (`BookId` crosses the wire as its `"GEN"` string). Corpus-wide aggregates,
/// the lexicon classification, and the per-glyph habit are all derived at
/// `judge` from the merged table — the per-book state is raw tallies only.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
pub struct CasingStats {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, BookCasing>"))]
    per_book: BTreeMap<BookId, BookCasing>,
}

/// One book's contribution: the pruned word table plus the cased-word-start
/// count that drives the emergent gate.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
struct BookCasing {
    #[cfg_attr(feature = "wasm", tsify(type = "Record<string, WordStats>"))]
    words: BTreeMap<String, WordStats>,
    /// Cased (upper or lower) word-start observations in the book — the
    /// emergent gate input, counted before pruning so a caseless book reads
    /// zero even though its (uncased) word entries are dropped.
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
    pub(crate) fn remove_book(&mut self, book: BookId) {
        self.per_book.remove(&book);
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

/// The absolute linear recurrence knee (ADR 0050, absolute form): a hapax
/// minority scores `1`, fading linearly to `0` past `k`. Fixing a word's
/// minority occurrences raises the survivors' rarity (clean-as-you-go).
fn rarity(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// The lexicon-restricted per-glyph capitalize-after-terminal habit: for each
/// forced key (a terminal glyph, or `None` = book-initial), the uppercase and
/// total forced counts summed over words the lexicon calls intrinsically
/// lowercase. Built corpus-wide at judge.
struct Habit {
    counts: HashMap<Option<char>, (u64, u64)>,
    z: f64,
}

impl Habit {
    /// Conservative uppercase dominance for a forced key (`0` when the key was
    /// never observed among lexicon-lowercase words).
    fn dominance(&self, key: Option<char>) -> f64 {
        match self.counts.get(&key) {
            Some(&(up, total)) => wilson_lower_bound(up, total, self.z),
            None => 0.0,
        }
    }

    fn raw(&self, key: Option<char>) -> (u64, u64) {
        self.counts.get(&key).copied().unwrap_or((0, 0))
    }
}

/// The corpus-wide model derived from merged [`CasingStats`]: the summed word
/// table and the lexicon-restricted per-glyph habit. Built once per `judge`;
/// shared by both rules and by the calibration harness ([`evaluate`]).
pub(crate) struct Model<'a> {
    words: HashMap<&'a str, WordStats>,
    habit: Habit,
    z: f64,
}

/// A convention factor pair: the dominance (Wilson bound) of the convention
/// the site breaks and the site's rarity inputs. `score = dominance ×
/// rarity(minority, k)`.
#[derive(Debug, Clone, Copy)]
pub struct Factors {
    pub dominance: f64,
    pub minority: u64,
    pub opportunities: u64,
    /// Descriptive majority/total for the finding args (ADR 0048).
    pub raw_major: u64,
    pub raw_total: u64,
}

impl<'a> Model<'a> {
    fn build(stats: &'a CasingStats, z: f64) -> Model<'a> {
        // Corpus-wide word table: sum each book's raw tallies.
        let mut words: HashMap<&str, WordStats> = HashMap::new();
        for bc in stats.per_book.values() {
            for (key, w) in &bc.words {
                words.entry(key.as_str()).or_default().add(w);
            }
        }
        // Lexicon-restricted per-glyph habit: forced tallies of the words the
        // (hard) lexicon calls intrinsically lowercase. The restriction is
        // what removes the proper-noun confound.
        let mut counts: HashMap<Option<char>, (u64, u64)> = HashMap::new();
        for w in words.values() {
            if !w.is_lexicon_lower(z) {
                continue;
            }
            if w.book_initial.upper + w.book_initial.lower > 0 {
                let e = counts.entry(None).or_default();
                e.0 += u64::from(w.book_initial.upper);
                e.1 += u64::from(w.book_initial.upper) + u64::from(w.book_initial.lower);
            }
            for (g, t) in &w.after_glyph {
                let e = counts.entry(Some(*g)).or_default();
                e.0 += u64::from(t.upper);
                e.1 += u64::from(t.upper) + u64::from(t.lower);
            }
        }
        Model { words, habit: Habit { counts, z }, z }
    }

    /// The intrinsic-channel factors for a lowercase site of word `key`, if the
    /// word is intrinsically capitalized (soft-censored). Covers the intrinsic
    /// and both quadrants.
    fn intrinsic(&self, key: &str) -> Option<Factors> {
        let w = self.words.get(key)?;
        if !w.is_cap_soft(&self.habit, self.z) {
            return None;
        }
        Some(Factors {
            dominance: w.cap_dominance_soft(&self.habit, self.z),
            minority: w.total_lower(),
            opportunities: w.total(),
            raw_major: w.total_upper(),
            raw_total: w.total(),
        })
    }

    /// The positional-channel factors for a forced-position lowercase site of
    /// word `key` at `pos`, if the word is classifiable (lexicon-lower or
    /// capitalized). Covers the positional and both quadrants.
    fn positional(&self, key: &str, pos: PosClass) -> Option<Factors> {
        if !pos.is_forced() {
            return None;
        }
        let w = self.words.get(key)?;
        // A forced-lowercase site of a word the lexicon cannot classify (neither
        // capitalized nor lower-dominant) is genuine ambiguity, not an anomaly.
        if !w.is_cap_soft(&self.habit, self.z) && !w.is_lower_soft(&self.habit, self.z) {
            return None;
        }
        let key_glyph = pos.habit_key();
        let (raw_major, raw_total) = self.habit.raw(key_glyph);
        Some(Factors {
            dominance: self.habit.dominance(key_glyph),
            minority: w.forced_lower(),
            opportunities: w.forced_total(),
            raw_major,
            raw_total,
        })
    }
}

/// A lowercase word-start observed by the book walk — a flag candidate for
/// either rule. Produced transiently and forwarded reduce→judge within a call
/// as [`crate::rule::RuleSites`] (ADR 0044); never stored in stats. Carries the
/// case-folded key so `judge` looks the word up in the merged model without
/// re-slicing the text on the forwarded path.
pub struct LowerSite {
    pub(crate) sid: Sid,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) key: String,
    pub(crate) pos: PosClass,
}

/// True iff `c` is a cased/uncased letter (GC L*) — the terminal machine's
/// "letter", and the flank test for a word-internal hyphen.
fn is_letter(c: char) -> bool {
    class_of(c).is_alphabetic()
}

/// The verse's word units: UAX #29 word tokens ([`crate::token::tokenize`]),
/// then adjacent tokens joined across a single word-internal hyphen (U+002D or
/// U+2010 flanked by a letter on both sides) merged into one span. UAX #29
/// keeps apostrophes word-internal (`ng'ombe` is one token) but SPLITS at
/// hyphens, so a compound like `Bar-jesus` would otherwise surface its tail as
/// a spurious lowercase word — the merge restores it as one word whose first
/// letter is `B`. Pure-number tokens (no letter) carry no casing evidence and
/// are dropped.
fn compound_words(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for t in tokenize(text) {
        if let Some(prev) = out.last_mut() {
            let gap = &text[prev.end..t.span.start];
            let mut g = gap.chars();
            let hyphen = matches!(g.next(), Some('\u{002D}' | '\u{2010}')) && g.next().is_none();
            if hyphen
                && text[..prev.end].chars().next_back().is_some_and(is_letter)
                && text[t.span.start..].chars().next().is_some_and(is_letter)
            {
                prev.end = t.span.end;
                continue;
            }
        }
        out.push(t.span);
    }
    out.retain(|s| text[s.start..s.end].chars().any(is_letter));
    out
}

/// Advance the pending-terminal state machine over a gap between words (all
/// non-word scalars): the first punctuation after a letter is the candidate
/// terminal, later punctuation before the next word marks the boundary
/// *intervening*, whitespace/digits are transparent.
fn advance_gap(gap: &str, pending: &mut Option<(char, bool)>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some((_, intervening)) => *intervening = true,
                None if *prev_letter => *pending = Some((c, false)),
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Scan one book's verses in canonical order, accumulating the raw per-word
/// table and producing the lowercase flag candidates. The pending terminal is
/// carried across verse seams (a verse start is not a sentence start); the
/// book-initial word is forced. A word cannot span a seam (verse texts are
/// separate strings). Entries with no lowercase occurrence are pruned before
/// return.
fn walk_book(verses: &[(Sid, &str)]) -> (BookCasing, Vec<LowerSite>) {
    let mut bc = BookCasing::default();
    let mut sites = Vec::new();
    let mut pending: Option<(char, bool)> = None;
    let mut book_initial = true;

    for (sid, text) in verses {
        let words = compound_words(text);
        // Seam gap: a terminal at this verse's start is not attached to the
        // previous verse's last letter. `prev_letter` never carries across.
        let mut prev_letter = false;
        let mut cursor = 0usize;

        for w in &words {
            advance_gap(&text[cursor..w.start], &mut pending, &mut prev_letter);

            let first = text[w.start..w.end].chars().next().unwrap();
            let fcl = class_of(first);
            let case = if fcl.is_uppercase() {
                Case::Upper
            } else if fcl.is_lowercase() {
                Case::Lower
            } else {
                Case::Uncased
            };
            let pos = if book_initial {
                PosClass::BookInitial
            } else if let Some((glyph, intervening)) = pending.take() {
                if intervening {
                    PosClass::Midflow
                } else {
                    PosClass::ForcedAfterTerminal(glyph)
                }
            } else {
                PosClass::Midflow
            };
            book_initial = false;

            if case != Case::Uncased {
                bc.cased_starts += 1;
            }
            let key = text[w.start..w.end].to_lowercase();
            bc.words.entry(key.clone()).or_default().record(pos, case);
            if case == Case::Lower {
                sites.push(LowerSite {
                    sid: *sid,
                    start: w.start as u32,
                    end: w.end as u32,
                    key,
                    pos,
                });
            }

            prev_letter = text[w.start..w.end].chars().next_back().is_some_and(is_letter);
            cursor = w.end;
        }
        advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
    }

    // Prune only uncased-only words (see the module doc): the sole per-book
    // drop that cannot change any corpus-wide verdict.
    bc.words.retain(|_, w| w.has_case());
    (bc, sites)
}

/// Shared reduce for both casing rules: walk each book once, producing the raw
/// per-book table and the forwarded lowercase sites.
fn reduce_casing(books: &Books<'_>) -> (CasingStats, BTreeMap<BookId, Vec<LowerSite>>) {
    let mut per_book = BTreeMap::new();
    let mut sites = BTreeMap::new();
    for (book, (bc, book_sites)) in rule::map_books(books, |book, verses| (book, walk_book(verses)))
    {
        per_book.insert(book, bc);
        sites.insert(book, book_sites);
    }
    (CasingStats { per_book }, sites)
}

/// True iff the merged corpus has any cased word-start — the emergent gate.
fn any_cased(stats: &CasingStats) -> bool {
    stats.per_book.values().any(|b| b.cased_starts > 0)
}

/// Shared judge skeleton: build the corpus model, recover each book's lowercase
/// sites (from the forwarded reduce sites where this call scanned the book, by
/// re-walking otherwise — ADR 0044), and let `emit` turn a site into at most
/// one finding for the calling rule's channel.
fn judge_casing(
    stats: &RuleStats,
    books: &Books<'_>,
    sites: Option<&rule::RuleSites>,
    cfg: &CasingConfig,
    emit: impl Fn(&LowerSite, &Model) -> Option<Finding> + Sync,
) -> Vec<Finding> {
    let RuleStats::Casing(stats) = stats else {
        return Vec::new();
    };
    // Emergent gate: no cased word-starts, no convention to violate.
    if !any_cased(stats) {
        return Vec::new();
    }
    let z = clamp_z(cfg.confidence_z);
    let model = Model::build(stats, z);

    let forwarded = match sites {
        Some(rule::RuleSites::Casing(m)) => Some(m),
        _ => None,
    };
    let mut out: Vec<Finding> = rule::map_books(books, |book, verses| {
        let mut found = Vec::new();
        if let Some(book_sites) = forwarded.and_then(|m| m.get(&book)) {
            for site in book_sites {
                if let Some(f) = emit(site, &model) {
                    found.push(f);
                }
            }
        } else {
            let (_, walked) = walk_book(verses);
            for site in &walked {
                if let Some(f) = emit(site, &model) {
                    found.push(f);
                }
            }
        }
        found
    })
    .into_iter()
    .flatten()
    .collect();
    out.sort_by_key(|f| (f.sid, f.range.start, f.range.end));
    out
}

fn site_span(site: &LowerSite) -> Span {
    Span { start: site.start as usize, end: site.end as usize }
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
        _source: Option<&VerseMap>,
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
        judge_casing(stats, books, sites, &self.cfg, |site, model| {
            let f = model.positional(&site.key, site.pos)?;
            let score = f.dominance * rarity(f.minority, k);
            if score < floor {
                return None;
            }
            Some(Finding {
                sid: site.sid,
                code: SENTENCE_INITIAL_LOWERCASE,
                severity: Severity::Info,
                range: site_span(site),
                score: Some(score as f32),
                args: Some(FindingArgs::CasingConvention {
                    glyph: site.pos.habit_key(),
                    upper: f.raw_major.min(u64::from(u32::MAX)) as u32,
                    total: f.raw_total.min(u64::from(u32::MAX)) as u32,
                }),
            })
        })
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
        _source: Option<&VerseMap>,
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
        judge_casing(stats, books, sites, &self.cfg, |site, model| {
            let f = model.intrinsic(&site.key)?;
            let score = f.dominance * rarity(f.minority, k);
            if score < floor {
                return None;
            }
            Some(Finding {
                sid: site.sid,
                code: INCONSISTENT_WORD_CASING,
                severity: Severity::Info,
                range: site_span(site),
                score: Some(score as f32),
                args: Some(FindingArgs::WordCasing {
                    word: site.key.clone(),
                    upper: f.raw_major.min(u64::from(u32::MAX)) as u32,
                    total: f.raw_total.min(u64::from(u32::MAX)) as u32,
                }),
            })
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Calibration API (ADR 0051). The `--casing` harness in examples/calibrate.rs
// consumes this to sweep floor/k and track review anchors over the real walk
// and the real model; it is not used by the shipped rules' judge, which apply
// the frozen knobs directly above.
// ─────────────────────────────────────────────────────────────────────────

/// One lowercase site evaluated against the corpus model: its position and the
/// two channels' factors (either may be absent). `score = dominance ×
/// rarity(minority, k)` on each present channel; the surfacing score is the
/// max of the two.
pub struct SiteEval {
    pub sid: Sid,
    pub start: u32,
    pub end: u32,
    pub pos: PosClass,
    pub intrinsic: Option<Factors>,
    pub positional: Option<Factors>,
}

/// Build the corpus model and classify every lowercase site — the calibration
/// entry point. Uses the same walk, model, and soft-censored classification the
/// shipped rules use, so swept volumes reflect the real implementation.
pub fn evaluate(books: &Books<'_>, confidence_z: f32) -> Vec<SiteEval> {
    let z = clamp_z(confidence_z);
    let (stats, sites_map) = reduce_casing(books);
    let model = Model::build(&stats, z);
    let mut out = Vec::new();
    for book_sites in sites_map.values() {
        for site in book_sites {
            out.push(SiteEval {
                sid: site.sid,
                start: site.start,
                end: site.end,
                pos: site.pos,
                intrinsic: model.intrinsic(&site.key),
                positional: model.positional(&site.key, site.pos),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;
    use crate::verse::by_book;

    fn cfg(emit_score_min: f32, recurrence_k: f32, confidence_z: f32) -> CasingConfig {
        CasingConfig { emit_score_min, recurrence_k, confidence_z }
    }

    fn sid(book: &str, v: u16) -> Sid {
        Sid::new(BookId::from_str(book).unwrap(), 1, v)
    }

    fn book(book: &str, verses: &[(u16, &str)]) -> VerseMap {
        verses.iter().map(|&(v, t)| (sid(book, v), t.to_string())).collect()
    }

    /// Reduce + judge one rule over a whole map (the same-call forwarded-sites
    /// path). Findings are already scoped to the map's verses.
    fn run(map: &VerseMap, r: &dyn StatefulRule) -> Vec<Finding> {
        let books = by_book(map);
        let (stats, sites) = r.reduce(&books, None, None);
        r.judge(&stats, &books, None, Some(&sites))
    }

    fn intrinsic(cfg: CasingConfig) -> InconsistentWordCasing {
        InconsistentWordCasing { cfg }
    }
    fn positional(cfg: CasingConfig) -> SentenceInitialLowercase {
        SentenceInitialLowercase { cfg }
    }

    fn slice<'a>(map: &'a VerseMap, f: &Finding) -> &'a str {
        &map[&f.sid][f.range.start..f.range.end]
    }

    /// Build a corpus by cycling `templates`, one verse each, `reps` cycles.
    fn cycle(book_code: &str, templates: &[&str], reps: u16) -> VerseMap {
        let mut out = VerseMap::new();
        let mut v = 1u16;
        for _ in 0..reps {
            for t in templates {
                out.insert(sid(book_code, v), (*t).to_string());
                v += 1;
            }
        }
        out
    }

    /// INTRINSIC fires: a capitalized word (`Jesus` mid-flow ×20) written once
    /// lowercase mid-flow is the anomaly; nothing else surfaces.
    #[test]
    fn intrinsic_flags_a_lowercased_capital_word() {
        let mut vm = cycle("GEN", &["we saw Jesus"], 20);
        vm.insert(sid("GEN", 100), "we saw jesus".to_string());
        let f = run(&vm, &intrinsic(cfg(0.5, 32.0, 0.0)));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].sid, sid("GEN", 100));
        assert_eq!(slice(&vm, &f[0]), "jesus");
        assert_eq!(f[0].code, INCONSISTENT_WORD_CASING);
        match &f[0].args {
            Some(FindingArgs::WordCasing { word, upper, total }) => {
                assert_eq!(word, "jesus");
                assert_eq!((*upper, *total), (20, 21));
            }
            other => panic!("expected WordCasing, got {other:?}"),
        }
        // The positional rule stays silent — `jesus` is mid-flow, not forced.
        assert!(run(&vm, &positional(cfg(0.5, 32.0, 0.0))).is_empty());
    }

    /// POSITIONAL fires: a corpus that reliably capitalizes a lexicon-lowercase
    /// word (`the`) after a period, then writes it lowercase there once.
    #[test]
    fn positional_flags_lowercase_after_a_strong_terminal() {
        // "The" opens every sentence (forced upper, across the seam); "the"
        // recurs mid-flow. The trailing '.' is what forces the next start.
        let mut vm = cycle("GEN", &["The men saw the gate."], 30);
        vm.insert(sid("GEN", 100), "He fell. the men ran.".to_string());
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        let hit: Vec<_> = f.iter().filter(|f| slice(&vm, f) == "the").collect();
        assert_eq!(hit.len(), 1, "{f:?}");
        assert_eq!(hit[0].sid, sid("GEN", 100));
        assert_eq!(hit[0].code, SENTENCE_INITIAL_LOWERCASE);
        match &hit[0].args {
            Some(FindingArgs::CasingConvention { glyph, upper, total }) => {
                assert_eq!(*glyph, Some('.'));
                assert!(*upper > 0 && *upper <= *total);
            }
            other => panic!("expected CasingConvention, got {other:?}"),
        }
    }

    /// The recurrence factor silences a minority that recurs, on BOTH channels:
    /// the same dominance that fires at minority 1 goes silent at minority > k.
    #[test]
    fn recurrence_silences_a_recurring_minority() {
        // Intrinsic: `jesus` capital ×100, lowercase mid-flow either 1 or 40.
        let one = {
            let mut vm = cycle("GEN", &["we saw Jesus"], 100);
            vm.insert(sid("GEN", 200), "we saw jesus".to_string());
            vm
        };
        let many = {
            let mut vm = cycle("GEN", &["we saw Jesus"], 100);
            for i in 0..40 {
                vm.insert(sid("GEN", 200 + i), "we saw jesus".to_string());
            }
            vm
        };
        let r = intrinsic(cfg(0.5, 32.0, 0.0));
        assert_eq!(run(&one, &r).len(), 1, "a single slip surfaces");
        assert!(run(&many, &r).is_empty(), "a minority recurring past k is silent");

        // Positional: `the` capital after '.' ×100, lowercase after '.' 1 or 40.
        let p_one = {
            let mut vm = cycle("GEN", &["The men saw the gate."], 100);
            vm.insert(sid("GEN", 300), "He fell. the men ran.".to_string());
            vm
        };
        let p_many = {
            let mut vm = cycle("GEN", &["The men saw the gate."], 100);
            for i in 0..40 {
                vm.insert(sid("GEN", 300 + i), "He fell. the men ran.".to_string());
            }
            vm
        };
        let pr = positional(cfg(0.5, 32.0, 0.0));
        assert!(run(&p_one, &pr).iter().any(|f| slice(&p_one, f) == "the"));
        assert!(
            !run(&p_many, &pr).iter().any(|f| slice(&p_many, f) == "the"),
            "a forced-lowercase form recurring past k is silent"
        );
    }

    /// Lexicon restriction: a corpus that capitalizes ONLY proper nouns after
    /// periods asserts no positional habit — the naive rule would flag, the
    /// lexicon-restricted one stays silent.
    #[test]
    fn lexicon_restriction_kills_a_proper_noun_only_habit() {
        // Every sentence start is the proper noun `God` (never lowercase);
        // the lexicon-lowercase words that follow periods (`we`, via the seam)
        // are lowercase, so the restricted habit is ~0.
        let mut vm = cycle("GEN", &["we praise God"], 30);
        vm.insert(sid("GEN", 100), "we praise God. god is good".to_string());
        // Positional is silent: `god` after '.' has no habit backing it.
        assert!(
            !run(&vm, &positional(cfg(0.95, 32.0, 1.96))).iter().any(|f| slice(&vm, f) == "god"),
            "proper-noun-only sentence starts assert no habit"
        );
    }

    /// Soft censoring: a word seen capitalized only at sentence starts still
    /// earns an intrinsic profile in a NO-habit corpus (the forced pool returns
    /// at weight ≈1), but not in a STRONG-habit corpus (the position explains
    /// the capital, weight ≈0).
    #[test]
    fn soft_censoring_depends_on_the_corpus_habit() {
        // "amen" appears only as forced-upper `Amen` after '.', plus one
        // mid-flow lowercase `amen`. The difference between the two corpora is
        // solely whether OTHER sentence starts are capitalized.
        // The lexicon-lowercase word `the` sets the habit: written lowercase
        // after '.' (`the dog runs`) it is a no-habit corpus; written `The` it
        // is a strong-habit corpus. That is the only difference between them.
        let no_habit = {
            let mut vm = cycle("GEN", &["we see the cat. the dog runs."], 20);
            for i in 0..5 {
                vm.insert(sid("GEN", 100 + i), "he said. Amen indeed.".to_string());
            }
            vm.insert(sid("GEN", 200), "so amen then".to_string());
            vm
        };
        let strong_habit = {
            let mut vm = cycle("GEN", &["we see the cat. The dog runs."], 20);
            for i in 0..5 {
                vm.insert(sid("GEN", 100 + i), "he said. Amen indeed.".to_string());
            }
            vm.insert(sid("GEN", 200), "so amen then".to_string());
            vm
        };
        let r = intrinsic(cfg(0.5, 32.0, 0.0));
        assert!(
            run(&no_habit, &r).iter().any(|f| slice(&no_habit, f) == "amen"),
            "no-habit corpus: forced-upper re-enters, `amen` is intrinsically capital"
        );
        assert!(
            !run(&strong_habit, &r).iter().any(|f| slice(&strong_habit, f) == "amen"),
            "strong-habit corpus: the position explains the capital, `amen` is not"
        );
    }

    /// Caseless script: no cased word-starts, so the emergent gate silences
    /// both rules (silence by construction, not a script list).
    #[test]
    fn caseless_script_is_silent() {
        let vm = book(
            "GEN",
            &[(1, "उसने कहा। वे चले गए।"), (2, "फिर वह चला गया।")],
        );
        assert!(run(&vm, &intrinsic(cfg(0.0, 32.0, 0.0))).is_empty());
        assert!(run(&vm, &positional(cfg(0.0, 32.0, 0.0))).is_empty());
    }

    /// The pending terminal carries across a verse seam: a period ending verse
    /// N forces the first word of verse N+1 (positional), which the old
    /// per-verse rule could never see.
    #[test]
    fn positional_carries_across_a_verse_seam() {
        // Every sentence starts with an uppercase `There`; `there` also recurs
        // mid-flow, so it is lexicon-lowercase with a strong '.'-habit.
        let mut vm = cycle("GEN", &["There we go there.", "There it is there."], 30);
        // A period ends verse 200; verse 201 opens with lowercase `there`.
        vm.insert(sid("GEN", 200), "he stops.".to_string());
        vm.insert(sid("GEN", 201), "there he goes".to_string());
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(
            f.iter().any(|f| f.sid == sid("GEN", 201) && slice(&vm, f) == "there"),
            "the '.' ending v200 forces `there` opening v201: {f:?}"
        );
    }

    /// Verse-initial is NOT forced: a verse continuing a previous verse that
    /// has NO terminal is mid-flow, so a lowercase word there is not positional
    /// even under a strong habit.
    #[test]
    fn verse_initial_without_a_terminal_is_not_forced() {
        // Strong '.'-habit (starts are uppercase `There`, no lowercase forced).
        let mut vm = cycle("GEN", &["There we go there.", "There it is there."], 30);
        vm.insert(sid("GEN", 200), "he walks".to_string()); // NO terminal
        vm.insert(sid("GEN", 201), "there he goes".to_string()); // continuation
        let f = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(
            !f.iter().any(|f| f.sid == sid("GEN", 201)),
            "no terminal at the seam ⇒ v201's `there` is a continuation, not forced: {f:?}"
        );
    }

    /// A hyphen-flanked compound is one word: `Bar-jesus` never surfaces its
    /// lowercase tail `jesus` as an anomaly (it is one word starting `B`),
    /// whereas a bare lowercase `jesus` does.
    #[test]
    fn hyphen_compound_is_one_word() {
        let mut compound = cycle("GEN", &["we saw Jesus"], 20);
        compound.insert(sid("GEN", 100), "he met Bar-jesus".to_string());
        assert!(
            run(&compound, &intrinsic(cfg(0.5, 32.0, 0.0))).is_empty(),
            "the compound tail is not a separate lowercase word"
        );

        let mut bare = cycle("GEN", &["we saw Jesus"], 20);
        bare.insert(sid("GEN", 100), "he met jesus".to_string());
        assert_eq!(
            run(&bare, &intrinsic(cfg(0.5, 32.0, 0.0))).len(),
            1,
            "a bare lowercase `jesus` does surface"
        );
    }

    /// A both-quadrant site (forced-position lowercase of an intrinsically-
    /// capitalized word) fires BOTH rules — corroboration.
    #[test]
    fn both_quadrant_fires_both_rules() {
        // `God` is capitalized mid-flow (intrinsic); `The` builds the '.'-habit
        // (positional); verse 100 writes `god` lowercase right after a period.
        let mut vm = cycle("GEN", &["The men praise God near the gate."], 30);
        vm.insert(sid("GEN", 100), "He wept. god is near.".to_string());
        let fi = run(&vm, &intrinsic(cfg(0.5, 32.0, 0.0)));
        let fp = run(&vm, &positional(cfg(0.5, 32.0, 0.0)));
        assert!(
            fi.iter().any(|f| f.sid == sid("GEN", 100) && slice(&vm, f) == "god"),
            "intrinsic fires on the lowercased capital: {fi:?}"
        );
        assert!(
            fp.iter().any(|f| f.sid == sid("GEN", 100) && slice(&vm, f) == "god"),
            "positional fires on the same forced site: {fp:?}"
        );
    }

    /// Editing a book supersedes its prior stats (merge at book granularity):
    /// a corrected re-reduce makes a previously-flagged anomaly disappear, and
    /// `remove_book` drops a book's contribution entirely.
    #[test]
    fn book_supersede_via_merge_and_remove() {
        let r = intrinsic(cfg(0.5, 32.0, 0.0));
        let mut dirty = cycle("GEN", &["we saw Jesus"], 20);
        dirty.insert(sid("GEN", 100), "we saw jesus".to_string());
        let dirty_books = by_book(&dirty);
        let (prior, _) = r.reduce(&dirty_books, None, None);
        assert_eq!(r.judge(&prior, &dirty_books, None, None).len(), 1);

        // Corrected edit of GEN: `jesus` → `Jesus`. Merge supersedes the book.
        let mut fixed = cycle("GEN", &["we saw Jesus"], 20);
        fixed.insert(sid("GEN", 100), "we saw Jesus".to_string());
        let fixed_books = by_book(&fixed);
        let (fresh, _) = r.reduce(&fixed_books, None, None);
        let merged = prior.merge(fresh);
        assert!(r.judge(&merged, &fixed_books, None, None).is_empty(), "supersede clears it");

        // remove_book drops the contribution: an EXO-only anomaly backed by
        // GEN's habit falls away once GEN is removed.
        let mut two = cycle("GEN", &["we saw Jesus"], 20);
        two.extend(book("EXO", &[(1, "we saw jesus")]));
        let (mut stats2, _) = r.reduce(&by_book(&two), None, None);
        assert_eq!(r.judge(&stats2, &by_book(&two), None, None).len(), 1);
        let RuleStats::Casing(ref mut c) = stats2 else { unreachable!() };
        c.remove_book(BookId::from_str("GEN").unwrap());
        let exo = book("EXO", &[(1, "we saw jesus")]);
        assert!(
            r.judge(&stats2, &by_book(&exo), None, None).is_empty(),
            "without GEN's evidence, EXO's lone `jesus` can't assert a convention"
        );
    }

    /// Floor and knee are honoured: the same site clears a low floor and not a
    /// high one; and shrinking `k` silences a two-occurrence slip a wide `k`
    /// keeps.
    #[test]
    fn floor_and_knee_config_are_respected() {
        // dom(jesus) = 100/101 ≈ 0.990 at z=0, minority 1 ⇒ score ≈ 0.990.
        let mut vm = cycle("GEN", &["we saw Jesus"], 100);
        vm.insert(sid("GEN", 200), "we saw jesus".to_string());
        assert_eq!(run(&vm, &intrinsic(cfg(0.95, 32.0, 0.0))).len(), 1, "clears 0.95");
        assert!(run(&vm, &intrinsic(cfg(0.999, 32.0, 0.0))).is_empty(), "not 0.999");

        // Two lowercase occurrences: rarity(2, k) = 1 − 1/k. At k=32 the
        // recurrence factor barely erodes the score (both survive a 0.5 floor);
        // at k=1 rarity = 0, so both go silent — the knee, not the floor.
        let mut two = cycle("GEN", &["we saw Jesus"], 100);
        two.insert(sid("GEN", 200), "we saw jesus".to_string());
        two.insert(sid("GEN", 201), "we saw jesus".to_string());
        assert_eq!(run(&two, &intrinsic(cfg(0.5, 32.0, 0.0))).len(), 2, "wide knee keeps both");
        assert!(run(&two, &intrinsic(cfg(0.5, 1.0, 0.0))).is_empty(), "narrow knee silences");
    }
}
