//! Typed observation substrates — the compile-time contract for shared,
//! strongly-typed evidence (plan §5.2/§5.3).
//!
//! A substrate is a reusable model that one or more rules ("consumers") judge
//! against. Rules consume substrates, never each other: shared evidence is
//! represented once as a typed substrate, so no rule ever inspects another
//! rule's enabled bit or verdict.
//!
//! The contract is entirely compile-time. There is no `dyn Any`, no runtime
//! downcast, and no `Box<dyn ObservationSubstrate>`: every cache slot and every
//! judge/substrate pairing is an explicit typed field the compiler checks. A
//! new substrate is a compile error until it has a cache slot and a registry
//! row.
//!
//! Three purity rules the trait's signatures enforce structurally:
//!
//! - **Mapping is predecessor-free.** [`map_chapter`](ObservationSubstrate::map_chapter)
//!   sees one chapter, the extractor config, and the substrate's append-only
//!   symbol naming ([`ObservationSubstrate::Symbols`], which encodes an
//!   observation without changing what it says) — never a neighbour's state and
//!   never a judging knob. A chapter's observation is therefore identical
//!   wherever the chapter sits, which is what lets an unchanged chapter's
//!   observation be reused after any structural edit.
//! - **Reduction carries all boundary state.**
//!   [`reduce_chapter`](ObservationSubstrate::reduce_chapter) is a left-to-right
//!   carry fold: it consumes the chapter's entering boundary state and emits its
//!   leaving state, never peeking at the next chapter. Cross-seam discourse
//!   (repo `CLAUDE.md`: a chapter boundary is not a discourse reset) flows
//!   through the boundary state, never through a silent reset.
//! - **Judging never mutates corpus stats.**
//!   [`judge`](ObservationSubstrate::judge) reads the corpus aggregate and its
//!   own judging knobs and returns a per-key outcome; the aggregate is built
//!   only by [`replace_book_in_corpus_stats`](ObservationSubstrate::replace_book_in_corpus_stats).
//!
//! The contract is driven from [`crate::transition`] via each substrate's
//! `drive_*` entry point. [`SubstrateCache::update_book`] is the generic ordered
//! reduction-to-convergence driver every substrate shares: it maps only the
//! chapters whose content moved and then replays the carry fold over **cached
//! observations** until the boundary state converges, so a changed carry never
//! re-walks an unchanged chapter's text.

use rustc_hash::FxHashMap;

use crate::config::Config;
use crate::diagnostics::RuleId;

/// The closed set of typed observation substrates. One variant per migrated
/// substrate. The cache slots ([`crate::cache::SubstrateSection`]) and the
/// registry below are exhaustive over this enum, and the completeness tests pin
/// that — the compiler, not a string list, proves the pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SubstrateId {
    /// `punct.spacing-anomaly`'s per-mark per-side attachment model.
    Spacing,
    /// `punct.adjacency-anomaly`'s per-pattern / per-lead-glyph run counts.
    Adjacency,
    /// `lex.repeated-character-run`'s per-cluster / per-word recurrence counts.
    RepeatedRun,
    /// `lex.punct-only-token`'s per-pattern candidate counts.
    PunctOnly,
    /// `uni.mixed-script-in-token`'s per-signature / per-script token counts.
    MixedScript,
    /// `uni.rare-glyph`'s scalar inventory + rare-letter word detail.
    Glyph,
    /// `proj.length-ratio`'s per-book target/reference ratio samples — the only
    /// substrate that declares a REFERENCE input (plan §5.2).
    Proportionality,
    /// `struct.duplicate-word`'s adjacent-pair sites.
    DuplicateWord,
    /// The shared casing model: per-word case tables + lowercase flag
    /// candidates, judged by two rules.
    Casing,
    /// `case.mixed-case-word`'s per-word case-shape profiles + its retained
    /// interior-capital occurrences.
    MixedCase,
}

impl SubstrateId {
    /// Every substrate id, declaration order — the exhaustive iteration source
    /// the registry-completeness tests walk.
    #[allow(dead_code)] // registry-completeness tests + future multi-substrate iteration
    pub(crate) const ALL: &'static [SubstrateId] =
        &[
        SubstrateId::Spacing,
        SubstrateId::Adjacency,
        SubstrateId::RepeatedRun,
        SubstrateId::PunctOnly,
        SubstrateId::MixedScript,
        SubstrateId::Glyph,
        SubstrateId::Proportionality,
        SubstrateId::DuplicateWord,
        SubstrateId::Casing,
        SubstrateId::MixedCase,
    ];

    /// This id's row in the drive-phase probe table — its position in
    /// [`ALL`](Self::ALL), which is declaration order.
    #[cfg(feature = "bench-probes")]
    const fn row(self) -> usize {
        self as usize
    }
}

/// The rebase base for one chapter of one book, during materialization: the
/// global index its verse 0 sits at, taken from the layout position the
/// contribution's chapter was folded from.
///
/// **Why the pairing is positional.** A drive hands `update_book` exactly the
/// layout's ordered chapter tokens; `update_book` keeps one reduced result per
/// position in that order, whether replayed or reused; `fold_book` folds those
/// reduced chapters in that order into the book's contribution. So contribution
/// position `i` is layout position `i`, for every substrate, by construction —
/// and materialization runs only after the drive has brought every book in the
/// current layout up to date, so there is no such thing as a contribution
/// chapter the layout does not have.
///
/// The token equality is nonetheless *asserted*, not merely documented, because
/// a mis-paired chapter would emit findings at wrong verse addresses — silent
/// corruption, not a slowdown. One short `&str` compare per chapter is a few
/// percent of the `(book, chapter)` linear scan this replaces, so the proof is
/// affordable at full strength.
pub(crate) fn chapter_base(layout: &crate::corpus::ChapterLayout, token: &str) -> crate::KeyIdx {
    assert_eq!(
        &*layout.chapter, token,
        "materialization paired a contribution chapter with the wrong layout \
         chapter — stop and report: the drive's ordered-token contract broke"
    );
    crate::KeyIdx::from_usize(layout.range.start)
}

// ─────────────────────────────────────────────────────────────────────
// The per-substrate × per-phase drive probe (`bench-probes` only).
//
// A drive's six phases are structurally distinct pieces of work with different
// removability: planning and reduction are duplicated per substrate over the
// whole layout and could in principle be shared or skipped, while judge-key
// discovery, judging and materialization are whole-corpus by construction and
// only a delta-consumption design removes them. Attributing a measured fixed
// cost to the drive as a whole cannot tell those apart, so the probe separates
// them. Off the feature every type here is a ZST and every method is an empty
// inlined body — the shipped path carries no timers.
// ─────────────────────────────────────────────────────────────────────

/// The phases of one `drive_*` call, in the order a drive performs them.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DrivePhase {
    /// Walk the whole layout, build every chapter's `ObservationInputStamp`,
    /// ask the cache which chapters are dirty, collect the map work.
    Plan,
    /// The ordered chapter-map seam: `map_chapter` over the dirty chapters plus
    /// slotting the results back into caller order.
    Map,
    /// `update_book` for every book — the ordered reduction-to-convergence
    /// replay, the book fold, and the corpus-aggregate replacement.
    Reduce,
    /// Discovering/reconstructing the judge key set (for a substrate that walks
    /// its retained sites to name it, or builds a corpus model).
    Keys,
    /// The per-key `judge` calls.
    Judge,
    /// Walking every book's retained sites and emitting findings.
    Materialize,
}

/// Row labels for [`drive_phase_table`] — `SubstrateId::ALL` order. Sized from
/// `SubstrateId::ALL` so a new substrate cannot silently fall off the table's
/// bottom row; `substrate_names_cover_every_id` pins the pairing.
#[cfg(feature = "bench-probes")]
pub const SUBSTRATE_NAMES: [&str; SubstrateId::ALL.len()] = [
    "spacing",
    "adjacency",
    "repeated-run",
    "punct-only",
    "mixed-script",
    "glyph",
    "proportionality",
    "duplicate-word",
    "casing",
    "mixed-case",
];

/// Column labels for [`drive_phase_table`] — [`DrivePhase`] declaration order.
#[cfg(feature = "bench-probes")]
pub const DRIVE_PHASE_NAMES: [&str; 6] = ["plan", "map", "reduce", "keys", "judge", "materialize"];

#[cfg(feature = "bench-probes")]
type DrivePhaseTable = [[std::time::Duration; 6]; SubstrateId::ALL.len()];

#[cfg(feature = "bench-probes")]
thread_local! {
    static DRIVE_PHASES: std::cell::Cell<DrivePhaseTable> =
        const { std::cell::Cell::new([[std::time::Duration::ZERO; 6]; SubstrateId::ALL.len()]) };
}

/// Zero the drive-phase table. `transition` calls this once per analyze so a
/// substrate that did not run this call reads as zero rather than as a stale
/// row from the previous one.
#[cfg(feature = "bench-probes")]
pub(crate) fn reset_drive_phases() {
    DRIVE_PHASES.with(|t| t.set([[std::time::Duration::ZERO; 6]; SubstrateId::ALL.len()]));
}

/// The most recent analyze's per-substrate × per-phase split on this thread.
#[cfg(feature = "bench-probes")]
pub fn drive_phase_table() -> DrivePhaseTable {
    DRIVE_PHASES.with(std::cell::Cell::get)
}

/// One drive's phase timer: [`mark`](DriveProbe::mark) closes the phase that
/// ended and opens the next. Accumulates rather than assigns, so a drive that
/// interleaves a phase (casing judges inside materialization) can mark the same
/// phase more than once.
#[cfg(feature = "bench-probes")]
pub(crate) struct DriveProbe {
    row: usize,
    last: std::time::Instant,
}

