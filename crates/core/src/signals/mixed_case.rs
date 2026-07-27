//! Mixed-case word — the interior-capital anomaly (`wOrd`), corpus-relative and
//! stateful (ADR 0055).
//!
//! A word written in an **OtherMixed** shape — it has both cases and is neither
//! Titlecase nor ALLCAPS, so it necessarily carries an *interior* capital
//! (`DIos`, `MUngu`, `FIls`, `asÍ`) — is a slip *unless it is a convention*. The
//! conventions the corpus produces (`McX` names, `LORD`-inflected forms, Bantu
//! class prefixes `baYuda`, Hebrew construct `HaElohim`, Indonesian `TUHANlah`)
//! must be excused by **recurrence**, never a hardcoded list.
//!
//! ## The one route the spike kept: within-word (ADR 0055; spike 2026-07-10)
//!
//! Per case-folded word type, a profile of case shapes over {`lower`, `title`,
//! `allcaps`, `other`}. An OtherMixed occurrence is scored by the house
//! two-factor shape (ADR 0050/0051):
//!
//! `score = dominance(word's not-other-mixed share) × rarity(other-mixed count)`
//!
//! - **dominance** = the Wilson lower bound of `(lower+title+allcaps) / total` —
//!   how firmly this word's *own* usage is some clean shape. A word that is
//!   *dominantly* OtherMixed (`HaElohim ×419`) has `dominance ≈ 0` and is silent.
//! - **rarity** = the ADR 0050 absolute linear knee on the OtherMixed count: one
//!   stray mixed occurrence scores `1`, a mixed form that recurs past `k` fades
//!   to `0` — so **recurrence excuses the convention with no name list**.
//!
//! A **hapax** OtherMixed word (its only occurrence is the mixed one) has
//! `not_other = 0 ⇒ dominance 0 ⇒ silent` — structurally. The spike measured the
//! corpus-level hapax fallback (route B) and **rejected** it: 16× the volume,
//! almost entirely missing-space run-ons (`deJésus` — a spacing phenomenon) and
//! productive-morphology hapaxes, because the corpus-wide not-other-mixed
//! dominance is ≈1 for every corpus and so non-discriminating. Hapaxes stay
//! silent here, which is the safe thing (the clean Latin corpora that would
//! catch a genuine hapax slip have essentially no OtherMixed hapaxes to begin
//! with).
//!
//! ## Position is irrelevant; no censoring machinery
//!
//! Unlike initial-case (ADR 0051), a *mid-word* capital is position-independent:
//! the fleet OtherMixed rate is flat across the sentence seam (forced/mid ratio
//! 0.964). So this rule imports **none** of casing's forced-position / trust /
//! censoring machinery — no pending-terminal walk, no `confidence_z`-shrunk
//! habit beyond the single dominance estimate.
//!
//! ## Token unit and boundary vs casing v2
//!
//! Token unit = the plain UAX #29 **letter-run** word ([`is_letter_token`]) — no
//! hyphen merge, so `Obed-Edom` is two Titlecase tokens, never one OtherMixed
//! one (this is deliberately *not* casing's hyphen-merged `compound_words`,
//! which is why this rule cannot ride casing's word table and needs its own —
//! see the ADR). Single-letter and caseless guards live in
//! [`crate::signals::case_shape::case_shape`].
//!
//! First-upper OtherMixed (`McDonald`, `DIos`) is invisible to casing (which
//! fires only on lowercase word-starts), so it is unambiguously this rule's.
//! First-lower OtherMixed (`asÍ`, `kaniyang`) overlaps casing's lowercase-site
//! domain; casing's lowercase-site rules **skip OtherMixed tokens** (see
//! `signals::casing::walk_book`) so the interior-capital phenomenon is reported
//! once here, not twice.
//!
//! ## Evidence shape and merge (raw, per chapter then per book)
//!
//! Per chapter, the observation stores a word→[`ShapeProfile`] table of raw
//! four-shape counts. Every **cased** word is kept (an uncased/caseless token is
//! dropped — it has no shape); a word cannot be pruned to "only words seen mixed
//! somewhere" because its clean-shape mass — which drives `dominance` — is spread
//! across chapters and books, and a chapter with no local mixed observation still
//! carries mass the corpus-wide dominance needs. Keeping every cased word is what
//! keeps replacement **sound** at every granularity. The four small counts per
//! word are compact — strictly smaller than the casing table's per-word tallies.
//!
//! ## Why this is a substrate, and what it retains
//!
//! Every judged quantity is a function of **one word type's own merged counts**:
//! `dominance` is the Wilson bound on that word's clean share and `rarity` is the
//! knee on that word's OtherMixed count. Nothing is corpus-*global*. Two
//! consequences the casing substrate does not get:
//!
//! - the corpus aggregate is maintained **incrementally and exactly** — a book
//!   replacement subtracts its old per-word counts and adds its new ones, and
//!   integer counts make that bit-exact (casing's aggregate must be re-folded
//!   whole because its judge sums floats in a load-bearing order);
//! - the **stats-delta is genuinely per-key**: exactly the words whose merged
//!   counts moved, computed by a merge-join over the two sorted book tables.
//!
//! What it retains per chapter is its OtherMixed occurrences
//! ([`MixedCaseSite`], 12 bytes) — the only positions that can ever emit. That
//! replaces the pre-substrate judge's whole-corpus re-scan, which re-tokenized
//! and re-shaped every verse of every book on every call just to recover the
//! spans of a handful of surviving words.
use std::collections::BTreeMap;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::charclass::class_of;
use crate::config::MixedCaseConfig;
use crate::corpus::{Corpus, LocalKeyIdx, SiteAddr, rebase};
use crate::diagnostics::{Finding, FindingArgs, RuleId, Severity};
use crate::evidence::{clamp_count, clamp_unit, clamp_z, wilson_lower_bound};
use crate::interner::{WordInterner, WordSym};
use crate::signals::case_shape::{CaseShape, case_shape};
use crate::span::Span;
use crate::token::tokenize_into;

pub const MIXED_CASE_WORD: RuleId = RuleId::MixedCaseWord;

/// The absolute linear recurrence knee (ADR 0050/0051/0053/0055): a stray
/// occurrence scores `1`, fading linearly to `0` past `k`.
fn rarity(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// A UAX #29 token made only of cased/caseless letters and their combining
/// marks — the letter-run word unit. Numeric and mixed `q1`-style tokens are
/// excluded, matching the spike's token unit (ADR 0055). Mirrors
/// `signals::rare_glyph::is_letter_token`.
pub(crate) fn is_letter_token(word: &str) -> bool {
    let mut has_letter = false;
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_alphabetic() && !cl.is_mark() {
            has_letter = true;
        } else if !cl.is_mark() {
            return false;
        }
    }
    has_letter
}

/// One case-folded word type's raw shape counts at **book/corpus** width. Raw
/// and mergeable — no dominance, no censoring — so replacement at any
/// granularity holds. Only ever built by widening and summing
/// [`ChapterShapeProfile`]s; single occurrences are counted at chapter width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShapeProfile {
    pub(crate) lower: u32,
    pub(crate) title: u32,
    pub(crate) allcaps: u32,
    pub(crate) other: u32,
}

impl ShapeProfile {
    fn add(&mut self, o: &ShapeProfile) {
        self.lower += o.lower;
        self.title += o.title;
        self.allcaps += o.allcaps;
        self.other += o.other;
    }

