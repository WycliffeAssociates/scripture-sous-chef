//! The chapter-outer map scheduler (epic plan §6).
//!
//! ## What moved, and what deliberately did not
//!
//! ADR 0067 gave every corpus-relative rule an independent typed observation
//! substrate, and each substrate's `drive_*` planned, mapped, reduced, judged and
//! materialized as one unit. That made the resident warm edit path narrow, at the
//! cost of a cold seed that walks the corpus once *per enabled substrate* (ADR
//! 0068: +16–35% serial cold). The repeated work is not the observation — each
//! substrate's evidence is genuinely its own — it is the **mechanical
//! preparation**: six token walks, six tape walks and three grapheme walks over
//! the same text.
//!
//! This module turns the map phase chapter-outer so that preparation is produced
//! once per chapter and consumed by every participant while it is alive:
//!
//! ```text
//! for each chapter in the union of participant-dirty chapters
//!     build the mechanical views its participants requested
//!     map only those participants, in fixed registry order
//!     drop the views before this worker takes another chapter
//! ```
//!
//! Everything else about ADR 0067 is untouched and must stay untouched. In
//! particular:
//!
//! - **No rule dependencies.** A participant declares which mechanical views its
//!   mapper reads ([`crate::prep::PrepNeeds`]); it never declares, reads, or is
//!   ordered against another participant. [`SubstrateMask`] is a scheduling fact,
//!   not an executable dependency graph.
//! - **Per-substrate validity is still the authority.** Participation is derived
//!   from each substrate's own [`ObservationInputStamp`] against its own cache,
//!   with the same predicate `update_book` reuses by. A judging-only config
//!   change still enrols nobody and therefore maps and reduces zero chapters.
//! - **Reduction, judgment and publication are unchanged.** After the ordered
//!   collection this module hands each substrate its own mapped observations and
//!   steps aside; reduction stays a per-book serial carry fold from that
//!   substrate's own cache, and finding partitions still commit at the single
//!   atomic boundary in [`crate::transition`].
//! - **No dynamic payloads.** [`MappedChapterBundle`] is one typed optional slot
//!   per closed participant. There is no `dyn Any`, no downcast, and no map keyed
//!   by an id.
//!
//! ## One Rayon grain, ordered slots
//!
//! Mapping has exactly one outer serial/book/chapter grain, chosen once from the
//! union work list by the existing [`crate::rule::map_route`] policy. A worker
//! maps its chapter's participants **serially**; nothing nests a second fan-out
//! inside the chapter task. Results are collected indexed, so the bundle at
//! position `k` is the work item at position `k` regardless of completion order,
//! and every scatter writes into the layout position the work item came from.
//! Serial and parallel builds, at any thread count, therefore produce identical
//! observations — the route is a wall-clock decision only.
//!
//! Parallel closures read the corpus and write nothing: they never touch a
//! resident cache. Observations are committed serially afterwards, by each
//! substrate's own `finish_*`.

use crate::corpus::{BookLayout, ChapterLayout, Corpus};
use crate::prep::{ChapterPrep, PrepNeeds};
use crate::substrate::{
    ChapterView, ObservationInputStamp, ObservationSubstrate, PairedView, SubstrateCache,
    SubstrateId,
};

/// The closed participation set for one chapter — one bit per [`SubstrateId`].
///
/// A scheduling fact. It says which substrates' observations this chapter owes,
/// never that one substrate needs another. It is deliberately not an "executable
/// union of rules": a substrate's bit is set from its own stamp comparison
/// against its own cache, in isolation.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct SubstrateMask(u16);

impl SubstrateMask {
    pub(crate) const EMPTY: Self = SubstrateMask(0);

    /// This id's bit. `SubstrateId` is a closed enum whose discriminants are its
    /// `ALL` positions (pinned by `substrate_names_cover_every_id`), so the shift
    /// is total and the width assertion below cannot silently overflow.
    const fn bit(id: SubstrateId) -> u16 {
        const _: () = assert!(
            SubstrateId::ALL.len() <= u16::BITS as usize,
            "SubstrateMask is a u16; a 17th substrate needs a wider mask"
        );
        1u16 << (id as u16)
    }