#[cfg(feature = "bench-probes")]
impl DriveProbe {
    pub(crate) fn new(id: SubstrateId) -> Self {
        DriveProbe {
            row: id.row(),
            last: std::time::Instant::now(),
        }
    }

    pub(crate) fn mark(&mut self, phase: DrivePhase) {
        let now = std::time::Instant::now();
        let elapsed = now - self.last;
        self.last = now;
        let row = self.row;
        DRIVE_PHASES.with(|t| {
            let mut table = t.get();
            table[row][phase as usize] += elapsed;
            t.set(table);
        });
    }
}

#[cfg(not(feature = "bench-probes"))]
pub(crate) struct DriveProbe;

#[cfg(not(feature = "bench-probes"))]
impl DriveProbe {
    #[inline(always)]
    pub(crate) fn new(_id: SubstrateId) -> Self {
        DriveProbe
    }

    #[inline(always)]
    pub(crate) fn mark(&mut self, _phase: DrivePhase) {}
}

/// A verse-slice view of one chapter, handed to
/// [`map_chapter`](ObservationSubstrate::map_chapter). It is the whole map
/// input: a chapter's text, addressed chapter-locally. It carries no book
/// position, no neighbour, and no config — mapping cannot depend on any of them.
pub(crate) struct ChapterView<'a> {
    pub(crate) chapter: &'a str,
    /// The chapter's verse texts in presented order; verse `i` is chapter-local
    /// index `i`.
    pub(crate) texts: &'a [String],
    /// The paired reference view, present **only** for a substrate whose closed
    /// registry entry declares a reference input (plan §5.2). The engine does not
    /// hand a substrate source access it did not declare, which is why this is an
    /// `Option` on the view rather than an always-available field: a target-only
    /// mapper cannot read reference text even by accident.
    pub(crate) paired: Option<PairedView<'a>>,
}

/// What a reference-declaring substrate reads beyond the target text: this
/// chapter's own verse keys, and the paired reference chapter's keys and texts.
///
/// The pairing is by `(slug, chapter token)`, and that is sound because a key's
/// chapter token is *parsed from the key* and a chapter run may not reopen: every
/// occurrence of a key string therefore lies inside one chapter run, on both
/// sides. So a target chapter's reference evidence is exactly the reference
/// chapter carrying the same token in the same book — never a wider scope, and
/// never a cross-slug read (plan §17's stop clause).
#[derive(Clone, Copy)]
pub(crate) struct PairedView<'a> {
    pub(crate) keys: &'a [String],
    pub(crate) reference_keys: &'a [String],
    pub(crate) reference_texts: &'a [String],
}

impl<'a> ChapterView<'a> {
    /// A target-only chapter view — the shape every substrate but
    /// `ProportionalitySubstrate` maps from.
    pub(crate) fn target(chapter: &'a str, texts: &'a [String]) -> Self {
        ChapterView {
            chapter,
            texts,
            paired: None,
        }
    }
}

/// Validity stamp for a cached [chapter observation](ObservationSubstrate::ChapterObservation).
/// The observation is a pure function of the chapter's text (+ the substrate's
/// schema and its extraction-only config), so a stamp match is always safe to
/// reuse. **No judging knob and no rule-enabled bit appears here** — that is
/// exactly why a judging-knob change reuses every observation (maps zero
/// chapters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObservationInputStamp {
    /// The substrate's compile-time schema stamp
    /// ([`ObservationSubstrate::SCHEMA_STAMP`]).
    pub(crate) schema_stamp: u64,
    /// The relevant target chapter content hash.
    pub(crate) chapter_hash: u128,
    /// The substrate's extraction-only config fingerprint. Judging knobs are
    /// absent by construction; a substrate with no extraction config (spacing)
    /// carries a constant here.
    pub(crate) extractor_fp: u64,
    /// The paired reference chapter's content hash, or the explicit absent tag,
    /// for a substrate that declares a reference input (plan §5.2). A target-only
    /// substrate carries [`ReferenceStamp::NotDeclared`], so its observations
    /// cannot be invalidated by reference movement — and a declared one's
    /// observations invalidate when the reference chapter's *content* moves, when
    /// the reference disappears, and when a reference appears where there was
    /// none, because all three are distinct values here.
    pub(crate) reference: ReferenceStamp,
}

/// The reference half of an [`ObservationInputStamp`] (plan §5.2's "relevant
/// reference chapter/book hash or explicit absent tag (if declared)").
///
/// Three states, not two: "declared but absent" must be distinguishable from
/// "not declared" so that a reference corpus being *removed* invalidates a
/// source-dependent substrate's observations while leaving every target-only
/// substrate's alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceStamp {
    /// This substrate declares no reference input.
    NotDeclared,
    /// Declared, and no reference chapter pairs with this one (no reference
    /// corpus, or none carrying this `(slug, chapter)`).
    Absent,
    /// Declared, and this is the paired reference chapter's content hash.
    Present(u128),
}

/// Validity stamp for a cached [reduced chapter](ObservationSubstrate::ReducedChapter).
/// A reduction is valid iff its observation is valid AND it was produced from the
/// same entering boundary state. The leaving state is the ordered replay's
/// convergence referee: when a replayed chapter leaves the state it left before,
/// every later cached reduction is already what a full re-reduce would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReducedChapterStamp<B> {
    /// The observation stamp the reduction was produced from.
    pub(crate) observation: ObservationInputStamp,
    /// The boundary state that entered this chapter's reduction.
    pub(crate) entering: B,
    /// The boundary state this chapter's reduction left behind.
    pub(crate) leaving: B,
}

/// A strongly-typed observation substrate (plan §5.2). Compile-time only: the
/// implementing type is a zero-sized marker, its associated types name the
/// evidence, and the four pure operations are the whole behaviour. Never boxed,
/// never `dyn`.
pub(crate) trait ObservationSubstrate {
    /// This substrate's closed-registry id.
    const ID: SubstrateId;
    /// A deterministic stamp bumped whenever the observation/reduction schema
    /// changes — folded into every [`ObservationInputStamp`] so a schema change
    /// invalidates cached observations.
    const SCHEMA_STAMP: u64;

    /// The judge key — one aggregate/verdict per key (spacing: the mark).
    type Key: Clone + Eq + Ord;
    /// The ordered-reduction boundary state carried across chapters. `Default`
    /// is the book-start state (nothing carried in).
    type BoundaryState: Clone + Eq + Default;
    /// One chapter's input-independent observation.
    type ChapterObservation: Clone + Eq;
    /// One chapter's reduced result after its entering boundary state is applied.
    /// `Default` is the empty reduced chapter — the carry sink at book start,
    /// where nothing resolves into a (nonexistent) previous chapter.
    type ReducedChapter: Clone + Eq + Default;
    /// A book's folded contribution to the corpus aggregate.
    type BookContribution: Clone + Eq;
    /// The corpus aggregate the judge reads.
    type CorpusStats: Default;
    /// The substrate's extraction-only config (spacing: `()` — no extraction
    /// knobs). Its fingerprint enters [`ObservationInputStamp`].
    type ExtractorConfig: Clone;
    /// The substrate's judging config (spacing: the four score knobs). Read only
    /// by [`judge`](Self::judge); never enters any stamp.
    type JudgeConfig: Clone;
    /// Shared append-only naming a substrate's map and fold may use to encode
    /// its observations compactly — casing's folded-word interner
    /// ([`crate::interner::WordInterner`]); `()` for a substrate that needs
    /// none.
    ///
    /// It is *naming*, never evidence, and that distinction is what keeps the
    /// map's purity intact: appending to it cannot change what an observation
    /// says, only which integer stands for a word, so it deliberately does not
    /// enter [`ObservationInputStamp`] and cannot invalidate a cached
    /// observation. Reached through `&self` because chapter mapping fans out,
    /// hence `Sync`.
    type Symbols: Default + Sync;
    /// A per-key verdict, materialised into findings at each site tagged with
    /// that key.
    type EntryOutcome;

    /// The extraction-config fingerprint that enters [`ObservationInputStamp`].
    /// A substrate with no extraction config returns a constant.
    fn extractor_fp(extractor: &Self::ExtractorConfig) -> u64;

    /// Map one chapter to its input-independent observation. Predecessor-free:
    /// no boundary state, no judging knob, no book position.
    fn map_chapter(
        chapter: &ChapterView<'_>,
        extractor: &Self::ExtractorConfig,
        symbols: &Self::Symbols,
    ) -> Self::ChapterObservation;

    /// The opaque chapter token that owns `entering`'s carried cross-seam
    /// contribution, if any — the chapter whose reduced result a resolution
    /// should fold into. `None` when nothing is carried. The generic driver
    /// uses this to route [`reduce_chapter`](Self::reduce_chapter)'s `carry_out`
    /// to the correct earlier chapter (a carried item can skip an all-empty
    /// chapter, so the owner is not always the immediate predecessor).
    fn pending_owner(state: &Self::BoundaryState) -> Option<&str>;

    /// The ordered-reduction step: apply the chapter's entering boundary state
    /// to its observation, producing this chapter's reduced result and the state
    /// leaving the chapter. Left-to-right carry fold — never reads the next
    /// chapter. `carry_out` is the reduced result of the chapter that owns
    /// `entering`'s carried contribution (a cross-seam mark whose far neighbour
    /// lands in THIS chapter); this chapter's data resolves it and folds the
    /// resolution into `carry_out`.
    fn reduce_chapter(
        observation: &Self::ChapterObservation,
        entering: &Self::BoundaryState,
        carry_out: &mut Self::ReducedChapter,
    ) -> (Self::ReducedChapter, Self::BoundaryState);