    /// Remove a contribution previously [`add`](Self::add)ed. Exact: these are
    /// integer counts, so subtract-then-add restores the identical value — which
    /// is what lets the corpus aggregate be maintained incrementally instead of
    /// re-folded whole (see the module docs).
    fn sub(&mut self, o: &ShapeProfile) {
        self.lower -= o.lower;
        self.title -= o.title;
        self.allcaps -= o.allcaps;
        self.other -= o.other;
    }

    fn is_empty(&self) -> bool {
        self.total() == 0
    }

    fn total(&self) -> u64 {
        u64::from(self.lower)
            + u64::from(self.title)
            + u64::from(self.allcaps)
            + u64::from(self.other)
    }

    /// The clean-shape mass — the dominance numerator (`lower+title+allcaps`).
    fn not_other(&self) -> u64 {
        self.total() - u64::from(self.other)
    }
}

/// A per-**chapter** word type's raw shape counts — the same four slots as
/// [`ShapeProfile`] at half the width (8 bytes, not 16).
///
/// Two representations, deliberately. A chapter-local count is bounded by its own
/// chapter's letter-token count, which the fleet probe
/// ([`chapter_extent_probe`]) measures at a maximum of **5,632** (`nabNT`) — an
/// 11.6x margin under `u16`, with the widest single measured shape count for one
/// word type in one chapter at **552** (`udu`), a 118x margin. The book table and
/// the corpus sum are different populations entirely (they accumulate a whole
/// corpus's occurrences) and stay `u32`.
///
/// This matters because the per-chapter table is the lane's *scattered* half:
/// WA-en-ulb retains 263,514 chapter word entries where one per-book table held
/// ~13,000, which is what made Entry 26's RAM watch fire. Halving the element
/// halves that scatter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ChapterShapeProfile {
    lower: u16,
    title: u16,
    allcaps: u16,
    other: u16,
}

impl ChapterShapeProfile {
    /// Count one occurrence. Checked, not saturating: the ceiling is 11.6x the
    /// measured fleet maximum *chapter*, so reaching it means the corpus broke
    /// the structural assumption and must stop and be reported — a silently
    /// saturated count would corrupt every book and corpus total derived from it.
    fn record(&mut self, shape: CaseShape) {
        let slot = match shape {
            CaseShape::Lower => &mut self.lower,
            CaseShape::Title => &mut self.title,
            CaseShape::AllCaps => &mut self.allcaps,
            CaseShape::OtherMixed => &mut self.other,
        };
        *slot = slot.checked_add(1).expect(
            "one word type's shape count in one chapter fits u16 (fleet max 552, chapter \
             token max 5,632 — a violation is a stop-and-report, see granularity-spine Entry 28)",
        );
    }

    /// Widen to the mergeable corpus-width profile — the only way a chapter count
    /// enters a book or corpus total.
    fn widen(self) -> ShapeProfile {
        ShapeProfile {
            lower: u32::from(self.lower),
            title: u32::from(self.title),
            allcaps: u32::from(self.allcaps),
            other: u32::from(self.other),
        }
    }

    /// Only the fleet probe and its tests need a chapter-width total; every
    /// shipped total is taken after [`widen`](Self::widen).
    #[cfg(any(test, feature = "bench-probes"))]
    fn total(self) -> u64 {
        u64::from(self.lower) + u64::from(self.title) + u64::from(self.allcaps)
            + u64::from(self.other)
    }
}

// ── The mixed-case observation substrate (plan §5.2 / §11 ledger row). ──────

/// One OtherMixed occurrence: the word type it is an occurrence of, and its
/// verse-local address within the owning chapter. 12 bytes.
///
/// Both fields are retained deliberately, and the plan's retain-vs-rederive
/// principle is what picks each:
///
/// - `word` is the judge **key's** identity, not a verse-local offset.
///   Re-deriving it would mean case-folding the token's bytes again at every
///   judge; the shared interner already named it at map time for free.
/// - `addr` is the verse-local span. Re-deriving it from a token ordinal is the
///   principle's default, and it is declined here on measurement: the fleet's
///   widest verse holds 1,963 UAX #29 tokens, so an ordinal needs 16 bits and
///   buys **nothing** over the packed 16-bit span, while costing a
///   re-tokenization of the verse per emitted finding. The population is also
///   two to three orders of magnitude smaller than casing's lowercase sites
///   (only interior-capital tokens land here), so retention's rent is
///   negligible where casing's was the whole problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MixedCaseSite {
    word: WordSym,
    addr: SiteAddr,
}

/// Everything about one chapter that no entering state can change: its word-type
/// symbols, their raw shape counts, and its OtherMixed occurrences in scan
/// order. Boxed, not `Vec`: built once by the walk and never grown again, so a
/// retained chapter would otherwise hold its growth slack for a whole session.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedCaseWords {
    /// chapter-local id → the word type's symbol in the cache's shared
    /// [`WordInterner`], in first-sight order.
    keys: Box<[WordSym]>,
    /// Per-id raw shape counts, at chapter width (see [`ChapterShapeProfile`]).
    profiles: Box<[ChapterShapeProfile]>,
    /// OtherMixed occurrences in scan order — the only positions that can emit.
    sites: Box<[MixedCaseSite]>,
}

/// One chapter's input-independent mixed-case observation.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedCaseChapterObs {
    token: Box<str>,
    /// Shared with the reduced chapter and the book contribution: reduction is
    /// the identity here, so the table is handed on by `Arc` rather than copied.
    words: Arc<MixedCaseWords>,
}

/// One chapter's reduced mixed-case result — identical to its observation,
/// because nothing crosses a chapter seam for this rule.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedCaseReduced {
    token: Box<str>,
    words: Arc<MixedCaseWords>,
}

/// A book's ordered word table: `(folded word, raw shape counts)` sorted by
/// word. Sorted for two reasons — a deterministic `Eq` that cannot see which
/// integers the interner happened to hand out, and a merge-join against the
/// previous table that yields the exact stats-delta in one pass.
///
/// The key is a shared `Arc<str>` from the cache's [`WordInterner`], so building
/// this table and keying the corpus aggregate by it copies no bytes.
type MixedCaseBookWords = Vec<(Arc<str>, ShapeProfile)>;

/// A book's folded mixed-case contribution: its ordered word table (the corpus
/// aggregate's addend) and its chapters' reduced results.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct MixedCaseBookContribution {
    words: Arc<MixedCaseBookWords>,
    chapters: Vec<MixedCaseReduced>,
}

/// The mixed-case corpus aggregate: each book's ordered table, plus the
/// corpus-wide per-word sum maintained **incrementally** across book
/// replacements. Exact because the counts are integers (see [`ShapeProfile::sub`]).
#[derive(Default)]
pub(crate) struct MixedCaseCorpusStats {
    per_book: BTreeMap<Box<str>, Arc<MixedCaseBookWords>>,
    /// word → corpus-wide summed shape counts. A word whose every contribution
    /// has been subtracted away is removed, so this holds exactly the corpus's
    /// live cased vocabulary.
    merged: FxHashMap<Arc<str>, ShapeProfile>,
}

/// The judge key: the case-folded word type. Every judged quantity is a function
/// of this one word's merged counts and nothing else, which is what makes the
/// per-key stats-delta meaningful here.
pub(crate) type MixedCaseKey = Arc<str>;

/// One word's verdict: its score and finding-arg counts, or silence.
#[derive(Clone, Copy, Default)]
pub(crate) struct MixedCaseOutcome {
    emit: Option<(f32, u32, u32)>,
}