    pub(crate) fn insert(&mut self, id: SubstrateId) {
        self.0 |= Self::bit(id);
    }

    pub(crate) fn contains(self, id: SubstrateId) -> bool {
        self.0 & Self::bit(id) != 0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One reference corpus, resolved once per analyze into the pairing every
/// reference-declaring participant reads.
///
/// The pairing is by `(slug, chapter token)`, and that is sound because a key's
/// chapter token is *parsed from the key* and a chapter run may not reopen: every
/// occurrence of a key string therefore lies inside one chapter run, on both
/// sides. So a target chapter's reference evidence is exactly the reference
/// chapter carrying the same token in the same book — never a wider scope, and
/// never a cross-slug read.
///
/// Resolved by the scheduler rather than per substrate because the chapter task
/// hands one pairing to every participant. Before the chapter-outer schedule each
/// reference-declaring drive built this index itself, which meant two identical
/// whole-reference walks whenever both were active.
pub(crate) struct ReferencePairingIndex<'a> {
    chapters: rustc_hash::FxHashMap<(&'a str, &'a str), &'a ChapterLayout>,
    keys: &'a [String],
    texts: &'a [String],
}

impl<'a> ReferencePairingIndex<'a> {
    pub(crate) fn new(source: Option<&'a Corpus>) -> Option<Self> {
        let source = source?;
        let mut chapters: rustc_hash::FxHashMap<(&'a str, &'a str), &'a ChapterLayout> =
            rustc_hash::FxHashMap::default();
        for book in source.book_layout() {
            for c in &book.chapters {
                // A chapter run may not reopen, so this is a function, not a
                // multimap — the same invariant that makes the per-key ordinal
                // chapter-local.
                chapters.insert((&book.slug, &c.chapter), c);
            }
        }
        Some(ReferencePairingIndex {
            chapters,
            keys: source.keys(),
            texts: source.texts(),
        })
    }

    /// The paired reference chapter's content hash, for the declared half of a
    /// reference-declaring substrate's [`ObservationInputStamp`]. `None` means
    /// nothing pairs with this chapter, which stamps as an explicit absent tag —
    /// distinct from "not declared", so a reference appearing or disappearing
    /// invalidates exactly the observations whose evidence moved.
    pub(crate) fn hash_of(&self, slug: &str, token: &str) -> Option<u128> {
        self.chapters.get(&(slug, token)).map(|rc| rc.hash)
    }

    /// The paired view for one target chapter: its own keys plus the reference
    /// chapter's keys and texts.
    pub(crate) fn view_of(
        &self,
        target_keys: &'a [String],
        slug: &str,
        token: &str,
    ) -> Option<PairedView<'a>> {
        let rc = self.chapters.get(&(slug, token))?;
        Some(PairedView {
            keys: target_keys,
            reference_keys: &self.keys[rc.range.clone()],
            reference_texts: &self.texts[rc.range.clone()],
        })
    }
}

/// One chapter's accumulated scheduling facts, before the work list is built.
#[derive(Clone, Copy, Default)]
struct Cell {
    participants: SubstrateMask,
    needs: PrepNeeds,
    /// The direct per-verse lane participates in this chapter. It is not a
    /// substrate — it owns no observation substrate and no corpus aggregate — but
    /// it *is* a participant of the chapter task, with its own stamp-derived
    /// dirty set and its own declared prep needs (the masked tape). Sharing that
    /// tape with the five tape-reading substrates is the largest single item in
    /// ADR 0068's unrealized list.
    direct: bool,
}

/// One chapter of chapter-outer map work: its layout position and identity, the
/// text its mappers read, the paired reference view if the corpus has one, and
/// the closed participation/prep facts the planning pass derived.
///
/// It carries no book position beyond the layout index used to route results
/// back, no neighbour, and no judging knob — mapping cannot depend on any of
/// them.
pub(crate) struct ChapterMapWork<'a> {
    book: usize,
    chapter: usize,
    token: &'a str,
    texts: &'a [String],
    /// The reference chapter carrying the same `(slug, chapter token)`, if any.
    /// Handed only to a participant whose `Pairing` declares a reference; see
    /// [`ChapterView::scheduled`].
    paired: Option<PairedView<'a>>,
    participants: SubstrateMask,
    needs: PrepNeeds,
    direct: bool,
}