    /// Resolve a book's final dangling boundary state (the book edge has no
    /// neighbour across the seam), folding any residual contribution into
    /// `carry_out` — the reduced result of the chapter that owns it (routed by
    /// [`pending_owner`](Self::pending_owner)).
    fn finish_book(leaving: &Self::BoundaryState, carry_out: &mut Self::ReducedChapter);

    /// Fold a book's ordered reduced chapters into its corpus contribution.
    /// Reads `symbols` to resolve whatever its observations encoded through them
    /// — the book table's keys are owned words, in a canonical order the judge's
    /// arithmetic depends on, so the fold is where symbols turn back into text.
    fn fold_book(
        reduced: &[Self::ReducedChapter],
        symbols: &Self::Symbols,
    ) -> Self::BookContribution;

    /// Replace a book's contribution in the corpus aggregate, returning the
    /// exact set of keys whose corpus aggregate changed (the stats-delta keys).
    /// Never reads a judging knob.
    fn replace_book_in_corpus_stats(
        stats: &mut Self::CorpusStats,
        slug: &str,
        old: Option<&Self::BookContribution>,
        new: Option<&Self::BookContribution>,
    ) -> Vec<Self::Key>;

    /// Judge one key against the corpus aggregate and this substrate's judging
    /// config, producing its per-key outcome. Pure: never mutates `stats`.
    fn judge(
        judge: &Self::JudgeConfig,
        key: &Self::Key,
        stats: &Self::CorpusStats,
    ) -> Self::EntryOutcome;
}

/// The resident cache for one substrate `S` (plan §5). Holds, per book, every
/// chapter's observation + reduced result behind their validity stamps, the
/// book's folded contribution, and the corpus aggregate the judge reads.
/// `Galley`-owned on the resident path; transient on the one-shot path.
///
/// Independent of the shared-prep and finding sections: its entries are keyed by
/// the substrate's own stamps, so a judging-knob change (which clears prep)
/// leaves it entirely valid.
pub(crate) struct SubstrateCache<S: ObservationSubstrate> {
    books: FxHashMap<Box<str>, SubstrateBook<S>>,
    corpus_stats: S::CorpusStats,
    /// Observability (`test-probes`): chapters mapped / reduced, and keys judged
    /// on the most recent analyze — the substrate half of the work probes Step 3
    /// asserts against.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) mapped: usize,
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) reduced: usize,
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) judged: usize,
    /// Which single map grain this substrate's chapter map used on the most
    /// recent analyze.
    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) map_route: &'static str,
}

/// One book's resident substrate state: its ordered chapters and its folded
/// contribution to the corpus aggregate.
struct SubstrateBook<S: ObservationSubstrate> {
    chapters: Vec<SubstrateChapter<S>>,
    /// Opaque chapter token → position in `chapters`. Tokens are unique within a
    /// book (a chapter run may not reopen), so this is a function. It makes the
    /// driver's per-chapter reuse decision O(1) rather than a scan of the book.
    by_token: FxHashMap<Box<str>, usize>,
    contribution: S::BookContribution,
}

/// One chapter's resident substrate state: its opaque token, the observation and
/// its input stamp, and the reduced result and its stamp.
struct SubstrateChapter<S: ObservationSubstrate> {
    token: Box<str>,
    input_stamp: ObservationInputStamp,
    observation: S::ChapterObservation,
    reduced_stamp: ReducedChapterStamp<S::BoundaryState>,
    reduced: S::ReducedChapter,
}

/// A book's cached state taken apart into parallel columns, indexed by the
/// chapter's position in the **cached** (previous) order. The replay driver
/// `take()`s from the `Option` columns, so an unchanged chapter hands its
/// observation and reduced result to the new state by move — a warm analyze
/// copies neither.
struct OldColumns<S: ObservationSubstrate> {
    tokens: Vec<Box<str>>,
    input_stamps: Vec<ObservationInputStamp>,
    observations: Vec<Option<S::ChapterObservation>>,
    reduced_stamps: Vec<Option<ReducedChapterStamp<S::BoundaryState>>>,
    reduced: Vec<Option<S::ReducedChapter>>,
    by_token: FxHashMap<Box<str>, usize>,
}

impl<S: ObservationSubstrate> OldColumns<S> {
    /// The empty previous state — a book this substrate has never seen. Every
    /// position then reads as changed, so the driver maps and reduces the whole
    /// book (the cold path).
    fn empty() -> Self {
        OldColumns {
            tokens: Vec::new(),
            input_stamps: Vec::new(),
            observations: Vec::new(),
            reduced_stamps: Vec::new(),
            reduced: Vec::new(),
            by_token: FxHashMap::default(),
        }
    }

    fn take_apart(book: SubstrateBook<S>) -> (Self, S::BookContribution) {
        let mut cols = Self::empty();
        cols.by_token = book.by_token;
        for c in book.chapters {
            cols.tokens.push(c.token);
            cols.input_stamps.push(c.input_stamp);
            cols.observations.push(Some(c.observation));
            cols.reduced_stamps.push(Some(c.reduced_stamp));
            cols.reduced.push(Some(c.reduced));
        }
        (cols, book.contribution)
    }

    /// Rebuild the book exactly as it was — the whole-book-unchanged path, which
    /// must put back what it took apart without reducing anything. Step 1 already
    /// moved every observation out into `observations` (position-aligned here,
    /// because this path runs only when no position changed), so they come back
    /// from there.
    fn reassemble(
        self,
        observations: Vec<S::ChapterObservation>,
        contribution: S::BookContribution,
    ) -> SubstrateBook<S> {
        let chapters = self
            .tokens
            .into_iter()
            .zip(self.input_stamps)
            .zip(observations)
            .zip(self.reduced_stamps)
            .zip(self.reduced)
            .map(
                |((((token, input_stamp), observation), reduced_stamp), reduced)| SubstrateChapter {
                    token,
                    input_stamp,
                    observation,
                    reduced_stamp: reduced_stamp.expect("unchanged book retains every stamp"),
                    reduced: reduced.expect("unchanged book retains every reduced chapter"),
                },
            )
            .collect();
        SubstrateBook {
            chapters,
            by_token: self.by_token,
            contribution,
        }
    }
}

impl<S: ObservationSubstrate> Default for SubstrateCache<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: ObservationSubstrate> SubstrateCache<S> {
    pub(crate) fn new() -> Self {
        Self {
            books: FxHashMap::default(),
            corpus_stats: S::CorpusStats::default(),
            #[cfg(any(test, feature = "test-probes"))]
            mapped: 0,
            #[cfg(any(test, feature = "test-probes"))]
            reduced: 0,
            #[cfg(any(test, feature = "test-probes"))]
            judged: 0,
            #[cfg(any(test, feature = "test-probes"))]
            map_route: "serial",
        }
    }

    /// Drop every book and reset the aggregate — the last-consumer-disabled
    /// invalidation (plan §5.3): a substrate with no active consumer keeps no
    /// products, so edits while it is inactive do no work for it.
    pub(crate) fn clear(&mut self) {
        self.books.clear();
        self.corpus_stats = S::CorpusStats::default();
    }

    /// Drop one book's contribution and chapters, updating the corpus aggregate
    /// so a removed book cannot keep contributing (plan §7.1 "remove book").
    pub(crate) fn remove_book(&mut self, slug: &str) {
        if let Some(book) = self.books.remove(slug) {
            S::replace_book_in_corpus_stats(
                &mut self.corpus_stats,
                slug,
                Some(&book.contribution),
                None,
            );
        }
    }

    pub(crate) fn corpus_stats(&self) -> &S::CorpusStats {
        &self.corpus_stats
    }

    #[cfg(any(test, feature = "test-probes"))]
    pub(crate) fn reset_probes(&mut self) {
        self.mapped = 0;
        self.reduced = 0;
        self.judged = 0;
        self.map_route = "serial";
    }

    /// A book's folded contribution, for materialization. `None` when the book
    /// is absent from this substrate's cache.
    pub(crate) fn book_contribution(&self, slug: &str) -> Option<&S::BookContribution> {
        self.books.get(slug).map(|b| &b.contribution)
    }

    /// Whether this substrate already holds a current observation for
    /// `(slug, token)` at `stamp`. This is the planning pass's question, and it is
    /// answered by the same token-keyed predicate [`update_book`] reuses by — so a
    /// plan and the driver can never disagree about which chapters are dirty.
    pub(crate) fn observation_is_current(
        &self,
        slug: &str,
        token: &str,
        stamp: &ObservationInputStamp,
    ) -> bool {
        self.books.get(slug).is_some_and(|b| {
            b.by_token
                .get(token)
                .is_some_and(|&i| b.chapters[i].input_stamp == *stamp)
        })
    }