/// The judging half: the three clamped knobs, hoisted out of the per-key path.
#[derive(Clone, Copy)]
pub(crate) struct MixedCaseJudge {
    k: f64,
    floor: f64,
    z: f64,
}

impl MixedCaseJudge {
    fn new(cfg: &MixedCaseConfig) -> Self {
        MixedCaseJudge {
            k: clamp_count(cfg.recurrence_k),
            floor: f64::from(clamp_unit(cfg.emit_score_min)),
            z: clamp_z(cfg.confidence_z),
        }
    }
}

/// `case.mixed-case-word`'s typed observation substrate. Its boundary state is
/// **empty**, proven from the extraction walk rather than assumed: a token's
/// shape is a pure function of the token's own bytes
/// ([`case_shape`](crate::signals::case_shape::case_shape)) and its word type is
/// a pure function of its own fold, so the walk holds no pending state, reads no
/// neighbour, and looks at no previous verse. Position is deliberately
/// irrelevant to this rule (ADR 0055: the fleet OtherMixed rate is flat across
/// the sentence seam), which is exactly why it imports none of casing's
/// pending-terminal machine. Reduction is therefore the identity and every
/// replay converges at the chapter that changed.
pub(crate) struct MixedCaseSubstrate;

/// Pins the substrate's registry id at compile time.
const _: crate::substrate::SubstrateId =
    <MixedCaseSubstrate as crate::substrate::ObservationSubstrate>::ID;

/// One chapter's mixed-case map: the same per-token tally the listener always
/// ran, plus the chapter's OtherMixed occurrences.
struct ChapterAcc {
    intern: FxHashMap<String, u32>,
    keys: Vec<String>,
    profiles: Vec<ChapterShapeProfile>,
    sites: Vec<(u32, SiteAddr)>,
    tokens_buf: Vec<crate::token::Token>,
}

impl ChapterAcc {
    fn new() -> Self {
        ChapterAcc {
            intern: FxHashMap::default(),
            keys: Vec::new(),
            profiles: Vec::new(),
            sites: Vec::new(),
            tokens_buf: Vec::new(),
        }
    }

    fn verse(&mut self, local_idx: LocalKeyIdx, text: &str) {
        self.tokens_buf.clear();
        tokenize_into(text, &mut self.tokens_buf);
        for i in 0..self.tokens_buf.len() {
            let span = self.tokens_buf[i].span;
            let word = span.slice(text);
            if !is_letter_token(word) {
                continue;
            }
            let Some(shape) = case_shape(word) else {
                continue;
            };
            // The fold is the exact `to_lowercase` the fused-walk listener keyed
            // by, so the word types — and every count derived from them — are
            // unchanged by the migration.
            let key = word.to_lowercase();
            let id = match self.intern.get(&key) {
                Some(&id) => id,
                None => {
                    let id = self.keys.len() as u32;
                    self.intern.insert(key.clone(), id);
                    self.keys.push(key);
                    self.profiles.push(ChapterShapeProfile::default());
                    id
                }
            };
            self.profiles[id as usize].record(shape);
            if shape == CaseShape::OtherMixed {
                self.sites.push((id, SiteAddr::pack(local_idx, span)));
            }
        }
    }

    fn finish(self, token: &str, symbols: &WordInterner) -> MixedCaseChapterObs {
        let keys = symbols.intern_all(self.keys);
        let sites = self
            .sites
            .iter()
            .map(|&(id, addr)| MixedCaseSite {
                word: keys[id as usize],
                addr,
            })
            .collect();
        MixedCaseChapterObs {
            token: Box::from(token),
            words: Arc::new(MixedCaseWords {
                keys: keys.into_boxed_slice(),
                profiles: self.profiles.into_boxed_slice(),
                sites,
            }),
        }
    }
}

/// Fleet field-width probe for the per-chapter shape table (WP7b item 4), the
/// mixed-case analogue of casing's `field_extent_probe`. Returns
/// `(max letter tokens in one chapter, max single shape count for one word type
/// in one chapter)` — the second is the quantity a narrowed per-chapter counter
/// must hold, and the first is its structural upper bound. Measured through the
/// *exact* tokenization and shape classification `ChapterAcc::verse` uses, so it
/// cannot disagree with the table it is sizing.
#[cfg(feature = "bench-probes")]
pub fn chapter_extent_probe(corpus: &Corpus) -> (usize, usize) {
    let texts = corpus.texts();
    let mut max_tokens = 0usize;
    let mut max_count = 0usize;
    for book in corpus.book_layout() {
        for c in &book.chapters {
            let mut acc = ChapterAcc::new();
            let mut tokens = 0usize;
            for (vi, text) in texts[c.range.clone()].iter().enumerate() {
                acc.verse(LocalKeyIdx::from_usize(vi), text);
            }
            for p in &acc.profiles {
                tokens += p.total() as usize;
                max_count = max_count
                    .max(p.lower as usize)
                    .max(p.title as usize)
                    .max(p.allcaps as usize)
                    .max(p.other as usize);
            }
            max_tokens = max_tokens.max(tokens);
        }
    }
    (max_tokens, max_count)
}

impl crate::substrate::ObservationSubstrate for MixedCaseSubstrate {
    const ID: crate::substrate::SubstrateId = crate::substrate::SubstrateId::MixedCase;
    // Bump on any observation/reduction schema change.
    const SCHEMA_STAMP: u64 = 1;

    type Key = MixedCaseKey;
    type BoundaryState = ();
    type ChapterObservation = MixedCaseChapterObs;
    type ReducedChapter = MixedCaseReduced;
    type BookContribution = MixedCaseBookContribution;
    type CorpusStats = MixedCaseCorpusStats;
    // Every `MixedCaseConfig` field (the score floor, the recurrence knee, the
    // confidence z) is read at judge, so a knob change maps and reduces nothing.
    type ExtractorConfig = ();
    // The shared folded-word table, the same instance casing names its word
    // types through: a word's symbol has to mean the same thing in both.
    type Symbols = WordInterner;
    type JudgeConfig = MixedCaseJudge;
    type EntryOutcome = MixedCaseOutcome;

    fn extractor_fp(_extractor: &()) -> u64 {
        0
    }

    fn map_chapter(
        chapter: &crate::substrate::ChapterView<'_>,
        _extractor: &(),
        symbols: &WordInterner,
    ) -> MixedCaseChapterObs {
        let mut acc = ChapterAcc::new();
        for (vi, text) in chapter.texts.iter().enumerate() {
            acc.verse(LocalKeyIdx::from_usize(vi), text);
        }
        acc.finish(chapter.chapter, symbols)
    }

    fn pending_owner(_state: &()) -> Option<&str> {
        None
    }

    fn reduce_chapter(
        observation: &MixedCaseChapterObs,
        _entering: &(),
        _carry_out: &mut MixedCaseReduced,
    ) -> (MixedCaseReduced, ()) {
        (
            MixedCaseReduced {
                token: observation.token.clone(),
                words: Arc::clone(&observation.words),
            },
            (),
        )
    }

    fn finish_book(_leaving: &(), _carry_out: &mut MixedCaseReduced) {}

