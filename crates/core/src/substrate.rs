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
//!   sees one chapter and the extractor config — never a neighbour's state and
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
//! Phase C **step 1** lands this contract and the spacing substrate that
//! implements it as behaviour-neutral scaffolding: the machinery and its
//! byte-identity against the shipped rule are proven by unit tests, but the
//! transition still drives the old spacing path. Step 2 wires this module into
//! [`crate::transition`] and deletes that path; this blanket allow, which keeps
//! the not-yet-driven surface from tripping dead-code lints, is removed then.
#![allow(dead_code)]

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
}

impl SubstrateId {
    /// Every substrate id, declaration order — the exhaustive iteration source
    /// the registry-completeness tests walk.
    pub(crate) const ALL: &'static [SubstrateId] = &[SubstrateId::Spacing];
}

/// A verse-slice view of one chapter, handed to
/// [`map_chapter`](ObservationSubstrate::map_chapter). It is the whole map
/// input: a chapter's text, addressed chapter-locally. It carries no book
/// position, no neighbour, and no config — mapping cannot depend on any of them.
pub(crate) struct ChapterView<'a> {
    pub(crate) slug: &'a str,
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
/// A reduction is valid iff its observation is valid AND it was produced from
/// the same entering boundary state; the leaving state is retained so an ordered
/// replay can compare it against the next chapter's entering state (the Phase D
/// convergence test — Phase C re-reduces the whole owning book, so it compares
/// nothing, but the stamp is carried from the start so the shape does not move).
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
    fn fold_book(reduced: &[Self::ReducedChapter]) -> Self::BookContribution;

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
}