    /// Bring one book up to date from its ordered chapters — the **ordered
    /// reduction-to-convergence driver** (plan §5.4). Returns the stats-delta
    /// keys the book's new contribution produced.
    ///
    /// Five steps:
    ///
    /// 1. map only the chapters whose observation input stamp changed, each into
    ///    its caller-order slot. Reuse is keyed by the chapter's **opaque token**,
    ///    not its position: [`map_chapter`](ObservationSubstrate::map_chapter) is
    ///    predecessor-free, so a chapter that merely moved carries its
    ///    observation with it. A judging-knob change leaves every stamp valid and
    ///    therefore maps nothing;
    /// 2. take the cached boundary state entering the earliest changed chapter
    ///    (the default state at book start);
    /// 3. reduce that chapter and compare the state it now leaves against the
    ///    state it left before;
    /// 4. while they differ, reduce the next chapter's **cached observation** with
    ///    the new state — a changed carry never re-walks a chapter's text; and
    /// 5. stop as soon as a replayed chapter leaves the state it left before. The
    ///    book's end is the correctness fallback; there is no replay cap, because
    ///    a cap would be a silent behavioural cutoff rather than a computed one.
    ///
    /// Convergence is sound because reduction is a left-to-right fold: if the
    /// state leaving chapter `k` is unchanged and the chapters after `k` are
    /// untouched, each of their cached reduced results was produced from exactly
    /// the inputs a full re-reduce would hand it.
    ///
    /// `chapters` is the book's ordered `(opaque token, ObservationInputStamp)`
    /// pairs; `map` is called only for a chapter whose observation is stale.
    ///
    /// **The tokens are borrowed, not owned.** A drive's planning pass builds
    /// this slice from the corpus layout, whose chapter tokens already outlive
    /// the call; owning them here would allocate one `Box<str>` per chapter per
    /// substrate per analyze (~1,189 × 6 for a resident Bible) purely to be
    /// dropped at the end of the drive. Ownership is taken exactly once, and
    /// only where the persistent cache entry is actually (re)built: the
    /// `chapters_out`/`by_token` construction at the bottom of this function,
    /// which the whole-book-unchanged early-out and the nothing-moved
    /// reassembly both skip.
    pub(crate) fn update_book(
        &mut self,
        slug: &str,
        chapters: &[(&str, ObservationInputStamp)],
        symbols: &S::Symbols,
        mut map: impl FnMut(usize) -> S::ChapterObservation,
    ) -> Vec<S::Key> {
        let n = chapters.len();

        // ── Step 0: the whole-book-unchanged early-out. Every position holds the
        // same chapter token mapped from the same input stamp, so this is exactly
        // the case step 2 below detects as `first_changed.is_none() && !structural`
        // — same positional token/stamp comparison, decided before anything is
        // disassembled instead of after. The existing path reaches the identical
        // answer (`Vec::new()`, the book put back byte-for-byte as it was) but pays
        // for it: the book is removed from the map, split into five parallel
        // columns, every observation moved out through a token hash lookup, a
        // token->position map built, and the whole book reassembled and re-inserted
        // under a freshly allocated key. None of that survives the call, and 1,188
        // of a resident Bible's 1,189 chapters take this path on a one-chapter
        // edit, per substrate.
        //
        // Reuse cannot be a *positional* decision anywhere else in this driver —
        // a chapter that merely moved must carry its observation with it, which is
        // why step 1 is token-keyed. It is sound here only because nothing moved:
        // equal length plus equal token at every position is the definition.
        if let Some(book) = self.books.get(slug)
            && book.chapters.len() == n
            && book
                .chapters
                .iter()
                .zip(chapters)
                .all(|(cached, (token, stamp))| {
                    *cached.token == **token && cached.input_stamp == *stamp
                })
        {
            return Vec::new();
        }

        let (mut old, old_contribution) = match self.books.remove(slug) {
            Some(book) => {
                let (cols, contribution) = OldColumns::take_apart(book);
                (cols, Some(contribution))
            }
            None => (OldColumns::empty(), None),
        };

        // ── Step 1: map the stale chapters into their caller-order slots.
        let mut observations: Vec<S::ChapterObservation> = Vec::with_capacity(n);
        for (k, (token, stamp)) in chapters.iter().enumerate() {
            let reused = old
                .by_token
                .get(*token)
                .copied()
                .filter(|&i| old.input_stamps[i] == *stamp)
                .and_then(|i| old.observations[i].take());
            match reused {
                Some(obs) => observations.push(obs),
                None => {
                    #[cfg(any(test, feature = "test-probes"))]
                    {
                        self.mapped += 1;
                    }
                    observations.push(map(k));
                }
            }
        }

        // Token → position in the NEW order. A carried cross-seam contribution
        // belongs to an EARLIER chapter, and that owner is not always the
        // immediate predecessor (a carry can cross an all-empty chapter), so the
        // driver resolves the owner by its opaque token.
        let new_pos: FxHashMap<&str, usize> = chapters
            .iter()
            .enumerate()
            .map(|(i, (t, _))| (*t, i))
            .collect();

        // ── Step 2: locate the replay window. A position is changed when the
        // chapter that sat there is gone, is a different chapter, or was mapped
        // from different content.
        let changed = |k: usize| match (old.tokens.get(k), old.input_stamps.get(k)) {
            (Some(token), Some(stamp)) => **token != *chapters[k].0 || *stamp != chapters[k].1,
            _ => true,
        };
        // A different chapter count reshaped the book: positions shifted, and the
        // book edge may now fall on a different dangling state, so the replay
        // runs to the book's end rather than looking for convergence.
        let structural = old.tokens.len() != n;
        let first_changed = (0..n).find(|&k| changed(k));
        let last_changed = (0..n).rev().find(|&k| changed(k));
        if first_changed.is_none() && !structural {
            // Nothing moved: put the book back exactly as it was. Zero reductions,
            // zero aggregate delta — this is the path a judging-knob change takes.
            let book = old.reassemble(
                observations,
                old_contribution.expect("an unchanged book was already resident"),
            );
            self.books.insert(Box::from(slug), book);
            return Vec::new();
        }
        let must_replay_through = if structural {
            n.saturating_sub(1)
        } else {
            last_changed.expect("a changed book has a last changed position")
        };

        // Walk the window's start back to the chapter that OWNS any cross-seam
        // contribution carried into it. That owner's reduced result is rebuilt
        // from nothing, and the resolution folds into it again; starting later
        // would fold the same contribution into a cached result that already
        // holds it. Each hop strictly decreases the index, so this terminates.
        let (start, mut carry) = {
            let entering_at = |k: usize| -> S::BoundaryState {
                if k == 0 {
                    return S::BoundaryState::default();
                }
                old.reduced_stamps[k - 1]
                    .as_ref()
                    .expect("the position before the replay window is an unchanged chapter")
                    .leaving
                    .clone()
            };
            let mut start = first_changed.unwrap_or(0);
            loop {
                let entering = entering_at(start);
                match S::pending_owner(&entering).and_then(|t| new_pos.get(t).copied()) {
                    Some(owner) if owner < start => start = owner,
                    _ => break,
                }
            }
            let carry = entering_at(start);
            (start, carry)
        };

        // ── Steps 3–5: replay the ordered reduction until the boundary state
        // converges. Positions before the window keep their cached reduced
        // results untouched (nothing the replay can produce reaches back past its
        // own start).
        let mut reduced: Vec<Option<S::ReducedChapter>> = (0..n)
            .map(|k| if k < start { old.reduced[k].take() } else { None })
            .collect();
        let mut stamps: Vec<Option<ReducedChapterStamp<S::BoundaryState>>> = vec![None; n];
        let mut converged_at: Option<usize> = None;
        for k in start..n {
            let entering = carry.clone();
            let owner = S::pending_owner(&entering)
                .and_then(|t| new_pos.get(t).copied())
                .filter(|&o| o >= start);
            let (this, leaving) = match owner.and_then(|o| reduced[o].as_mut()) {
                // `carry_out` is the owning chapter's reduced result; this
                // chapter's data resolves the carried contribution into it.
                Some(carry_out) => S::reduce_chapter(&observations[k], &entering, carry_out),
                None => {
                    let mut sink = S::ReducedChapter::default();
                    S::reduce_chapter(&observations[k], &entering, &mut sink)
                }
            };
            #[cfg(any(test, feature = "test-probes"))]
            {
                self.reduced += 1;
            }
            stamps[k] = Some(ReducedChapterStamp {
                observation: chapters[k].1,
                entering,
                leaving: leaving.clone(),
            });
            reduced[k] = Some(this);
            carry = leaving;

            // Convergence: this chapter leaves the state it left before, the same
            // chapter sits here, and every later position is untouched — so every
            // later cached reduced result is already what a full re-reduce would
            // produce. A contribution still carried and still owned by a chapter
            // this replay rebuilt is NOT converged: its resolution lives in a
            // later chapter and has to fold in again.
            let dangling = S::pending_owner(&carry)
                .and_then(|t| new_pos.get(t).copied())
                .is_some_and(|o| o >= start);
            let same_chapter_here = old.tokens.get(k).is_some_and(|t| **t == *chapters[k].0);
            let left_as_before = old
                .reduced_stamps
                .get(k)
                .and_then(Option::as_ref)
                .is_some_and(|s| s.leaving == carry);
            if !structural
                && k >= must_replay_through
                && same_chapter_here
                && !dangling
                && left_as_before
            {
                converged_at = Some(k);
                break;
            }
        }

        // Book edge: no neighbour across the final seam, so a still-dangling state
        // resolves into its owning chapter. Only when the replay actually reached
        // the book's end — a replay that converged earlier left the cached
        // book-edge resolution in place, inside a cached reduced result.
        if converged_at.is_none_or(|k| k + 1 == n)
            && let Some(owner) = S::pending_owner(&carry)
                .and_then(|t| new_pos.get(t).copied())
                .filter(|&o| o >= start)
            && let Some(carry_out) = reduced[owner].as_mut()
        {
            S::finish_book(&carry, carry_out);
        }

        // Past the convergence point every cached reduction stands.
        if let Some(m) = converged_at {
            for (slot, cached) in reduced.iter_mut().zip(old.reduced.iter_mut()).skip(m + 1) {
                *slot = cached.take();
            }
        }

        let reduced: Vec<S::ReducedChapter> = reduced
            .into_iter()
            .map(|r| r.expect("every position is either replayed or reused"))
            .collect();
        let stamps: Vec<ReducedChapterStamp<S::BoundaryState>> = stamps
            .into_iter()
            .enumerate()
            .map(|(k, s)| {
                s.or_else(|| old.reduced_stamps[k].take())
                    .expect("every position is either replayed or reused")
            })
            .collect();

        let new_contribution = S::fold_book(&reduced, symbols);
        let delta = S::replace_book_in_corpus_stats(
            &mut self.corpus_stats,
            slug,
            old_contribution.as_ref(),
            Some(&new_contribution),
        );

        let chapters_out: Vec<SubstrateChapter<S>> = chapters
            .iter()
            .zip(observations)
            .zip(reduced)
            .zip(stamps)
            .map(
                |((((token, stamp), observation), reduced), reduced_stamp)| SubstrateChapter {
                    // The one ownership point: this entry outlives the drive.
                    token: Box::from(*token),
                    input_stamp: *stamp,
                    observation,
                    reduced_stamp,
                    reduced,
                },
            )
            .collect();
        let by_token = chapters_out
            .iter()
            .enumerate()
            .map(|(i, c)| (c.token.clone(), i))
            .collect();
        self.books.insert(
            Box::from(slug),
            SubstrateBook {
                chapters: chapters_out,
                by_token,
                contribution: new_contribution,
            },
        );
        delta
    }
}