/// What one chapter task mapped: one typed optional slot per closed participant.
///
/// A struct of typed `Option`s rather than a payload map keyed by id — the
/// compiler, not a runtime lookup, pairs a participant with its observation
/// type, and a new substrate is a compile error until it has a slot here.
#[derive(Default)]
pub(crate) struct MappedChapterBundle {
    pub(crate) direct: Option<Vec<crate::cache::CachedPerVerseFinding>>,
    pub(crate) spacing: Option<crate::signals::punctuation::SpacingChapterObs>,
    pub(crate) adjacency: Option<crate::signals::punctuation::AdjacencyChapterObs>,
    pub(crate) normalization: Option<crate::signals::mixed_normalization::NormChapterObs>,
    pub(crate) punct_only: Option<crate::signals::lexical::PunctOnlyChapterObs>,
    pub(crate) bracket: Option<crate::signals::bracket_balance::BracketChapterObs>,
    pub(crate) repeated_run: Option<crate::signals::lexical::RepeatChapterObs>,
    pub(crate) mixed_script: Option<crate::signals::script_mixing::MixedScriptChapterObs>,
    pub(crate) glyph: Option<crate::signals::rare_glyph::GlyphChapterObs>,
    pub(crate) duplicate_word: Option<crate::signals::lexical::DuplicateChapterObs>,
    pub(crate) mixed_case: Option<crate::signals::mixed_case::MixedCaseChapterObs>,
    pub(crate) casing: Option<crate::signals::casing::CasingChapterObs>,
    pub(crate) proportionality: Option<crate::signals::proportionality::RatioChapterObs>,
    pub(crate) untranslated_words: Option<crate::signals::untranslated_words::WordChapterObs>,
}

impl MappedChapterBundle {
    /// Which substrate slots this bundle actually filled — the observable half of
    /// the closed-set guard `the_chapter_task_maps_every_substrate_it_is_given`
    /// needs. Exhaustive over [`SubstrateId`], so a new substrate is a compile
    /// error here too.
    #[cfg(test)]
    fn filled(&self) -> SubstrateMask {
        let mut m = SubstrateMask::EMPTY;
        for (id, present) in [
            (SubstrateId::Spacing, self.spacing.is_some()),
            (SubstrateId::Adjacency, self.adjacency.is_some()),
            (SubstrateId::RepeatedRun, self.repeated_run.is_some()),
            (SubstrateId::PunctOnly, self.punct_only.is_some()),
            (SubstrateId::MixedScript, self.mixed_script.is_some()),
            (SubstrateId::Glyph, self.glyph.is_some()),
            (SubstrateId::Proportionality, self.proportionality.is_some()),
            (SubstrateId::Normalization, self.normalization.is_some()),
            (SubstrateId::Bracket, self.bracket.is_some()),
            (SubstrateId::DuplicateWord, self.duplicate_word.is_some()),
            (SubstrateId::Casing, self.casing.is_some()),
            (SubstrateId::MixedCase, self.mixed_case.is_some()),
            (
                SubstrateId::UntranslatedWords,
                self.untranslated_words.is_some(),
            ),
        ] {
            if present {
                m.insert(id);
            }
        }
        m
    }
}

/// The inputs a chapter task needs beyond the corpus text: the shared
/// append-only word table its interning participants name observations through,
/// and the enabled per-verse rules the direct lane runs.
///
/// `Sync` by construction — every field is a shared reference to something the
/// map phase only reads.
pub(crate) struct MapContext<'a> {
    pub(crate) words: &'a crate::interner::WordInterner,
    pub(crate) per_verse: &'a [Box<dyn crate::rule::PerVerseRule>],
}

/// One substrate's planned map work: the ordered `(chapter token, stamp)` pairs
/// its `update_book` consumes per book, and the layout-shaped slots the scheduler
/// scatters its mapped observations into.
///
/// The tokens are **borrowed** from the corpus layout, which outlives the
/// analyze, so planning allocates no chapter names; ownership is taken only where
/// `update_book` rebuilds a persistent cache entry.
pub(crate) struct SubstratePlan<'a, S: ObservationSubstrate> {
    pub(crate) stamped: Vec<Vec<(&'a str, ObservationInputStamp)>>,
    pub(crate) slots: MappedSlots<S>,
}