    fn fold_book(reduced: &[MixedCaseReduced], symbols: &WordInterner) -> MixedCaseBookContribution {
        // Sum the chapters' per-symbol profiles, keyed by the shared SYMBOL, so
        // this pass hashes 4-byte integers instead of words and never touches
        // the arena; the words are resolved once, below, for the sort.
        let mut intern: FxHashMap<WordSym, u32> = FxHashMap::default();
        let mut syms: Vec<WordSym> = Vec::new();
        let mut profiles: Vec<ShapeProfile> = Vec::new();
        for r in reduced {
            for (i, &sym) in r.words.keys.iter().enumerate() {
                let id = match intern.get(&sym) {
                    Some(&id) => id,
                    None => {
                        let id = syms.len() as u32;
                        intern.insert(sym, id);
                        syms.push(sym);
                        profiles.push(ShapeProfile::default());
                        id
                    }
                };
                profiles[id as usize].add(&r.words.profiles[i].widen());
            }
        }
        // Sort by the keys' STRING order — symbols are assigned in map-completion
        // order and carry no meaning here beyond identity, so a symbol-ordered
        // table would compare unequal across two caches holding identical text.
        let resolved = symbols.resolve_all(syms.iter().copied());
        let mut order: Vec<u32> = (0..resolved.len() as u32).collect();
        order.sort_unstable_by(|&a, &b| resolved[a as usize].cmp(&resolved[b as usize]));
        let words: MixedCaseBookWords = order
            .iter()
            .map(|&i| (Arc::clone(&resolved[i as usize]), profiles[i as usize]))
            .collect();
        MixedCaseBookContribution {
            words: Arc::new(words),
            chapters: reduced.to_vec(),
        }
    }