/// The active substrate set for a coalesced config: which substrates have at
/// least one enabled consumer. Computed once before mapping, from the closed
/// registry and the final config — never per event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ActiveSubstrates {
    pub(crate) spacing: bool,
    pub(crate) adjacency: bool,
    pub(crate) repeated_run: bool,
    pub(crate) punct_only: bool,
    pub(crate) mixed_script: bool,
    pub(crate) glyph: bool,
    pub(crate) proportionality: bool,
    pub(crate) duplicate_word: bool,
    pub(crate) casing: bool,
    pub(crate) mixed_case: bool,
}

impl ActiveSubstrates {
    /// Derive the active set from the final coalesced config: a substrate is
    /// active iff any of its consumers is enabled (the closed registry below).
    pub(crate) fn from_config(config: &Config) -> Self {
        let any = |rules: &[RuleId]| rules.iter().any(|&r| config.is_enabled(r));
        Self {
            spacing: any(spacing_consumers()),
            adjacency: any(adjacency_consumers()),
            repeated_run: any(repeated_run_consumers()),
            punct_only: any(punct_only_consumers()),
            mixed_script: any(mixed_script_consumers()),
            glyph: any(glyph_consumers()),
            proportionality: any(proportionality_consumers()),
            duplicate_word: any(duplicate_word_consumers()),
            casing: any(casing_consumers()),
            mixed_case: any(mixed_case_consumers()),
        }
    }

    #[allow(dead_code)] // exhaustive per-id accessor; the completeness tests walk it
    pub(crate) fn is_active(&self, id: SubstrateId) -> bool {
        match id {
            SubstrateId::Spacing => self.spacing,
            SubstrateId::Adjacency => self.adjacency,
            SubstrateId::RepeatedRun => self.repeated_run,
            SubstrateId::PunctOnly => self.punct_only,
            SubstrateId::MixedScript => self.mixed_script,
            SubstrateId::Glyph => self.glyph,
            SubstrateId::Proportionality => self.proportionality,
            SubstrateId::DuplicateWord => self.duplicate_word,
            SubstrateId::Casing => self.casing,
            SubstrateId::MixedCase => self.mixed_case,
        }
    }
}

/// The closed registry: which rules consume the spacing substrate. `spacing`'s
/// sole consumer today is `punct.spacing-anomaly`; a future second consumer is
/// added here and the completeness tests keep the registry honest.
pub(crate) fn spacing_consumers() -> &'static [RuleId] {
    &[RuleId::PunctuationSpacingAnomaly]
}

/// The closed registry: the adjacency substrate's sole consumer.
pub(crate) fn adjacency_consumers() -> &'static [RuleId] {
    &[RuleId::PunctuationAdjacencyAnomaly]
}

/// The closed registry: the repeated-run substrate's sole consumer.
pub(crate) fn repeated_run_consumers() -> &'static [RuleId] {
    &[RuleId::RepeatedCharacterRun]
}

/// The closed registry: the punct-only substrate's sole consumer.
pub(crate) fn punct_only_consumers() -> &'static [RuleId] {
    &[RuleId::PunctOnlyToken]
}

/// The closed registry: the mixed-script substrate's sole consumer.
pub(crate) fn mixed_script_consumers() -> &'static [RuleId] {
    &[RuleId::MixedScriptInToken]
}

/// The closed registry: the glyph substrate's sole consumer.
pub(crate) fn glyph_consumers() -> &'static [RuleId] {
    &[RuleId::RareGlyph]
}

/// The closed registry: the proportionality substrate's sole consumer — and the
/// only rule in the whole registry whose `InputDependency` is
/// `TargetAndReferenceSilentWhenAbsent`.
pub(crate) fn proportionality_consumers() -> &'static [RuleId] {
    &[RuleId::ProjectLengthRatio]
}

/// The closed registry: the duplicate-word substrate's sole consumer.
pub(crate) fn duplicate_word_consumers() -> &'static [RuleId] {
    &[RuleId::DuplicateWord]
}

/// The closed registry: the casing substrate's two consumers. Either may be
/// disabled while the other keeps the shared substrate alive.
pub(crate) fn casing_consumers() -> &'static [RuleId] {
    &[
        RuleId::SentenceInitialLowercase,
        RuleId::InconsistentWordCasing,
    ]
}

/// The closed registry: the mixed-case substrate's sole consumer.
pub(crate) fn mixed_case_consumers() -> &'static [RuleId] {
    &[RuleId::MixedCaseWord]
}

/// The consumers of a substrate by id — the exhaustive closed match the
/// completeness tests walk.
#[allow(dead_code)] // registry-completeness tests + future multi-substrate iteration
pub(crate) fn consumers_of(id: SubstrateId) -> &'static [RuleId] {
    match id {
        SubstrateId::Spacing => spacing_consumers(),
        SubstrateId::Adjacency => adjacency_consumers(),
        SubstrateId::RepeatedRun => repeated_run_consumers(),
        SubstrateId::PunctOnly => punct_only_consumers(),
        SubstrateId::MixedScript => mixed_script_consumers(),
        SubstrateId::Glyph => glyph_consumers(),
        SubstrateId::Proportionality => proportionality_consumers(),
        SubstrateId::DuplicateWord => duplicate_word_consumers(),
        SubstrateId::Casing => casing_consumers(),
        SubstrateId::MixedCase => mixed_case_consumers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each substrate type's `ID` const matches its registry id — the typed
    /// cache slot and the closed enum name the same substrate.
    #[test]
    fn substrate_ids_pair_with_the_registry() {
        assert_eq!(
            <crate::signals::punctuation::SpacingSubstrate as ObservationSubstrate>::ID,
            SubstrateId::Spacing
        );
        assert_eq!(
            <crate::signals::punctuation::AdjacencySubstrate as ObservationSubstrate>::ID,
            SubstrateId::Adjacency
        );
        assert_eq!(
            <crate::signals::lexical::RepeatedRunSubstrate as ObservationSubstrate>::ID,
            SubstrateId::RepeatedRun
        );
        assert_eq!(
            <crate::signals::lexical::PunctOnlySubstrate as ObservationSubstrate>::ID,
            SubstrateId::PunctOnly
        );
        assert_eq!(
            <crate::signals::script_mixing::MixedScriptSubstrate as ObservationSubstrate>::ID,
            SubstrateId::MixedScript
        );
        assert_eq!(
            <crate::signals::rare_glyph::GlyphSubstrate as ObservationSubstrate>::ID,
            SubstrateId::Glyph
        );
        assert_eq!(
            <crate::signals::proportionality::ProportionalitySubstrate as ObservationSubstrate>::ID,
            SubstrateId::Proportionality
        );
        assert_eq!(
            <crate::signals::lexical::DuplicateWordSubstrate as ObservationSubstrate>::ID,
            SubstrateId::DuplicateWord
        );
        assert_eq!(
            <crate::signals::casing::CasingSubstrate as ObservationSubstrate>::ID,
            SubstrateId::Casing
        );
        assert_eq!(
            <crate::signals::mixed_case::MixedCaseSubstrate as ObservationSubstrate>::ID,
            SubstrateId::MixedCase
        );
    }

    /// Every substrate id has at least one consumer, and the active-set fields
    /// cover every id — the registry is exhaustive over `SubstrateId::ALL`.
    #[test]
    fn registry_covers_every_substrate() {
        for &id in SubstrateId::ALL {
            assert!(
                !consumers_of(id).is_empty(),
                "{id:?} has no consumer — a substrate with no consumer is dead"
            );
            // `is_active` matches exhaustively, so this compiles only if every
            // id is handled; assert every field reads through for its own id.
            let all_on = ActiveSubstrates {
                spacing: true,
                adjacency: true,
                repeated_run: true,
                punct_only: true,
                mixed_script: true,
                glyph: true,
                proportionality: true,
                duplicate_word: true,
                casing: true,
                mixed_case: true,
            };
            assert!(all_on.is_active(id), "{id:?} has no active-set field");
            assert!(!ActiveSubstrates::default().is_active(id));
        }
    }

    /// The drive-probe row labels pair with `SubstrateId::ALL` position for
    /// position — the table is indexed by `SubstrateId as usize`, so a mislabeled
    /// row would attribute one substrate's cost to another.
    #[cfg(feature = "bench-probes")]
    #[test]
    fn substrate_names_cover_every_id() {
        for (i, &id) in SubstrateId::ALL.iter().enumerate() {
            assert_eq!(i, id as usize, "{id:?} is not at its own ALL position");
        }
        assert_eq!(SUBSTRATE_NAMES.len(), SubstrateId::ALL.len());
    }

    /// Every consumer maps to exactly one substrate — no rule consumes two
    /// substrates (the closed pairing the compiler proves; this pins there is no
    /// accidental double-registration).
    #[test]
    fn each_consumer_belongs_to_one_substrate() {
        let mut seen: Vec<RuleId> = Vec::new();
        for &id in SubstrateId::ALL {
            for &r in consumers_of(id) {
                assert!(!seen.contains(&r), "{r:?} registered under two substrates");
                seen.push(r);
            }
        }
    }

    /// Active-substrate computation reads the final config, not any per-event
    /// state: enabling the sole consumer activates the substrate; disabling it
    /// deactivates.
    #[test]
    fn active_set_follows_the_final_config() {
        let mut cfg = Config::v1_defaults();
        assert!(
            !ActiveSubstrates::from_config(&cfg).spacing,
            "spacing is default-disabled, so its substrate is inactive"
        );
        cfg.rules.insert(RuleId::PunctuationSpacingAnomaly, true);
        assert!(
            ActiveSubstrates::from_config(&cfg).spacing,
            "enabling the sole consumer activates the spacing substrate"
        );
        cfg.rules.insert(RuleId::PunctuationSpacingAnomaly, false);
        assert!(!ActiveSubstrates::from_config(&cfg).spacing);
    }
}