/// One book's resident substrate state: its ordered chapters and its folded
/// contribution to the corpus aggregate.
struct SubstrateBook<S: ObservationSubstrate> {
    chapters: Vec<SubstrateChapter<S>>,
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
    }

    #[cfg(test)]
    pub(crate) fn book_contribution(&self, slug: &str) -> Option<&S::BookContribution> {
        self.books.get(slug).map(|b| &b.contribution)
    }

    /// Bring one book up to date from its ordered chapters (plan §5.4, Phase C
    /// schedule). Each chapter is mapped iff its observation input stamp changed
    /// (a knob change leaves every stamp valid → zero maps); the whole book is
    /// then re-reduced left-to-right whenever any chapter changed, or reused
    /// wholesale when none did. Returns the stats-delta keys the aggregate
    /// change produced. The predecessor state never re-walks unchanged chapter
    /// text — reduction consumes cached observations only.
    ///
    /// `chapters` is the book's ordered `(opaque token, ObservationInputStamp,
    /// map)` triples; `map` is called only for a chapter whose stamp changed.
    pub(crate) fn update_book(
        &mut self,
        slug: &str,
        chapters: &[(Box<str>, ObservationInputStamp)],
        mut map: impl FnMut(usize) -> S::ChapterObservation,
    ) -> Vec<S::Key> {
        let existing = self.books.get(slug);
        // Decide, per chapter, whether the cached observation is reusable. A
        // chapter is reusable iff a cached chapter at the same position carries
        // the same opaque token and an equal input stamp.
        let mut observations: Vec<(ObservationInputStamp, S::ChapterObservation)> =
            Vec::with_capacity(chapters.len());
        let mut any_changed = existing.is_none_or(|b| b.chapters.len() != chapters.len());
        for (i, (token, stamp)) in chapters.iter().enumerate() {
            let reuse = existing.and_then(|b| b.chapters.get(i)).filter(|c| {
                *c.token == **token && c.input_stamp == *stamp
            });
            match reuse {
                Some(c) => observations.push((*stamp, c.observation.clone())),
                None => {
                    any_changed = true;
                    #[cfg(any(test, feature = "test-probes"))]
                    {
                        self.mapped += 1;
                    }
                    observations.push((*stamp, map(i)));
                }
            }
        }

        if !any_changed {
            // Whole book unchanged: reuse its contribution and reduced results
            // as-is, contributing zero aggregate delta.
            return Vec::new();
        }

        // Whole-book left-to-right carry reduce over the (cached or fresh)
        // observations (Phase C conservative schedule; the §5.4 replay-to-
        // convergence driver is Phase D). Text is never re-walked: reduction
        // consumes observations only.
        // Route a carried cross-seam contribution to its OWNING chapter by
        // opaque token — an owner can sit behind an all-empty chapter, so it is
        // not always the immediate predecessor. Tokens are unique within a book
        // (no reopened chapter — Phase A invariant), so token → position is a
        // function.
        let token_pos: FxHashMap<&str, usize> = chapters
            .iter()
            .enumerate()
            .map(|(i, (t, _))| (&**t, i))
            .collect();
        let mut reduced: Vec<S::ReducedChapter> = Vec::with_capacity(observations.len());
        let mut stamps: Vec<ReducedChapterStamp<S::BoundaryState>> =
            Vec::with_capacity(observations.len());
        let mut carry = S::BoundaryState::default();
        for (input_stamp, obs) in &observations {
            let entering = carry.clone();
            let owner = S::pending_owner(&entering).map(|tok| token_pos[tok]);
            // `carry_out` is the owner chapter's reduced result (already pushed,
            // so its index is < the current one); at book start there is no
            // carry, so the throwaway sink is never written.
            let (this, leaving) = match owner {
                Some(k) => S::reduce_chapter(obs, &entering, &mut reduced[k]),
                None => {
                    let mut sink = S::ReducedChapter::default();
                    S::reduce_chapter(obs, &entering, &mut sink)
                }
            };
            stamps.push(ReducedChapterStamp {
                observation: *input_stamp,
                entering,
                leaving: leaving.clone(),
            });
            reduced.push(this);
            carry = leaving;
            #[cfg(any(test, feature = "test-probes"))]
            {
                self.reduced += 1;
            }
        }
        // Book edge: no neighbour across the final seam — resolve the dangling
        // boundary state into its OWNING chapter's reduced result.
        if let Some(owner) = S::pending_owner(&carry).map(|tok| token_pos[tok]) {
            S::finish_book(&carry, &mut reduced[owner]);
        }

        let new_contribution = S::fold_book(&reduced);
        let old_contribution = self.books.get(slug).map(|b| b.contribution.clone());
        let delta = S::replace_book_in_corpus_stats(
            &mut self.corpus_stats,
            slug,
            old_contribution.as_ref(),
            Some(&new_contribution),
        );

        let new_chapters: Vec<SubstrateChapter<S>> = chapters
            .iter()
            .zip(observations)
            .zip(reduced)
            .zip(stamps)
            .map(|((((token, _stamp), (input_stamp, observation)), reduced), reduced_stamp)| {
                SubstrateChapter {
                    token: token.clone(),
                    input_stamp,
                    observation,
                    reduced_stamp,
                    reduced,
                }
            })
            .collect();
        self.books.insert(
            Box::from(slug),
            SubstrateBook {
                chapters: new_chapters,
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
}

impl ActiveSubstrates {
    /// Derive the active set from the final coalesced config: a substrate is
    /// active iff any of its consumers is enabled (the closed registry below).
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            spacing: spacing_consumers()
                .iter()
                .any(|&r| config.is_enabled(r)),
        }
    }

    pub(crate) fn is_active(&self, id: SubstrateId) -> bool {
        match id {
            SubstrateId::Spacing => self.spacing,
        }
    }
}

/// The closed registry: which rules consume the spacing substrate. `spacing`'s
/// sole consumer today is `punct.spacing-anomaly`; a future second consumer is
/// added here and the completeness tests keep the registry honest.
pub(crate) fn spacing_consumers() -> &'static [RuleId] {
    &[RuleId::PunctuationSpacingAnomaly]
}

/// The consumers of a substrate by id — the exhaustive closed match the
/// completeness tests walk.
pub(crate) fn consumers_of(id: SubstrateId) -> &'static [RuleId] {
    match id {
        SubstrateId::Spacing => spacing_consumers(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            // id is handled; assert it reads the right field for spacing.
            let active = ActiveSubstrates { spacing: true };
            let _ = active.is_active(id);
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