    fn replace_book_in_corpus_stats(
        stats: &mut MixedCaseCorpusStats,
        slug: &str,
        old: Option<&MixedCaseBookContribution>,
        new: Option<&MixedCaseBookContribution>,
    ) -> Vec<MixedCaseKey> {
        // Both tables are sorted by word, so one merge-join both applies the
        // replacement (subtract the old contribution, add the new) and yields
        // the EXACT stats-delta: a word's corpus sum moves iff this book's
        // contribution to it moved, so a word contributed identically by both
        // tables is untouched and is not a delta key. Equal counts are proof
        // here — unlike site equality — because the aggregate is a pure sum.
        let empty: MixedCaseBookWords = Vec::new();
        let o = old.map_or(&empty[..], |c| &c.words[..]);
        let n = new.map_or(&empty[..], |c| &c.words[..]);
        let mut delta: Vec<MixedCaseKey> = Vec::new();
        let mut i = 0usize;
        let mut j = 0usize;
        while i < o.len() || j < n.len() {
            let ord = match (o.get(i), n.get(j)) {
                (Some((a, _)), Some((b, _))) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => unreachable!("loop guard"),
            };
            match ord {
                std::cmp::Ordering::Less => {
                    let (w, p) = &o[i];
                    subtract(stats, w, p);
                    delta.push(Arc::clone(w));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    let (w, p) = &n[j];
                    stats.merged.entry(Arc::clone(w)).or_default().add(p);
                    delta.push(Arc::clone(w));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let (w, op) = &o[i];
                    let np = &n[j].1;
                    if op != np {
                        let e = stats
                            .merged
                            .get_mut(w)
                            .expect("a contributed word is in the aggregate");
                        e.sub(op);
                        e.add(np);
                        delta.push(Arc::clone(w));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        match new {
            Some(c) => {
                stats.per_book.insert(Box::from(slug), Arc::clone(&c.words));
            }
            None => {
                stats.per_book.remove(slug);
            }
        }
        delta
    }

    fn judge(
        judge: &MixedCaseJudge,
        key: &MixedCaseKey,
        stats: &MixedCaseCorpusStats,
    ) -> MixedCaseOutcome {
        let Some(p) = stats.merged.get(key) else {
            return MixedCaseOutcome::default();
        };
        // A word never seen OtherMixed has nothing to report; a hapax mixed word
        // has not_other == 0 ⇒ dominance 0 ⇒ silent, structurally (ADR 0055).
        if p.other == 0 {
            return MixedCaseOutcome::default();
        }
        let total = p.total();
        let dominance = wilson_lower_bound(p.not_other(), total, judge.z);
        let score = dominance * rarity(u64::from(p.other), judge.k);
        if score < judge.floor {
            return MixedCaseOutcome::default();
        }
        MixedCaseOutcome {
            emit: Some((
                score as f32,
                p.other,
                total.min(u64::from(u32::MAX)) as u32,
            )),
        }
    }
}

/// Remove one book's contribution to a word, dropping the entry when nothing is
/// left — so the aggregate holds exactly the corpus's live cased vocabulary and
/// a removed book cannot keep a dead word alive.
fn subtract(stats: &mut MixedCaseCorpusStats, word: &Arc<str>, p: &ShapeProfile) {
    if let Some(e) = stats.merged.get_mut(word) {
        e.sub(p);
        if e.is_empty() {
            stats.merged.remove(word);
        }
    }
}

impl MixedCaseBookContribution {
    /// Emit this book's findings: one per retained OtherMixed occurrence whose
    /// word type survived judging. `verdicts` is resolved once per analyze for
    /// every word any site names, so this is a hash probe per site.
    fn materialize(
        &self,
        slug: &str,
        corpus: &Corpus,
        verdicts: &FxHashMap<WordSym, (Arc<str>, MixedCaseOutcome)>,
        out: &mut Vec<Finding>,
    ) {
        for chapter in &self.chapters {
            let Some(range) = corpus.chapter_range(slug, &chapter.token) else {
                continue;
            };
            let base = crate::corpus::KeyIdx::from_usize(range.start);
            for site in chapter.words.sites.iter() {
                let Some((word, outcome)) = verdicts.get(&site.word) else {
                    continue;
                };
                let Some((score, other, total)) = outcome.emit else {
                    continue;
                };
                let (local, range) = site.addr.unpack();
                out.push(Finding {
                    key_idx: rebase(base, local),
                    code: MIXED_CASE_WORD,
                    severity: Severity::Info,
                    range: Span {
                        start: range.start,
                        end: range.end,
                    },
                    score: Some(score),
                    args: Some(FindingArgs::MixedCaseWord {
                        word: word.to_string(),
                        other,
                        total,
                    }),
                });
            }
        }
    }
}

/// The resident state one mixed-case drive reads and writes: the substrate's own
/// cache and the shared word table its observations name word types through.
pub(crate) struct MixedCaseState<'a> {
    pub(crate) cache: &'a mut crate::substrate::SubstrateCache<MixedCaseSubstrate>,
    pub(crate) symbols: &'a WordInterner,
}

/// One chapter the substrate has to map this analysis, as the ordered map seam
/// sees it: its caller-order `(book, chapter)` slot plus the view mapping reads.
struct MixedCaseMapWork<'a> {
    book: usize,
    chapter: usize,
    view: crate::substrate::ChapterView<'a>,
}

/// Drive the `case.mixed-case-word` observation substrate for one analysis: map
/// the dirty chapters through the ordered chapter-map seam, reduce (the
/// identity), judge exactly the word types its retained sites name, and
/// materialize. When inactive, drop the cached products so an edit while it is
/// disabled does no work for it.
pub(crate) fn drive_mixed_case(
    active: bool,
    state: MixedCaseState<'_>,
    corpus: &Corpus,
    cfg: &MixedCaseConfig,
    out: &mut Vec<Finding>,
) {
    use crate::substrate::{ChapterView, ObservationInputStamp, ObservationSubstrate};
    let MixedCaseState { cache, symbols } = state;
    #[cfg(any(test, feature = "test-probes"))]
    cache.reset_probes();
    if !active {
        cache.clear();
        return;
    }
    let texts = corpus.texts();
    let layout = corpus.book_layout();
    let mut stamped: Vec<Vec<(Box<str>, ObservationInputStamp)>> = Vec::with_capacity(layout.len());
    let mut work: Vec<MixedCaseMapWork<'_>> = Vec::new();
    let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut work_bytes = 0usize;
    for (bi, book) in layout.iter().enumerate() {
        let run_start = work.len();
        let mut chapters = Vec::with_capacity(book.chapters.len());
        for (ci, c) in book.chapters.iter().enumerate() {
            let stamp = ObservationInputStamp {
                schema_stamp: MixedCaseSubstrate::SCHEMA_STAMP,
                chapter_hash: c.hash,
                extractor_fp: MixedCaseSubstrate::extractor_fp(&()),
            };
            if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                let verses = &texts[c.range.clone()];
                work_bytes += verses.iter().map(String::len).sum::<usize>();
                work.push(MixedCaseMapWork {
                    book: bi,
                    chapter: ci,
                    view: ChapterView {
                        chapter: &c.chapter,
                        texts: verses,
                    },
                });
            }
            chapters.push((c.chapter.clone(), stamp));
        }
        if work.len() > run_start {
            book_runs.push(run_start..work.len());
        }
        stamped.push(chapters);
    }
    let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.map_route = route.label();
    }
    let fresh = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
        MixedCaseSubstrate::map_chapter(&w.view, &(), symbols)
    });
    // Back into caller-order `(book, chapter)` slots, so reduction reads them in
    // corpus order and never in completion order.
    let mut slots: Vec<Vec<Option<MixedCaseChapterObs>>> = layout
        .iter()
        .map(|b| (0..b.chapters.len()).map(|_| None).collect())
        .collect();
    for (w, obs) in work.iter().zip(fresh) {
        slots[w.book][w.chapter] = Some(obs);
    }
    for (bi, book) in layout.iter().enumerate() {
        cache.update_book(&book.slug, &stamped[bi], symbols, |i| {
            slots[bi][i].take().unwrap_or_else(|| {
                let c = &book.chapters[i];
                MixedCaseSubstrate::map_chapter(
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

    // The judge key set is exactly the word types the retained sites name — the
    // only words that could ever emit. Collected before judging so each word type
    // is judged once however many occurrences it has.
    let mut named: FxHashSet<WordSym> = FxHashSet::default();
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            for chapter in &contrib.chapters {
                for site in chapter.words.sites.iter() {
                    named.insert(site.word);
                }
            }
        }
    }
    let judge = MixedCaseJudge::new(cfg);
    let stats = cache.corpus_stats();
    let syms: Vec<WordSym> = named.into_iter().collect();
    let words = symbols.resolve_all(syms.iter().copied());
    let verdicts: FxHashMap<WordSym, (Arc<str>, MixedCaseOutcome)> = syms
        .iter()
        .copied()
        .zip(words)
        .map(|(sym, word)| {
            let outcome = MixedCaseSubstrate::judge(&judge, &word, stats);
            (sym, (word, outcome))
        })
        .collect();
    #[cfg(any(test, feature = "test-probes"))]
    {
        cache.judged = verdicts.len();
    }
    for book in layout {
        if let Some(contrib) = cache.book_contribution(&book.slug) {
            contrib.materialize(&book.slug, corpus, &verdicts, out);
        }
    }
}

/// The corpus-wide shape-count totals this substrate observes, as
/// `(lower, title, allcaps, other)` — the census lane's cross-check: its
/// `CaseShapes` section must count exactly the same shapes over the same token
/// unit. Reads the substrate's own corpus aggregate, so the two cannot drift.
#[cfg(test)]
pub(crate) fn shape_totals(corpus: &Corpus) -> [u64; 4] {
    let mut cache = crate::substrate::SubstrateCache::new();
    let symbols = WordInterner::default();
    let mut sink = Vec::new();
    drive_mixed_case(
        true,
        MixedCaseState {
            cache: &mut cache,
            symbols: &symbols,
        },
        corpus,
        &MixedCaseConfig::default(),
        &mut sink,
    );
    let mut totals = [0u64; 4];
    for p in cache.corpus_stats().merged.values() {
        totals[0] += u64::from(p.lower);
        totals[1] += u64::from(p.title);
        totals[2] += u64::from(p.allcaps);
        totals[3] += u64::from(p.other);
    }
    totals
}

/// `case.mixed-case-word` findings for a whole corpus at a given config, via the
/// observation substrate over a fresh transient cache — the single mixed-case
/// implementation, for tests and calibration callers. Findings are in the final
/// stable order.
#[cfg(any(test, feature = "bench-probes"))]
pub fn mixed_case_findings(corpus: &Corpus, cfg: &MixedCaseConfig) -> Vec<Finding> {
    let mut cache = crate::substrate::SubstrateCache::new();
    let symbols = WordInterner::default();
    let mut out = Vec::new();
    drive_mixed_case(
        true,
        MixedCaseState {
            cache: &mut cache,
            symbols: &symbols,
        },
        corpus,
        cfg,
        &mut out,
    );
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two representations are the point of WP7b item 4: the scattered
    /// per-chapter element is half the width of the mergeable one. Pinned,
    /// because a widening back is a multi-MiB regression on the retained lane
    /// that no behavioral test would notice.
    #[test]
    fn the_chapter_shape_profile_is_half_the_width_of_the_corpus_one() {
        assert_eq!(std::mem::size_of::<ChapterShapeProfile>(), 8);
        assert_eq!(std::mem::size_of::<ShapeProfile>(), 16);
    }

    /// The narrowed counter widens losslessly into the corpus-width profile —
    /// the only path by which a chapter count enters a book or corpus total.
    #[test]
    fn a_chapter_profile_widens_losslessly() {
        let mut c = ChapterShapeProfile::default();
        for _ in 0..7 {
            c.record(CaseShape::Lower);
        }
        c.record(CaseShape::Title);
        c.record(CaseShape::AllCaps);
        for _ in 0..3 {
            c.record(CaseShape::OtherMixed);
        }
        assert_eq!(c.total(), 12);
        assert_eq!(
            c.widen(),
            ShapeProfile {
                lower: 7,
                title: 1,
                allcaps: 1,
                other: 3
            }
        );
        assert_eq!(c.widen().total(), c.total());
    }

    /// The chapter-width bound is enforced, not assumed. The fleet's widest
    /// single shape count for one word type in one chapter is 552 (`udu`) and the
    /// widest chapter holds 5,632 letter tokens (`nabNT`) — 118x and 11.6x
    /// margins — and a corpus that broke that must stop rather than saturate,
    /// because a saturated chapter count would silently corrupt every book and
    /// corpus total folded from it.
    #[test]
    #[should_panic(expected = "one word type's shape count in one chapter fits u16")]
    fn the_chapter_shape_count_bound_panics_instead_of_saturating() {
        let mut c = ChapterShapeProfile {
            lower: u16::MAX,
            ..Default::default()
        };
        c.record(CaseShape::Lower);
    }

    fn cfg(emit_score_min: f32, recurrence_k: f32, confidence_z: f32) -> MixedCaseConfig {
        MixedCaseConfig {
            emit_score_min,
            recurrence_k,
            confidence_z,
        }
    }

    /// Every test runs the shipped substrate over a fresh transient cache — the
    /// one mixed-case implementation.
    fn run(corpus: &Corpus, cfg: &MixedCaseConfig) -> Vec<Finding> {
        mixed_case_findings(corpus, cfg)
    }

    fn slice<'a>(corpus: &'a Corpus, f: &Finding) -> &'a str {
        &corpus.text(f.key_idx)[f.range.start as usize..f.range.end as usize]
    }

    /// Accumulates `(key, text)` pairs, in insertion order, then builds the
    /// validated `Corpus` — the test-local stand-in for the old `VerseMap`,
    /// which let a test insert one extra verse at an arbitrary "verse number"
    /// because a `BTreeMap<Sid, _>` didn't care about insertion order. `Corpus`
    /// only requires each book's block to stay contiguous, so pushing extra
    /// verses onto the same book at the end works the same way.
    #[derive(Default)]
    struct CorpusBuilder {
        keys: Vec<String>,
        texts: Vec<String>,
    }

    impl CorpusBuilder {
        fn push(&mut self, book: &str, v: u16, text: &str) -> &mut Self {
            self.keys.push(format!("{book} 1:{v}"));
            self.texts.push(text.to_string());
            self
        }

        fn build(self) -> Corpus {
            Corpus::try_from_parts(self.keys, self.texts).unwrap()
        }
    }

    /// Build a corpus by cycling `templates`, one verse each, `reps` cycles.
    fn cycle(book: &str, templates: &[&str], reps: u16) -> CorpusBuilder {
        let mut b = CorpusBuilder::default();
        let mut v = 1u16;
        for _ in 0..reps {
            for t in templates {
                b.push(book, v, t);
                v += 1;
            }
        }
        b
    }

    // ── profile building + two-factor scoring ───────────────────────────────

    /// A word dominantly written clean (`dios` as `Dios`) with a lone interior-
    /// capital slip (`DIos`) surfaces exactly once, and the args carry the fact.
    #[test]
    fn interior_capital_slip_flags() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 40);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        let f = run(&corpus, &cfg(0.5, 32.0, 0.0));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&corpus, &f[0]), "DIos");
        assert_eq!(f[0].severity, Severity::Info);
        match &f[0].args {
            Some(FindingArgs::MixedCaseWord { word, other, total }) => {
                assert_eq!(word, "dios");
                assert_eq!((*other, *total), (1, 41));
            }
            other => panic!("expected MixedCaseWord, got {other:?}"),
        }
    }

    /// The score is `dominance × rarity`. With z=0 dominance is the raw share
    /// (40/41), rarity(1, k) = 1, so the score is ≈ 0.976.
    #[test]
    fn two_factor_score_is_dominance_times_rarity() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 40);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        let f = run(&corpus, &cfg(0.5, 32.0, 0.0));
        assert_eq!(f.len(), 1);
        let expected = (40.0 / 41.0) * 1.0;
        assert!(
            (f[0].score.unwrap() as f64 - expected).abs() < 1e-4,
            "{:?}",
            f[0].score
        );
    }

    /// The floor is respected: a low-dominance word (mixed is a big share of its
    /// own usage) drops below a high floor.
    #[test]
    fn floor_is_respected() {
        let mut cb = cycle("GEN", &["we praise Dios today"], 3);
        cb.push("GEN", 500, "we praise DIos today");
        let corpus = cb.build();
        // dominance = 3/4 = 0.75, below a 0.9 floor.
        assert!(run(&corpus, &cfg(0.9, 32.0, 0.0)).is_empty());
        assert_eq!(run(&corpus, &cfg(0.5, 32.0, 0.0)).len(), 1);
    }

    // ── recurrence excuses conventions (no hardcoded list) ───────────────────

    /// A mixed form that recurs is a convention, not a slip: the knee drives its
    /// rarity to zero and it goes silent — the `TUHANlah`/`MUngu` class, with no
    /// name list. The *same* word type recurring in its mixed shape is excused;
    /// one stray occurrence of it flags.
    #[test]
    fn recurrence_excuses_a_recurring_mixed_form() {
        // One-off: dominantly `Mungu` (Title), a single `MUngu` interior cap.
        let one = {
            let mut cb = cycle("GEN", &["we praise Mungu now"], 60);
            cb.push("GEN", 500, "we praise MUngu now");
            cb.build()
        };
        assert_eq!(run(&one, &cfg(0.5, 4.0, 0.0)).len(), 1);

        // Recurring convention: `MUngu` ×many collapses rarity past the knee.
        let many = {
            let mut cb = cycle("GEN", &["we praise Mungu now"], 60);
            for i in 0..20u16 {
                cb.push("GEN", 500 + i, "we praise MUngu now");
            }
            cb.build()
        };
        assert!(
            run(&many, &cfg(0.5, 4.0, 0.0)).is_empty(),
            "recurring convention silenced"
        );
    }

    /// A word dominantly written OtherMixed (a live convention like `HaElohim`)
    /// has dominance ≈ 0 and stays silent even though every occurrence is mixed.
    #[test]
    fn dominantly_mixed_convention_is_silent() {
        let corpus = cycle("GEN", &["and HaElohim spoke here"], 60).build();
        assert!(run(&corpus, &cfg(0.5, 32.0, 0.0)).is_empty());
    }

    // ── hapax silence + guards ───────────────────────────────────────────────

    /// A hapax OtherMixed word (its only occurrence is the mixed one) has
    /// not_other = 0 ⇒ dominance 0 ⇒ silent (route B is rejected, ADR 0055).
    #[test]
    fn hapax_mixed_word_is_silent() {
        let mut cb = cycle("GEN", &["nothing to see here"], 40);
        cb.push("GEN", 500, "a stray deJésus word");
        let corpus = cb.build();
        assert!(
            run(&corpus, &cfg(0.5, 32.0, 0.0)).is_empty(),
            "hapax mixed word stays silent"
        );
    }

    /// Single cased letters (`I`, `A`) are never OtherMixed, so a text full of
    /// them produces no findings (single-letter guard, via `case_shape`).
    #[test]
    fn single_letter_is_never_mixed() {
        let corpus = cycle("GEN", &["I A I saw A tree"], 40).build();
        assert!(run(&corpus, &cfg(0.0, 32.0, 0.0)).is_empty());
    }

    /// A caseless script has no shape, so nothing is a candidate.
    #[test]
    fn caseless_script_is_silent() {
        let corpus = cycle("GEN", &["उसने कहा वे चले", "फिर वह चला गया"], 40).build();
        assert!(run(&corpus, &cfg(0.0, 32.0, 0.0)).is_empty());
    }

    /// Hyphen compounds are two tokens, not one: `Obed-Edom` is two Titlecase
    /// tokens (never one OtherMixed), so it never flags — the token-unit rule.
    #[test]
    fn hyphen_compound_is_two_tokens() {
        let corpus = cycle("GEN", &["from Obed-Edom the gittite"], 60).build();
        assert!(
            run(&corpus, &cfg(0.5, 32.0, 0.0)).is_empty(),
            "Obed-Edom is two Title tokens"
        );
    }

    // ── boundary vs casing v2: reported once, not twice ──────────────────────

    /// The interior-capital phenomenon is reported once. A cap-dominant word
    /// (`dios` → `Dios`) written with a *plain* lowercase slip flags casing's
    /// `case.inconsistent-word-casing` (the control — casing genuinely fires on a
    /// lowercase site of this word). The *same* word written first-lower
    /// OtherMixed (`dIos`) is casing's to skip and mixed-case's to flag — so it
    /// surfaces once (interior-capital), never twice.
    #[test]
    fn casing_skips_othermixed_while_mixed_case_flags_it() {
        use crate::config::CasingConfig;

        let casing_cfg = CasingConfig {
            emit_score_min: 0.5,
            recurrence_k: 32.0,
            confidence_z: 0.0,
            trust_gate: 0.90,
        };
        // `case.inconsistent-word-casing` alone — the intrinsic consumer of the
        // shared casing substrate.
        let run_casing = |corpus: &Corpus| {
            crate::signals::casing::casing_findings(corpus, &casing_cfg, false, true)
        };

        // Control: a plain lowercase `dios` — casing DOES flag it.
        let control = {
            let mut cb = cycle("GEN", &["we praise Dios today"], 40);
            cb.push("GEN", 500, "we praise dios today");
            cb.build()
        };
        assert!(
            run_casing(&control)
                .iter()
                .any(|f| slice(&control, f) == "dios"),
            "control: casing flags a plain lowercase slip of a cap-dominant word"
        );

        // OtherMixed `dIos`: casing SKIPS it (reported by mixed-case instead) …
        let mixed = {
            let mut cb = cycle("GEN", &["we praise Dios today"], 40);
            cb.push("GEN", 500, "we praise dIos today");
            cb.build()
        };
        assert!(
            run_casing(&mixed).is_empty(),
            "casing skips the OtherMixed token: {:?}",
            run_casing(&mixed)
        );
        // … and mixed-case flags exactly that token.
        let f = run(&mixed, &cfg(0.5, 32.0, 0.0));
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(slice(&mixed, &f[0]), "dIos");
    }

    // ── resident equivalence + work probes (plan §8 Phase E, §12.3/§12.6) ────

    /// Keys and texts for a multi-chapter, multi-book corpus built from a fixed
    /// shape rotation. Keys are `BOOK ch:v`, so a chapter is a real chapter run.
    fn shaped(
        books: &[&str],
        chapters: u16,
        verses: u16,
        shapes: &'static [&'static str],
    ) -> (Vec<String>, Vec<&'static str>) {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for book in books {
            for ch in 1..=chapters {
                for v in 1..=verses {
                    keys.push(format!("{book} {ch}:{v}"));
                    texts.push(shapes[(keys.len() + ch as usize) % shapes.len()]);
                }
            }
        }
        (keys, texts)
    }

    /// Drive the substrate over a resident cache, in `mixed_case_findings`' order.
    fn resident(
        cache: &mut crate::substrate::SubstrateCache<MixedCaseSubstrate>,
        symbols: &WordInterner,
        corpus: &Corpus,
        cfg: &MixedCaseConfig,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        drive_mixed_case(true, MixedCaseState { cache, symbols }, corpus, cfg, &mut out);
        out.sort_by_key(|f| (f.key_idx, f.range.start, f.range.end));
        out
    }

    /// Comparable rendering — key, span text, score and both arg counts, so an
    /// equal-length-but-wrong result cannot pass.
    fn render(corpus: &Corpus, f: &[Finding]) -> Vec<String> {
        f.iter()
            .map(|f| {
                let a = match &f.args {
                    Some(FindingArgs::MixedCaseWord { word, other, total }) => {
                        format!("{word}/{other}/{total}")
                    }
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

    /// The egress form of casing's `symbol_numbering_never_reaches_the_book_fold`:
    /// a word's symbol number must not reach a **finding**, either. The shared
    /// `WordInterner` is append-only and long-lived, so by the time a corpus is
    /// analyzed in a real session the table already holds another corpus's
    /// vocabulary and hands out entirely different integers for the same words.
    ///
    /// Driven through the whole substrate — map, fold, judge, materialize — with a
    /// fresh table and with a pre-populated one, and compared on the rendered
    /// findings (key, span text, score, args), not on an internal table. Symbol
    /// numbers also govern the fold's `FxHashMap<WordSym, _>` iteration, so a
    /// numbering-dependent order anywhere in the lane would show up here.
    #[test]
    fn a_prefilled_interner_changes_no_finding() {
        const SHAPES: &[&str] = &[
            "we praise Dios today",
            "we praise DIos today",
            "and MUngu spoke here",
            "and Mungu spoke here",
            "HaElohim said so",
            "TUHANlah is written thus",
        ];
        let (keys, texts) = shaped(&["GEN", "EXO"], 6, 4, SHAPES);
        let corpus =
            Corpus::try_from_parts(keys, texts.iter().map(|t| (*t).to_string()).collect()).unwrap();
        let knobs = cfg(0.0, 32.0, 0.0);

        let fresh = WordInterner::default();
        let mut cache_a = crate::substrate::SubstrateCache::new();
        let a = resident(&mut cache_a, &fresh, &corpus, &knobs);
        assert!(!a.is_empty(), "the fixture must actually emit");

        // Pre-populate the table with an unrelated vocabulary, so every word of
        // `corpus` is numbered differently than it was above.
        let warm = WordInterner::default();
        let (pk, pt) = shaped(&["LEV"], 3, 3, &["wholly different vocabulary appears first"]);
        let primer =
            Corpus::try_from_parts(pk, pt.iter().map(|t| (*t).to_string()).collect()).unwrap();
        let mut primer_cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut primer_cache, &warm, &primer, &knobs);
        let primed = warm.len();
        assert!(primed > 0, "the primer must have named some words");

        let mut cache_b = crate::substrate::SubstrateCache::new();
        let b = resident(&mut cache_b, &warm, &corpus, &knobs);
        assert!(
            warm.len() > primed,
            "the second corpus must have taken fresh symbols"
        );
        assert_eq!(
            render(&corpus, &a),
            render(&corpus, &b),
            "a word's symbol number must not reach a finding"
        );
    }

    /// An edit to one chapter maps and reduces exactly that chapter. The boundary
    /// state is empty — a token's case shape is a function of its own bytes — so
    /// no reduction can cascade past the chapter that changed.
    #[test]
    fn an_edit_maps_and_reduces_exactly_its_own_chapter() {
        const SHAPES: &[&str] = &["we praise Dios today", "and Dios spoke", "Dios again"];
        let (keys, mut texts) = shaped(&["GEN"], 8, 3, SHAPES);
        let build = |texts: &[&str]| {
            Corpus::try_from_parts(keys.clone(), texts.iter().map(|t| (*t).to_string()).collect())
                .unwrap()
        };
        let knobs = cfg(0.5, 32.0, 0.0);
        let symbols = WordInterner::default();
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &symbols, &build(&texts), &knobs);
        assert_eq!(cache.mapped, 8, "cold maps every chapter");
        assert_eq!(cache.reduced, 8);

        // Introduce an interior-capital slip in chapter 5.
        texts[4 * 3 + 1] = "we praise DIos today";
        let e = build(&texts);
        cache.reset_probes();
        let inc = resident(&mut cache, &symbols, &e, &knobs);
        assert_eq!(cache.mapped, 1, "one changed chapter maps one chapter");
        assert_eq!(
            cache.reduced, 1,
            "an empty boundary state can never cascade past the changed chapter"
        );
        assert_eq!(inc.len(), 1, "{:?}", render(&e, &inc));
        assert_eq!(render(&e, &inc), render(&e, &mixed_case_findings(&e, &knobs)));

        // An unchanged re-drive maps and reduces nothing, and says the same thing.
        cache.reset_probes();
        let again = resident(&mut cache, &symbols, &e, &knobs);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert_eq!(render(&e, &again), render(&e, &inc));
    }

    /// A judging-knob change maps and reduces **nothing**: every knob is read at
    /// judge, so the extraction fingerprint cannot move.
    #[test]
    fn a_knob_change_maps_and_reduces_nothing() {
        const SHAPES: &[&str] = &[
            "we praise Dios today",
            "we praise DIos today",
            "Dios spoke",
        ];
        let (keys, texts) = shaped(&["GEN"], 4, 3, SHAPES);
        let corpus =
            Corpus::try_from_parts(keys, texts.iter().map(|t| (*t).to_string()).collect()).unwrap();
        let symbols = WordInterner::default();
        let mut cache = crate::substrate::SubstrateCache::new();
        let low = resident(&mut cache, &symbols, &corpus, &cfg(0.0, 32.0, 0.0));
        cache.reset_probes();
        let high = resident(&mut cache, &symbols, &corpus, &cfg(0.99, 32.0, 0.0));
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert!(!low.is_empty(), "the low floor must actually emit");
        assert!(high.is_empty(), "the high floor must actually silence it");
        assert_eq!(
            render(&corpus, &low),
            render(&corpus, &mixed_case_findings(&corpus, &cfg(0.0, 32.0, 0.0)))
        );
    }

    /// Property test (plan §12.6 shape): a resident cache driven through a
    /// pseudo-random edit sequence over a multi-chapter, multi-book corpus equals
    /// a cold whole-corpus run at every step. The rotation deliberately moves
    /// clean and mixed mass around, so both the corpus-wide dominance and the
    /// recurrence knee change under it — which is what exercises the
    /// incrementally maintained aggregate rather than just the site bookkeeping.
    #[test]
    fn resident_mixed_case_equals_cold_under_randomized_edits() {
        const SHAPES: &[&str] = &[
            "we praise Dios today",
            "we praise DIos today",
            "and MUngu spoke here",
            "and Mungu spoke here",
            "HaElohim said so",
            "",
            "plain words only",
        ];
        let (keys, mut texts) = shaped(&["GEN", "EXO"], 4, 3, SHAPES);
        let build = |texts: &[&str]| {
            Corpus::try_from_parts(keys.clone(), texts.iter().map(|t| (*t).to_string()).collect())
                .unwrap()
        };
        let knobs = cfg(0.5, 4.0, 0.0);
        let symbols = WordInterner::default();
        let mut cache = crate::substrate::SubstrateCache::new();
        let _ = resident(&mut cache, &symbols, &build(&texts), &knobs);
        let mut rng = 0x2545_F491_4F6C_DD1Du64;
        let next = |rng: &mut u64| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            *rng
        };
        let mut saw_findings = false;
        for step in 0..120 {
            let which = (next(&mut rng) % texts.len() as u64) as usize;
            texts[which] = SHAPES[(next(&mut rng) % SHAPES.len() as u64) as usize];
            let corpus = build(&texts);
            let inc = resident(&mut cache, &symbols, &corpus, &knobs);
            assert!(
                cache.mapped <= 1 && cache.reduced <= 1,
                "step {step}: one edited verse touches one chapter and converges there"
            );
            saw_findings |= !inc.is_empty();
            assert_eq!(
                render(&corpus, &inc),
                render(&corpus, &mixed_case_findings(&corpus, &knobs)),
                "step {step}: resident differs from cold"
            );
        }
        assert!(saw_findings, "the edit sequence never produced a finding");
    }

    // ── the incrementally maintained aggregate ───────────────────────────────

    /// The score is corpus-wide, and the aggregate is maintained incrementally
    /// across book replacements: a slip in one book scores against the whole
    /// resident corpus, and removing the book that supplied the dominance mass
    /// silences it again. Exercises `replace_book_in_corpus_stats`'s subtract/add
    /// path, which a whole-corpus re-fold would hide.
    #[test]
    fn the_aggregate_is_maintained_incrementally_across_book_replacement() {
        let knobs = cfg(0.5, 32.0, 0.0);

        // A clean GEN establishing `Dios` dominance, plus a dirty EXO.
        let mut both = cycle("GEN", &["we praise Dios today"], 40);
        both.push("EXO", 1, "we praise DIos today");
        let both = both.build();

        let mut cache = crate::substrate::SubstrateCache::new();
        let symbols = WordInterner::default();
        let out = resident(&mut cache, &symbols, &both, &knobs);
        assert_eq!(out.len(), 1, "corpus-wide dominance lifts the EXO slip");
        assert_eq!(both.key(out[0].key_idx), "EXO 1:1");

        // Drop GEN from the resident aggregate: the EXO slip loses its dominance
        // mass (its own book has seen only the mixed form) and goes silent.
        cache.remove_book("GEN");
        let exo = {
            let mut cb = CorpusBuilder::default();
            cb.push("EXO", 1, "we praise DIos today");
            cb.build()
        };
        let after = resident(&mut cache, &symbols, &exo, &knobs);
        assert!(after.is_empty(), "{after:?}");
    }

    /// The stats-delta is exactly the words whose corpus sum moved — not every
    /// word the replaced book contributed. Equal counts ARE proof here, because
    /// the aggregate is a pure integer sum (unlike site equality, plan §6.2).
    #[test]
    fn the_stats_delta_names_exactly_the_words_whose_sum_moved() {
        use crate::substrate::ObservationSubstrate;

        let before = cycle("GEN", &["alpha beta gamma"], 3).build();
        let after = {
            let mut cb = cycle("GEN", &["alpha beta gamma"], 2);
            cb.push("GEN", 3, "alpha beta DElta");
            cb.build()
        };
        let symbols = WordInterner::default();
        let fold = |corpus: &Corpus| {
            let mut cache: crate::substrate::SubstrateCache<MixedCaseSubstrate> =
                crate::substrate::SubstrateCache::new();
            let _ = resident(&mut cache, &symbols, corpus, &cfg(0.5, 32.0, 0.0));
            cache.book_contribution("GEN").expect("GEN folded").clone()
        };
        let old = fold(&before);
        let new = fold(&after);

        let mut stats = MixedCaseCorpusStats::default();
        assert_eq!(
            MixedCaseSubstrate::replace_book_in_corpus_stats(&mut stats, "GEN", None, Some(&old))
                .len(),
            3,
            "a first insertion names every word it contributes"
        );
        let delta = MixedCaseSubstrate::replace_book_in_corpus_stats(
            &mut stats,
            "GEN",
            Some(&old),
            Some(&new),
        );
        let mut names: Vec<&str> = delta.iter().map(|k| &**k).collect();
        names.sort_unstable();
        // `gamma` lost an occurrence and `delta` gained one; `alpha`/`beta` are
        // contributed identically by both tables and are NOT delta keys.
        assert_eq!(names, vec!["delta", "gamma"]);

        // And the incrementally maintained sum equals a fresh fold of `after`.
        let mut fresh = MixedCaseCorpusStats::default();
        MixedCaseSubstrate::replace_book_in_corpus_stats(&mut fresh, "GEN", None, Some(&new));
        let sorted = |s: &MixedCaseCorpusStats| {
            let mut v: Vec<(String, ShapeProfile)> =
                s.merged.iter().map(|(w, p)| (w.to_string(), *p)).collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        assert_eq!(sorted(&stats), sorted(&fresh));
    }
}