/// The generic driver's own replay tests (plan §12.3), driven by two synthetic
/// substrates whose boundary state is chosen to put the convergence rule under
/// direct control. Spacing's tests exercise the driver on real evidence; these
/// exercise the *algorithm*: where the replay window starts, how far it walks,
/// and that a carry change never re-maps a chapter.
#[cfg(test)]
mod replay {
    use super::*;

    /// Boundary state `()` — the chapter-local shape (a direct rule). Every
    /// chapter leaves the same (empty) state, so a changed chapter always
    /// converges at itself.
    struct Local;

    /// Boundary state `Option<char>` — a carry that a chapter either replaces
    /// (its content's last character) or, when it has no content, passes
    /// straight through. That pass-through is what lets a test place
    /// convergence an arbitrary distance away: a run of empty chapters forwards
    /// a changed carry until the next chapter with content absorbs it.
    struct Carry;

    /// What a chapter reduced to: the state that entered it and its own content.
    /// The entering state is part of the value, so a reduction produced from the
    /// wrong carry cannot compare equal to the right one — which is what makes
    /// "resident equals cold" a real check on the replay window.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Reduced {
        entering: Option<char>,
        content: String,
    }

    fn render(reduced: &[Reduced]) -> String {
        reduced
            .iter()
            .map(|r| format!("<{}|{}>", r.entering.unwrap_or('_'), r.content))
            .collect()
    }

    impl ObservationSubstrate for Local {
        const ID: SubstrateId = SubstrateId::Spacing;
        const SCHEMA_STAMP: u64 = 1;
        type Key = String;
        type BoundaryState = ();
        type ChapterObservation = String;
        type ReducedChapter = Reduced;
        type BookContribution = String;
        type CorpusStats = Vec<(String, String)>;
        type ExtractorConfig = ();
        type JudgeConfig = ();
        type Symbols = ();
        type EntryOutcome = ();

        fn extractor_fp(_: &()) -> u64 {
            0
        }
        fn map_chapter(chapter: &ChapterView<'_>, _: &(), _: &()) -> String {
            chapter.texts.concat()
        }
        fn pending_owner(_: &()) -> Option<&str> {
            None
        }
        fn reduce_chapter(obs: &String, _: &(), _: &mut Reduced) -> (Reduced, ()) {
            (
                Reduced {
                    entering: None,
                    content: obs.clone(),
                },
                (),
            )
        }
        fn finish_book(_: &(), _: &mut Reduced) {}
        fn fold_book(reduced: &[Reduced], _: &()) -> String {
            render(reduced)
        }
        fn replace_book_in_corpus_stats(
            stats: &mut Vec<(String, String)>,
            slug: &str,
            _: Option<&String>,
            new: Option<&String>,
        ) -> Vec<String> {
            stats.retain(|(s, _)| s != slug);
            if let Some(new) = new {
                stats.push((slug.to_string(), new.clone()));
            }
            vec![slug.to_string()]
        }
        fn judge(_: &(), _: &String, _: &Vec<(String, String)>) {}
    }

    impl ObservationSubstrate for Carry {
        const ID: SubstrateId = SubstrateId::Spacing;
        const SCHEMA_STAMP: u64 = 2;
        type Key = String;
        type BoundaryState = Option<char>;
        type ChapterObservation = String;
        type ReducedChapter = Reduced;
        type BookContribution = String;
        type CorpusStats = Vec<(String, String)>;
        type ExtractorConfig = ();
        type JudgeConfig = ();
        type Symbols = ();
        type EntryOutcome = ();

        fn extractor_fp(_: &()) -> u64 {
            0
        }
        fn map_chapter(chapter: &ChapterView<'_>, _: &(), _: &()) -> String {
            chapter.texts.concat()
        }
        fn pending_owner(_: &Option<char>) -> Option<&str> {
            None
        }
        fn reduce_chapter(
            obs: &String,
            entering: &Option<char>,
            _: &mut Reduced,
        ) -> (Reduced, Option<char>) {
            // A chapter with content replaces the carry with its last character;
            // an empty chapter forwards whatever entered it.
            let leaving = obs.chars().last().or(*entering);
            (
                Reduced {
                    entering: *entering,
                    content: obs.clone(),
                },
                leaving,
            )
        }
        fn finish_book(_: &Option<char>, _: &mut Reduced) {}
        fn fold_book(reduced: &[Reduced], _: &()) -> String {
            render(reduced)
        }
        fn replace_book_in_corpus_stats(
            stats: &mut Vec<(String, String)>,
            slug: &str,
            _: Option<&String>,
            new: Option<&String>,
        ) -> Vec<String> {
            stats.retain(|(s, _)| s != slug);
            if let Some(new) = new {
                stats.push((slug.to_string(), new.clone()));
            }
            vec![slug.to_string()]
        }
        fn judge(_: &(), _: &String, _: &Vec<(String, String)>) {}
    }

    /// Drive one book through the generic driver. `chapters` are
    /// `(opaque token, content)`; the stamp's chapter hash is the content's, so
    /// editing a chapter's content is exactly what marks its observation stale.
    fn drive<S>(cache: &mut SubstrateCache<S>, slug: &str, chapters: &[(&str, &str)])
    where
        S: ObservationSubstrate<ChapterObservation = String, ExtractorConfig = (), Symbols = ()>,
    {
        let stamped: Vec<(&str, ObservationInputStamp)> = chapters
            .iter()
            .map(|(token, content)| {
                (
                    *token,
                    ObservationInputStamp {
                        schema_stamp: S::SCHEMA_STAMP,
                        chapter_hash: content
                            .bytes()
                            .fold(1u128, |h, b| h.wrapping_mul(31).wrapping_add(u128::from(b))),
                        extractor_fp: S::extractor_fp(&()),
                        reference: ReferenceStamp::NotDeclared,
                    },
                )
            })
            .collect();
        let texts: Vec<Vec<String>> = chapters
            .iter()
            .map(|(_, c)| vec![(*c).to_string()])
            .collect();
        cache.update_book(slug, &stamped, &(), |i| {
            S::map_chapter(
                &ChapterView::target(chapters[i].0, &texts[i]),
                &(),
                &(),
            )
        });
    }

    /// The resident cache's book contribution must equal a cold build's. This is
    /// the property every replay test asserts alongside its work probes: probes
    /// prove the driver did little, this proves it did enough.
    fn assert_equals_cold<S>(resident: &SubstrateCache<S>, slug: &str, chapters: &[(&str, &str)])
    where
        S: ObservationSubstrate<ChapterObservation = String, ExtractorConfig = (), Symbols = ()>,
        S::BookContribution: std::fmt::Debug,
    {
        let mut cold: SubstrateCache<S> = SubstrateCache::new();
        drive(&mut cold, slug, chapters);
        assert_eq!(
            resident.book_contribution(slug),
            cold.book_contribution(slug),
            "resident replay differs from a cold whole-book build"
        );
    }

    /// The driver takes `&str` chapter tokens but the cache entry it builds must
    /// own them: the planning pass's slice is a per-analyze temporary, while the
    /// entry is resident across calls. Drive a book from tokens whose storage is
    /// dropped immediately afterwards, then ask the cache the planning pass's own
    /// question — a borrowed token in the entry could not answer it.
    #[test]
    fn the_cache_entry_owns_its_chapter_token_though_the_driver_borrows_it() {
        let mut cache: SubstrateCache<Local> = SubstrateCache::new();
        let stamp = {
            let tokens: Vec<String> = vec!["1".to_string(), "2".to_string()];
            let chapters: Vec<(&str, &str)> =
                vec![(tokens[0].as_str(), "aa"), (tokens[1].as_str(), "bb")];
            drive(&mut cache, "GEN", &chapters);
            ObservationInputStamp {
                schema_stamp: Local::SCHEMA_STAMP,
                chapter_hash: "aa"
                    .bytes()
                    .fold(1u128, |h, b| h.wrapping_mul(31).wrapping_add(u128::from(b))),
                extractor_fp: Local::extractor_fp(&()),
                reference: ReferenceStamp::NotDeclared,
            }
        };
        assert!(
            cache.observation_is_current("GEN", "1", &stamp),
            "the resident entry must answer for a token whose caller-side storage is gone"
        );
    }

    /// Boundary state `()`: a changed chapter leaves the same (empty) state, so
    /// the replay stops at the chapter it started on.
    #[test]
    fn empty_state_stops_at_the_changed_chapter() {
        let mut cache: SubstrateCache<Local> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", "cc"), ("4", "dd")];
        drive(&mut cache, "GEN", &cold);
        assert_eq!((cache.mapped, cache.reduced), (4, 4), "cold does every chapter");

        cache.reset_probes();
        let edited = [("1", "aa"), ("2", "BB"), ("3", "cc"), ("4", "dd")];
        drive(&mut cache, "GEN", &edited);
        assert_eq!(cache.mapped, 1, "only the changed chapter is mapped");
        assert_eq!(cache.reduced, 1, "an empty boundary state converges at once");
        assert_equals_cold(&cache, "GEN", &edited);
    }

    /// A changed carry converges at the next chapter: chapter 2's content — and
    /// so the state it leaves — changed, chapter 3 absorbs the new carry and
    /// leaves the same state it left before.
    #[test]
    fn a_changed_carry_converges_at_the_next_chapter() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", "cc"), ("4", "dd")];
        drive(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let edited = [("1", "aa"), ("2", "bx"), ("3", "cc"), ("4", "dd")];
        drive(&mut cache, "GEN", &edited);
        assert_eq!(cache.mapped, 1, "only the changed chapter is mapped");
        assert_eq!(
            cache.reduced, 2,
            "chapter 2 leaves a new carry; chapter 3 absorbs it and converges"
        );
        assert_equals_cold(&cache, "GEN", &edited);
    }

    /// The carry survives several pass-through (empty) chapters, so convergence
    /// lands well beyond the changed chapter — and the intervening chapters are
    /// re-reduced from their CACHED observations, never re-mapped.
    #[test]
    fn a_changed_carry_converges_after_several_chapters() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [
            ("1", "aa"),
            ("2", "bb"),
            ("3", ""),
            ("4", ""),
            ("5", ""),
            ("6", "ff"),
            ("7", "gg"),
        ];
        drive(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let edited = [
            ("1", "aa"),
            ("2", "bx"),
            ("3", ""),
            ("4", ""),
            ("5", ""),
            ("6", "ff"),
            ("7", "gg"),
        ];
        drive(&mut cache, "GEN", &edited);
        assert_eq!(
            cache.mapped, 1,
            "a changed carry never re-maps an unchanged chapter's observation"
        );
        assert_eq!(
            cache.reduced, 5,
            "chapters 2..6 replay; chapter 6 has content, absorbs the carry and converges"
        );
        assert_equals_cold(&cache, "GEN", &edited);
    }

    /// Nothing absorbs the changed carry, so the replay legitimately runs to the
    /// book's end. It still maps exactly the one changed chapter — the plan's
    /// "one changed chapter maps exactly one chapter even when ordered reduction
    /// reaches book end".
    #[test]
    fn a_changed_carry_may_converge_only_at_book_end() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", ""), ("4", ""), ("5", "")];
        drive(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let edited = [("1", "aa"), ("2", "bx"), ("3", ""), ("4", ""), ("5", "")];
        drive(&mut cache, "GEN", &edited);
        assert_eq!(
            cache.mapped, 1,
            "one changed chapter maps exactly one chapter, however far the reduction runs"
        );
        assert_eq!(
            cache.reduced, 4,
            "no later chapter absorbs the carry, so the replay runs to the book's end"
        );
        assert_equals_cold(&cache, "GEN", &edited);
    }

    /// An unchanged re-drive does nothing at all: no map, no reduce. The
    /// whole-book-unchanged path must leave the book byte-for-byte as it was —
    /// this is the step-0 early-out's referee, and
    /// [`a_moved_chapter_is_re_reduced_but_never_re_mapped`] is the proof that it
    /// declines a book whose tokens all still match but sit at new positions.
    #[test]
    fn an_unchanged_book_maps_and_reduces_nothing() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", "cc")];
        drive(&mut cache, "GEN", &cold);
        let after_cold = cache.book_contribution("GEN").cloned();

        cache.reset_probes();
        drive(&mut cache, "GEN", &cold);
        assert_eq!((cache.mapped, cache.reduced), (0, 0));
        assert_eq!(cache.book_contribution("GEN").cloned(), after_cold);
    }

    /// A chapter that merely MOVES carries its observation with it: mapping is
    /// predecessor-free, so a reordered book re-reduces but re-maps nothing.
    #[test]
    fn a_moved_chapter_is_re_reduced_but_never_re_mapped() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", "cc")];
        drive(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let moved = [("1", "aa"), ("3", "cc"), ("2", "bb")];
        drive(&mut cache, "GEN", &moved);
        assert_eq!(cache.mapped, 0, "a moved chapter's observation is position-independent");
        assert!(cache.reduced >= 2, "the reordered suffix is re-reduced");
        assert_equals_cold(&cache, "GEN", &moved);
    }

    /// Chapter insertion and removal reshape the book: the driver must re-reduce
    /// to the book's end (positions shifted and the book edge moved) while still
    /// mapping only genuinely new content.
    #[test]
    fn chapter_insertion_and_removal_stay_equal_to_cold() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        let cold = [("1", "aa"), ("2", "bb"), ("3", "cc")];
        drive(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let inserted = [("1", "aa"), ("1b", "xx"), ("2", "bb"), ("3", "cc")];
        drive(&mut cache, "GEN", &inserted);
        assert_eq!(cache.mapped, 1, "only the inserted chapter is mapped");
        assert_equals_cold(&cache, "GEN", &inserted);

        cache.reset_probes();
        let removed = [("1", "aa"), ("2", "bb")];
        drive(&mut cache, "GEN", &removed);
        assert_eq!(cache.mapped, 0, "removal maps nothing");
        assert_equals_cold(&cache, "GEN", &removed);
    }

    /// The property test (plan §12.6 shape): a resident cache driven through a
    /// deterministic pseudo-random edit sequence equals a cold build at every
    /// step, and never maps a chapter whose content did not change.
    #[test]
    fn resident_equals_cold_under_randomized_edit_sequences() {
        let mut cache: SubstrateCache<Carry> = SubstrateCache::new();
        // Chapters carry content or are empty (pass-through) so the generated
        // edits exercise short, long, and book-end convergence distances.
        let tokens = ["1", "2", "3", "4", "5", "6", "7", "8"];
        let mut contents: Vec<String> =
            tokens.iter().map(|t| format!("{t}x")).collect();
        let mut rng = 0x2545_F491_4F6C_DD1Du64;
        // Seed cold, so every later step measures incremental work only.
        let next = |rng: &mut u64| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            *rng
        };
        {
            let seed: Vec<(&str, &str)> = tokens
                .iter()
                .enumerate()
                .map(|(i, t)| (*t, contents[i].as_str()))
                .collect();
            drive(&mut cache, "GEN", &seed);
        }
        for step in 0..120 {
            let which = (next(&mut rng) % tokens.len() as u64) as usize;
            let kind = next(&mut rng) % 3;
            contents[which] = match kind {
                0 => String::new(),              // becomes a pass-through chapter
                1 => format!("{which}{step}"),   // new content, new trailing char
                _ => format!("{step}{which}"),   // new content, same trailing char
            };
            let chapters: Vec<(&str, &str)> = tokens
                .iter()
                .enumerate()
                .map(|(i, t)| (*t, contents[i].as_str()))
                .collect();
            let before = cache.mapped;
            drive(&mut cache, "GEN", &chapters);
            assert!(
                cache.mapped - before <= 1,
                "step {step}: one edited chapter must map at most one chapter"
            );
            assert_equals_cold(&cache, "GEN", &chapters);
        }
    }

    /// A substrate with **owner-routed carry**: a chapter can leave behind an
    /// unresolved item that belongs to *it*, and a later chapter resolves that
    /// item by folding the resolution back into the owner's reduced result. This
    /// is spacing's real shape (a trailing mark whose neighbour lands in a later
    /// chapter, possibly across an all-empty one), reduced to its essentials —
    /// content ending in `!` buffers an item; content starting with `+` resolves
    /// a carried one.
    struct Owned;

    /// `Owned`'s observation has to carry its own chapter token: the item it
    /// buffers belongs to this chapter, and reduction is the only place that
    /// identity can be attached.
    #[derive(Clone, PartialEq, Eq)]
    struct TokenedObs {
        token: Box<str>,
        content: String,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct OwnedReduced {
        content: String,
        /// Resolutions folded in from LATER chapters. Losing one, or folding one
        /// in twice, changes the book contribution — which is what makes the
        /// replay window's start observable.
        resolutions: usize,
        /// Set when the book edge resolved this chapter's dangling item.
        abstained: bool,
    }

    impl ObservationSubstrate for Owned {
        const ID: SubstrateId = SubstrateId::Spacing;
        const SCHEMA_STAMP: u64 = 3;
        type Key = String;
        type BoundaryState = Option<Box<str>>;
        type ChapterObservation = TokenedObs;
        type ReducedChapter = OwnedReduced;
        type BookContribution = String;
        type CorpusStats = Vec<(String, String)>;
        type ExtractorConfig = ();
        type JudgeConfig = ();
        type Symbols = ();
        type EntryOutcome = ();

        fn extractor_fp(_: &()) -> u64 {
            0
        }
        fn map_chapter(chapter: &ChapterView<'_>, _: &(), _: &()) -> TokenedObs {
            TokenedObs {
                token: Box::from(chapter.chapter),
                content: chapter.texts.concat(),
            }
        }
        fn pending_owner(state: &Option<Box<str>>) -> Option<&str> {
            state.as_deref()
        }
        fn reduce_chapter(
            obs: &TokenedObs,
            entering: &Option<Box<str>>,
            carry_out: &mut OwnedReduced,
        ) -> (OwnedReduced, Option<Box<str>>) {
            let mut pending = entering.clone();
            if pending.is_some() && obs.content.starts_with('+') {
                // Resolve the carried item into the chapter that OWNS it.
                carry_out.resolutions += 1;
                pending = None;
            }
            if obs.content.ends_with('!') {
                pending = Some(obs.token.clone());
            }
            (
                OwnedReduced {
                    content: obs.content.clone(),
                    resolutions: 0,
                    abstained: false,
                },
                pending,
            )
        }
        fn finish_book(_: &Option<Box<str>>, carry_out: &mut OwnedReduced) {
            carry_out.abstained = true;
        }
        fn fold_book(reduced: &[OwnedReduced], _: &()) -> String {
            reduced
                .iter()
                .map(|r| format!("<{}|{}|{}>", r.content, r.resolutions, r.abstained))
                .collect()
        }
        fn replace_book_in_corpus_stats(
            stats: &mut Vec<(String, String)>,
            slug: &str,
            _: Option<&String>,
            new: Option<&String>,
        ) -> Vec<String> {
            stats.retain(|(s, _)| s != slug);
            if let Some(new) = new {
                stats.push((slug.to_string(), new.clone()));
            }
            vec![slug.to_string()]
        }
        fn judge(_: &(), _: &String, _: &Vec<(String, String)>) {}
    }

    /// Drive one book of the owner-routed substrate.
    fn drive_owned(cache: &mut SubstrateCache<Owned>, slug: &str, chapters: &[(&str, &str)]) {
        let stamped: Vec<(&str, ObservationInputStamp)> = chapters
            .iter()
            .map(|(token, content)| {
                (
                    *token,
                    ObservationInputStamp {
                        schema_stamp: Owned::SCHEMA_STAMP,
                        chapter_hash: content
                            .bytes()
                            .fold(1u128, |h, b| h.wrapping_mul(31).wrapping_add(u128::from(b))),
                        extractor_fp: 0,
                        reference: ReferenceStamp::NotDeclared,
                    },
                )
            })
            .collect();
        let texts: Vec<Vec<String>> = chapters
            .iter()
            .map(|(_, c)| vec![(*c).to_string()])
            .collect();
        cache.update_book(slug, &stamped, &(), |i| {
            Owned::map_chapter(
                &ChapterView::target(chapters[i].0, &texts[i]),
                &(),
                &(),
            )
        });
    }

    fn assert_owned_equals_cold(
        resident: &SubstrateCache<Owned>,
        slug: &str,
        chapters: &[(&str, &str)],
    ) {
        let mut cold: SubstrateCache<Owned> = SubstrateCache::new();
        drive_owned(&mut cold, slug, chapters);
        assert_eq!(
            resident.book_contribution(slug),
            cold.book_contribution(slug),
            "resident replay differs from a cold whole-book build"
        );
    }

    /// The replay window must start at the chapter that OWNS a carried item, not
    /// at the earliest changed chapter: the owner's reduced result is rebuilt from
    /// nothing, so the resolution has to fold into it again. Starting later either
    /// drops the resolution or folds it into a cached result that already holds it.
    #[test]
    fn the_replay_window_starts_at_the_owner_of_a_carried_item() {
        let mut cache: SubstrateCache<Owned> = SubstrateCache::new();
        let cold = [("1", "a!"), ("2", "+b"), ("3", "c")];
        drive_owned(&mut cache, "GEN", &cold);

        // Edit chapter 2 — the chapter that RESOLVES chapter 1's item.
        cache.reset_probes();
        let edited = [("1", "a!"), ("2", "+bb"), ("3", "c")];
        drive_owned(&mut cache, "GEN", &edited);
        assert_eq!(cache.mapped, 1, "only the edited chapter is mapped");
        assert!(
            cache.reduced >= 2,
            "the replay reaches back to the owning chapter, so it reduces at least two"
        );
        assert_owned_equals_cold(&cache, "GEN", &edited);
    }

    /// The owner can sit behind chapters that neither buffer nor resolve
    /// anything — the all-empty-chapter case Entry 19's spacing fix pinned. The
    /// window still reaches back to the owner.
    #[test]
    fn the_owner_is_found_across_intervening_chapters() {
        let mut cache: SubstrateCache<Owned> = SubstrateCache::new();
        let cold = [("1", "a!"), ("2", ""), ("3", ""), ("4", "+d"), ("5", "e")];
        drive_owned(&mut cache, "GEN", &cold);

        cache.reset_probes();
        let edited = [("1", "a!"), ("2", ""), ("3", ""), ("4", "+dd"), ("5", "e")];
        drive_owned(&mut cache, "GEN", &edited);
        assert_eq!(cache.mapped, 1);
        assert_owned_equals_cold(&cache, "GEN", &edited);
    }

    /// A dangling item at the book edge is resolved by `finish_book` into its
    /// owner. A replay that converges early must not do that twice, and a replay
    /// that reaches the end must do it exactly once.
    #[test]
    fn the_book_edge_resolution_happens_exactly_once() {
        let mut cache: SubstrateCache<Owned> = SubstrateCache::new();
        let cold = [("1", "a"), ("2", "b"), ("3", "c!")];
        drive_owned(&mut cache, "GEN", &cold);
        assert_owned_equals_cold(&cache, "GEN", &cold);

        // Edit the first chapter: the replay converges immediately (nothing is
        // carried), so the cached book-edge resolution must stay put.
        cache.reset_probes();
        let edited = [("1", "aa"), ("2", "b"), ("3", "c!")];
        drive_owned(&mut cache, "GEN", &edited);
        assert_eq!(cache.reduced, 1, "nothing is carried, so it converges at once");
        assert_owned_equals_cold(&cache, "GEN", &edited);

        // Edit the dangling chapter itself: the replay reaches the book edge.
        cache.reset_probes();
        let edited2 = [("1", "aa"), ("2", "b"), ("3", "cc!")];
        drive_owned(&mut cache, "GEN", &edited2);
        assert_owned_equals_cold(&cache, "GEN", &edited2);
    }

    /// STRUCTURAL insertion between an owner and its resolver — the case the rest
    /// of this module's owner coverage does not reach. Every other owner test
    /// mutates a FIXED chapter set, and the insertion/removal tests use a
    /// substrate with no owner routing at all; so the interaction between "a
    /// carried item's owner is found by TOKEN, not by position" and "positions
    /// shift under a structural edit" was untested.
    ///
    /// Chapter `1` buffers an item it owns; `2` resolves it. Inserting a
    /// pass-through chapter between them moves the resolver from position 1 to
    /// position 2 while the owner keeps its token — so a driver that remembered
    /// the owner by index instead of by token would fold the resolution into the
    /// wrong chapter, and one that skipped re-reducing the owner would lose it.
    #[test]
    fn a_chapter_inserted_between_an_owner_and_its_resolver_replays_correctly() {
        let mut cache: SubstrateCache<Owned> = SubstrateCache::new();
        let seed = [("1", "a!"), ("2", "+b"), ("3", "c")];
        drive_owned(&mut cache, "GEN", &seed);
        assert_owned_equals_cold(&cache, "GEN", &seed);

        // Insert a chapter that neither buffers nor resolves. The carried item
        // now crosses it, and `2` — the owner's resolver — is one slot later.
        let inserted = [("1", "a!"), ("1b", "x"), ("2", "+b"), ("3", "c")];
        cache.reset_probes();
        drive_owned(&mut cache, "GEN", &inserted);
        assert_eq!(
            cache.mapped, 1,
            "only the inserted chapter is new text; the rest are reused by token"
        );
        assert_owned_equals_cold(&cache, "GEN", &inserted);

        // Insert a chapter that RESOLVES, ahead of the original resolver: the
        // owner's item is now consumed earlier, and the later `+b` has nothing to
        // resolve — a different book contribution, which cold agrees with.
        let stealing = [("1", "a!"), ("1b", "+x"), ("2", "+b"), ("3", "c")];
        drive_owned(&mut cache, "GEN", &stealing);
        assert_owned_equals_cold(&cache, "GEN", &stealing);

        // Insert a chapter that BUFFERS between owner and resolver: the first
        // item now dangles to the book edge while the new chapter owns the one
        // that `2` resolves.
        let shadowing = [("1", "a!"), ("1b", "y!"), ("2", "+b"), ("3", "c")];
        drive_owned(&mut cache, "GEN", &shadowing);
        assert_owned_equals_cold(&cache, "GEN", &shadowing);

        // And removing the interposed chapter again restores the original book.
        drive_owned(&mut cache, "GEN", &seed);
        assert_owned_equals_cold(&cache, "GEN", &seed);
    }

    /// The owner-routed property test: randomized edits over a book whose
    /// chapters buffer, resolve, pass through, or do neither.
    #[test]
    fn owner_routed_resident_equals_cold_under_randomized_edits() {
        let mut cache: SubstrateCache<Owned> = SubstrateCache::new();
        let tokens = ["1", "2", "3", "4", "5", "6"];
        let shapes = ["a!", "+b", "", "c", "+d!", "e!"];
        let mut contents: Vec<String> = shapes.iter().map(|s| (*s).to_string()).collect();
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        let next = |rng: &mut u64| {
            *rng ^= *rng << 13;
            *rng ^= *rng >> 7;
            *rng ^= *rng << 17;
            *rng
        };
        {
            let seed: Vec<(&str, &str)> = tokens
                .iter()
                .enumerate()
                .map(|(i, t)| (*t, contents[i].as_str()))
                .collect();
            drive_owned(&mut cache, "GEN", &seed);
        }
        for step in 0..150 {
            let which = (next(&mut rng) % tokens.len() as u64) as usize;
            let shape = (next(&mut rng) % 5) as usize;
            contents[which] = match shape {
                0 => format!("{step}!"),
                1 => format!("+{step}"),
                2 => String::new(),
                3 => format!("+{step}!"),
                _ => format!("{step}"),
            };
            let chapters: Vec<(&str, &str)> = tokens
                .iter()
                .enumerate()
                .map(|(i, t)| (*t, contents[i].as_str()))
                .collect();
            let before = cache.mapped;
            drive_owned(&mut cache, "GEN", &chapters);
            assert!(
                cache.mapped - before <= 1,
                "step {step}: one edited chapter must map at most one chapter"
            );
            assert_owned_equals_cold(&cache, "GEN", &chapters);
        }
    }
}