/// One substrate's mapped observations, in layout-shaped slots.
///
/// A separate field from `stamped` so a `finish_*` can hand `update_book` the
/// borrowed stamp list and a mutating take-closure at the same time.
pub(crate) struct MappedSlots<S: ObservationSubstrate> {
    slots: Vec<Vec<Option<S::ChapterObservation>>>,
}

impl<S: ObservationSubstrate> MappedSlots<S> {
    /// Take the observation the scheduler mapped for layout position
    /// `(book, chapter)`.
    ///
    /// A missing slot means the planning pass and the scheduler disagreed about
    /// participation, which would silently produce an observation from different
    /// inputs than the stamp claims. That is a loud internal invariant failure,
    /// **not** an implicit recomputation path: the pre-scheduler drives each
    /// carried an `unwrap_or_else(|| map_chapter(..))` fallback that could never
    /// fire, and re-mapping here would hide a scheduler defect behind a
    /// slowdown.
    pub(crate) fn take(&mut self, book: usize, chapter: usize) -> S::ChapterObservation {
        self.slots[book][chapter].take().unwrap_or_else(|| {
            panic!(
                "{:?} was asked for chapter ({book},{chapter})'s observation but the scheduler \
                 never mapped it — stop and report: the planning pass and the chapter-outer \
                 map disagree about participation",
                S::ID
            )
        })
    }
}

/// The chapter-outer plan under construction: the layout it is shaped to, and
/// each chapter's accumulated participation and prep needs.
pub(crate) struct Schedule<'a> {
    layout: &'a [BookLayout],
    texts: &'a [String],
    keys: &'a [String],
    grid: Vec<Vec<Cell>>,
}

impl<'a> Schedule<'a> {
    pub(crate) fn new(corpus: &'a Corpus) -> Self {
        let layout = corpus.book_layout();
        Schedule {
            layout,
            texts: corpus.texts(),
            keys: corpus.keys(),
            grid: layout
                .iter()
                .map(|b| vec![Cell::default(); b.chapters.len()])
                .collect(),
        }
    }

