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
        SubstrateId::DuplicateWord,
        SubstrateId::Casing,
        SubstrateId::MixedCase,
    ];
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
    pub(crate) fn update_book(
        &mut self,
        slug: &str,
        chapters: &[(Box<str>, ObservationInputStamp)],
        symbols: &S::Symbols,
        mut map: impl FnMut(usize) -> S::ChapterObservation,
    ) -> Vec<S::Key> {
        let n = chapters.len();
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
                .get(&**token)
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
            .map(|(i, (t, _))| (&**t, i))
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
                    token: token.clone(),
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
                duplicate_word: true,
                casing: true,
                mixed_case: true,
            };
            assert!(all_on.is_active(id), "{id:?} has no active-set field");
            assert!(!ActiveSubstrates::default().is_active(id));
        }
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
        let stamped: Vec<(Box<str>, ObservationInputStamp)> = chapters
            .iter()
            .map(|(token, content)| {
                (
                    Box::from(*token),
                    ObservationInputStamp {
                        schema_stamp: S::SCHEMA_STAMP,
                        chapter_hash: content
                            .bytes()
                            .fold(1u128, |h, b| h.wrapping_mul(31).wrapping_add(u128::from(b))),
                        extractor_fp: S::extractor_fp(&()),
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
                &ChapterView {
                    chapter: chapters[i].0,
                    texts: &texts[i],
                },
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
    /// whole-book-unchanged path must put the book back byte-for-byte.
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
        let stamped: Vec<(Box<str>, ObservationInputStamp)> = chapters
            .iter()
            .map(|(token, content)| {
                (
                    Box::from(*token),
                    ObservationInputStamp {
                        schema_stamp: Owned::SCHEMA_STAMP,
                        chapter_hash: content
                            .bytes()
                            .fold(1u128, |h, b| h.wrapping_mul(31).wrapping_add(u128::from(b))),
                        extractor_fp: 0,
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
                &ChapterView {
                    chapter: chapters[i].0,
                    texts: &texts[i],
                },
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