    pub(crate) fn layout(&self) -> &'a [BookLayout] {
        self.layout
    }

    pub(crate) fn texts(&self) -> &'a [String] {
        self.texts
    }

    pub(crate) fn keys(&self) -> &'a [String] {
        self.keys
    }

    /// Enrol one substrate: stamp every chapter of every book, ask that
    /// substrate's own cache which observations are stale, and record its
    /// participation and declared prep needs on exactly those chapters.
    ///
    /// The stamp comparison is the same `observation_is_current` predicate
    /// `update_book` reuses by, so a plan and the driver can never disagree about
    /// which chapters are dirty. `stamp_of` is the substrate's own stamp
    /// construction — target-only or reference-declaring, gated by its `Pairing`
    /// type — so this generic pass cannot quietly stamp a substrate the wrong
    /// way.
    pub(crate) fn enrol<S: ObservationSubstrate>(
        &mut self,
        cache: &SubstrateCache<S>,
        stamp_of: impl Fn(&'a str, &'a ChapterLayout) -> ObservationInputStamp,
    ) -> SubstratePlan<'a, S> {
        // Bound the layout out of `self` first: it is a shared reference the
        // schedule merely holds, so iterating it must not borrow `self` while the
        // grid is written.
        let layout = self.layout;
        let mut stamped = Vec::with_capacity(layout.len());
        let mut slots = Vec::with_capacity(layout.len());
        for (bi, book) in layout.iter().enumerate() {
            let mut chapters = Vec::with_capacity(book.chapters.len());
            for (ci, c) in book.chapters.iter().enumerate() {
                let stamp = stamp_of(&book.slug, c);
                if !cache.observation_is_current(&book.slug, &c.chapter, &stamp) {
                    let cell = &mut self.grid[bi][ci];
                    cell.participants.insert(S::ID);
                    cell.needs = cell.needs.union(S::NEEDS);
                }
                chapters.push((&*c.chapter, stamp));
            }
            slots.push((0..book.chapters.len()).map(|_| None).collect());
            stamped.push(chapters);
        }
        SubstratePlan {
            stamped,
            slots: MappedSlots { slots },
        }
    }

    /// Enrol the direct per-verse lane for one chapter position.
    pub(crate) fn enrol_direct(&mut self, book: usize, chapter: usize) {
        let cell = &mut self.grid[book][chapter];
        cell.direct = true;
        cell.needs = cell.needs.union(crate::signals::DIRECT_LANE_NEEDS);
    }

    /// Build the corpus-order work list from the accumulated grid, then map every
    /// work item through the single outer Rayon grain.
    ///
    /// `paired_of` supplies the reference chapter pairing when the corpus has a
    /// reference at all; it is consulted once per work item during planning, not
    /// inside the map, so the fan-out closure borrows only finished data.
    pub(crate) fn run(
        &self,
        ctx: &MapContext<'_>,
        paired_of: impl Fn(&'a str, &'a ChapterLayout) -> Option<PairedView<'a>>,
    ) -> (Vec<ChapterMapWork<'a>>, Vec<MappedChapterBundle>) {
        let mut work: Vec<ChapterMapWork<'a>> = Vec::new();
        let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
        // The work's size, for the route decision only: summing already-known
        // string lengths, so it costs one integer add per dirty verse and reads
        // no text.
        let mut work_bytes = 0usize;
        for (bi, book) in self.layout.iter().enumerate() {
            let run_start = work.len();
            for (ci, c) in book.chapters.iter().enumerate() {
                let cell = self.grid[bi][ci];
                if cell.participants.is_empty() && !cell.direct {
                    continue;
                }
                let texts = &self.texts[c.range.clone()];
                work_bytes += texts.iter().map(String::len).sum::<usize>();
                work.push(ChapterMapWork {
                    book: bi,
                    chapter: ci,
                    token: &c.chapter,
                    texts,
                    paired: paired_of(&book.slug, c),
                    participants: cell.participants,
                    needs: cell.needs,
                    direct: cell.direct,
                });
            }
            if work.len() > run_start {
                book_runs.push(run_start..work.len());
            }
        }
        let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
        let mapped =
            crate::rule::map_chapter_work(&work, &book_runs, route, |w| map_one_chapter(w, ctx));
        (work, mapped)
    }

    /// The single-participant path: plan, map and scatter one substrate on its
    /// own. Used by the per-substrate convenience entry points (`*_findings`) and
    /// their tests, which hold one substrate's cache rather than the whole
    /// section.
    ///
    /// It is the same chapter task the production scheduler runs — same
    /// [`ChapterPrep`] construction from the same declared [`PrepNeeds`], same
    /// [`map_participant`] call — with a one-substrate mask. There is no second
    /// mapper implementation and no behaviourally different cold analyzer.
    pub(crate) fn run_solo<S: ObservationSubstrate>(
        &self,
        plan: &mut SubstratePlan<'a, S>,
        extractor: &S::ExtractorConfig,
        symbols: &S::Symbols,
        paired_of: impl Fn(&'a str, &'a ChapterLayout) -> Option<PairedView<'a>> + Sync,
    ) where
        S::ChapterObservation: Send,
        S::ExtractorConfig: Sync,
    {
        let mut work: Vec<ChapterMapWork<'a>> = Vec::new();
        let mut book_runs: Vec<std::ops::Range<usize>> = Vec::new();
        let mut work_bytes = 0usize;
        for (bi, book) in self.layout.iter().enumerate() {
            let run_start = work.len();
            for (ci, c) in book.chapters.iter().enumerate() {
                if !self.grid[bi][ci].participants.contains(S::ID) {
                    continue;
                }
                let texts = &self.texts[c.range.clone()];
                work_bytes += texts.iter().map(String::len).sum::<usize>();
                work.push(ChapterMapWork {
                    book: bi,
                    chapter: ci,
                    token: &c.chapter,
                    texts,
                    paired: paired_of(&book.slug, c),
                    participants: {
                        let mut m = SubstrateMask::EMPTY;
                        m.insert(S::ID);
                        m
                    },
                    needs: S::NEEDS,
                    direct: false,
                });
            }
            if work.len() > run_start {
                book_runs.push(run_start..work.len());
            }
        }
        let route = crate::rule::map_route(&book_runs, work.len(), work_bytes);
        let mapped = crate::rule::map_chapter_work(&work, &book_runs, route, |w| {
            let prep = ChapterPrep::build(w.texts, w.needs);
            map_participant::<S>(w.token, w.texts, &prep, w.paired, extractor, symbols)
        });
        for (w, obs) in work.iter().zip(mapped) {
            plan.slots.slots[w.book][w.chapter] = Some(obs);
        }
    }
}

/// Route one substrate's mapped observations from the ordered bundles into its
/// plan's layout-shaped slots. `pick` is the bundle's typed slot for that
/// substrate — `|b| b.adjacency.take()` — so the pairing is a compile-time field
/// access, never a lookup.
pub(crate) fn scatter<S: ObservationSubstrate>(
    work: &[ChapterMapWork<'_>],
    mapped: &mut [MappedChapterBundle],
    plan: &mut SubstratePlan<'_, S>,
    pick: impl Fn(&mut MappedChapterBundle) -> Option<S::ChapterObservation>,
) {
    for (w, bundle) in work.iter().zip(mapped.iter_mut()) {
        if !w.participants.contains(S::ID) {
            continue;
        }
        let obs = pick(bundle).unwrap_or_else(|| {
            panic!(
                "{:?} participated in chapter ({},{}) but its bundle slot is empty — stop and \
                 report: the chapter task's participant block and the participation mask \
                 disagree",
                S::ID,
                w.book,
                w.chapter
            )
        });
        plan.slots.slots[w.book][w.chapter] = Some(obs);
    }
}

/// The direct lane's mapped records, in work order, with the layout position each
/// came from.
pub(crate) fn scatter_direct<'w>(
    work: &'w [ChapterMapWork<'_>],
    mapped: &mut [MappedChapterBundle],
) -> Vec<(usize, usize, Vec<crate::cache::CachedPerVerseFinding>)> {
    work.iter()
        .zip(mapped.iter_mut())
        .filter(|(w, _)| w.direct)
        .map(|(w, bundle)| {
            let records = bundle.direct.take().unwrap_or_else(|| {
                panic!(
                    "the direct lane participated in chapter ({},{}) but its bundle slot is \
                     empty — stop and report: the chapter task and the participation mask \
                     disagree",
                    w.book, w.chapter
                )
            });
            (w.book, w.chapter, records)
        })
        .collect()
}

/// One chapter task: build the mechanical views its participants requested, map
/// each participant over them in fixed registry order, and return the typed
/// bundle. `prep` is dropped on return, before this worker takes another chapter.
///
/// Mapper order is fixed for determinism and auditability only — the mappers are
/// independent, so the order has no semantic effect. Each mapper walks the
/// prepared views separately; nothing fuses two participants' collectors into one
/// loop, which would couple their ownership for a win that has not been measured.
fn map_one_chapter(w: &ChapterMapWork<'_>, ctx: &MapContext<'_>) -> MappedChapterBundle {
    let prep = ChapterPrep::build(w.texts, w.needs);
    let mut bundle = MappedChapterBundle::default();
    if w.participants.contains(SubstrateId::Spacing) {
        bundle.spacing = Some(map_participant::<
            crate::signals::punctuation::SpacingSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::Adjacency) {
        bundle.adjacency = Some(map_participant::<
            crate::signals::punctuation::AdjacencySubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::PunctOnly) {
        bundle.punct_only = Some(
            map_participant::<crate::signals::lexical::PunctOnlySubstrate>(
                w.token,
                w.texts,
                &prep,
                w.paired,
                &(),
                &(),
            ),
        );
    }
    if w.participants.contains(SubstrateId::Bracket) {
        bundle.bracket = Some(map_participant::<
            crate::signals::bracket_balance::BracketSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::Normalization) {
        bundle.normalization = Some(map_participant::<
            crate::signals::mixed_normalization::NormalizationSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::RepeatedRun) {
        bundle.repeated_run = Some(map_participant::<
            crate::signals::lexical::RepeatedRunSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::MixedScript) {
        bundle.mixed_script = Some(map_participant::<
            crate::signals::script_mixing::MixedScriptSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::Glyph) {
        bundle.glyph = Some(
            map_participant::<crate::signals::rare_glyph::GlyphSubstrate>(
                w.token,
                w.texts,
                &prep,
                w.paired,
                &(),
                &(),
            ),
        );
    }
    if w.participants.contains(SubstrateId::DuplicateWord) {
        bundle.duplicate_word = Some(map_participant::<
            crate::signals::lexical::DuplicateWordSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    // The two interning participants name their observations through the shared
    // append-only word table. It is NAMING, never evidence: appending to it cannot
    // change what an observation says, only which integer stands for a word, which
    // is why it may be read from a parallel closure at all.
    if w.participants.contains(SubstrateId::MixedCase) {
        bundle.mixed_case = Some(map_participant::<
            crate::signals::mixed_case::MixedCaseSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), ctx.words));
    }
    if w.participants.contains(SubstrateId::Casing) {
        bundle.casing = Some(map_participant::<crate::signals::casing::CasingSubstrate>(
            w.token,
            w.texts,
            &prep,
            w.paired,
            &(),
            ctx.words,
        ));
    }
    // The two reference-declaring participants. `ChapterView::scheduled` is what
    // grants them the pairing and withholds it from everyone else, through their
    // `Pairing` type rather than through this block's ordering.
    if w.participants.contains(SubstrateId::Proportionality) {
        bundle.proportionality = Some(map_participant::<
            crate::signals::proportionality::ProportionalitySubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    if w.participants.contains(SubstrateId::UntranslatedWords) {
        bundle.untranslated_words = Some(map_participant::<
            crate::signals::untranslated_words::UntranslatedWordsSubstrate,
        >(w.token, w.texts, &prep, w.paired, &(), &()));
    }
    bundle
}

/// Map one participant over the chapter's prepared views.
///
/// The one place a `map_chapter` call is assembled, shared by the production
/// chapter task and [`Schedule::run_solo`] so the two cannot drift.
/// [`ChapterView::scheduled`] hands the mapper exactly the views its
/// [`ObservationSubstrate::NEEDS`] declared and exactly the reference access its
/// `Pairing` declared — nothing more, so a mapper cannot read an undeclared view
/// even by accident, and nothing less, so an undeclared read fails loudly.
#[inline]
fn map_participant<S: ObservationSubstrate>(
    token: &str,
    texts: &[String],
    prep: &ChapterPrep,
    paired: Option<PairedView<'_>>,
    extractor: &S::ExtractorConfig,
    symbols: &S::Symbols,
) -> S::ChapterObservation {
    let view = ChapterView::scheduled::<S>(token, texts, prep, paired);
    S::map_chapter(&view, extractor, symbols)
}

/// Map one chapter's observation for a single substrate outside any schedule,
/// building exactly that substrate's declared prep for the chapter and dropping
/// it again.
///
/// For measurement and test code that walks a corpus itself (the bracket
/// stack-depth fleet probe, the substrate-level unit batteries). It routes
/// through the same [`map_participant`] the production chapter task uses, so such
/// a caller cannot accidentally hand a mapper a different set of views than the
/// scheduler would.
pub(crate) fn map_chapter_standalone<S: ObservationSubstrate>(
    token: &str,
    texts: &[String],
    paired: Option<PairedView<'_>>,
    extractor: &S::ExtractorConfig,
    symbols: &S::Symbols,
) -> S::ChapterObservation {
    let prep = ChapterPrep::build(texts, S::NEEDS);
    map_participant::<S>(token, texts, &prep, paired, extractor, symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mask is a closed bitset over `SubstrateId`: every id has its own bit,
    /// no two ids share one, and membership is exact.
    #[test]
    fn every_substrate_has_its_own_mask_bit() {
        let mut all = SubstrateMask::EMPTY;
        assert!(all.is_empty());
        for &id in SubstrateId::ALL {
            let mut one = SubstrateMask::EMPTY;
            one.insert(id);
            assert!(one.contains(id), "{id:?} bit does not read back");
            for &other in SubstrateId::ALL {
                if other != id {
                    assert!(!one.contains(other), "{id:?}'s bit also reads as {other:?}");
                }
            }
            all.insert(id);
        }
        for &id in SubstrateId::ALL {
            assert!(all.contains(id));
        }
        assert_eq!(all.0.count_ones() as usize, SubstrateId::ALL.len());
    }

    /// THE CLOSED-SET GUARD. For every `SubstrateId`, a chapter task given exactly
    /// that participant must fill exactly that bundle slot.
    ///
    /// Both halves matter, and both were real defects while this module was being
    /// built. A missing arm in [`map_one_chapter`] left the bit set and the slot
    /// empty, which `scatter` can only discover as a runtime panic on the first
    /// corpus that reaches it; an arm keyed to the wrong id would fill a
    /// neighbour's slot and hand one substrate another's observation. Neither is
    /// something the type system catches, because every arm is well typed on its
    /// own — so it is pinned here instead.
    #[test]
    fn the_chapter_task_maps_every_substrate_it_is_given() {
        // Verse texts rich enough that no mapper's output depends on finding
        // something: `filled()` reports slot presence, not emptiness, so even a
        // mapper that observes nothing must still fill its slot.
        let texts: Vec<String> = [
            "In the beginning, God created;",
            "a  b) \"word\" ४५ e\u{0301}",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let words = crate::interner::WordInterner::default();
        let per_verse: Vec<Box<dyn crate::rule::PerVerseRule>> = Vec::new();
        let ctx = MapContext {
            words: &words,
            per_verse: &per_verse,
        };
        for &id in SubstrateId::ALL {
            let mut participants = SubstrateMask::EMPTY;
            participants.insert(id);
            // The union of one participant's needs is its own declaration; a
            // mapper reading a view it did not declare panics in the accessor,
            // which is the other half of the contract this exercises.
            let needs = needs_of(id);
            let work = ChapterMapWork {
                book: 0,
                chapter: 0,
                token: "1",
                texts: &texts,
                paired: None,
                participants,
                needs,
                direct: false,
            };
            let bundle = map_one_chapter(&work, &ctx);
            assert_eq!(
                bundle.filled(),
                participants,
                "the chapter task given only {id:?} filled a different slot set — its \
                 participant arm is missing or keyed to the wrong id"
            );
        }
    }

    /// The declared needs of one substrate by id — the exhaustive match that lets
    /// the guard above drive every participant without naming its type twice.
    #[cfg(test)]
    fn needs_of(id: SubstrateId) -> PrepNeeds {
        use crate::signals::{
            bracket_balance::BracketSubstrate, casing::CasingSubstrate,
            lexical::DuplicateWordSubstrate, lexical::PunctOnlySubstrate,
            lexical::RepeatedRunSubstrate, mixed_case::MixedCaseSubstrate,
            mixed_normalization::NormalizationSubstrate, proportionality::ProportionalitySubstrate,
            punctuation::AdjacencySubstrate, punctuation::SpacingSubstrate,
            rare_glyph::GlyphSubstrate, script_mixing::MixedScriptSubstrate,
            untranslated_words::UntranslatedWordsSubstrate,
        };
        match id {
            SubstrateId::Spacing => SpacingSubstrate::NEEDS,
            SubstrateId::Adjacency => AdjacencySubstrate::NEEDS,
            SubstrateId::RepeatedRun => RepeatedRunSubstrate::NEEDS,
            SubstrateId::PunctOnly => PunctOnlySubstrate::NEEDS,
            SubstrateId::MixedScript => MixedScriptSubstrate::NEEDS,
            SubstrateId::Glyph => GlyphSubstrate::NEEDS,
            SubstrateId::Proportionality => ProportionalitySubstrate::NEEDS,
            SubstrateId::Normalization => NormalizationSubstrate::NEEDS,
            SubstrateId::Bracket => BracketSubstrate::NEEDS,
            SubstrateId::DuplicateWord => DuplicateWordSubstrate::NEEDS,
            SubstrateId::Casing => CasingSubstrate::NEEDS,
            SubstrateId::MixedCase => MixedCaseSubstrate::NEEDS,
            SubstrateId::UntranslatedWords => UntranslatedWordsSubstrate::NEEDS,
        }
    }
}
