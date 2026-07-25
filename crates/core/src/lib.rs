//! `ssc-core` — a pure, addressable content analyzer.
//!
//! sous receives verse *text* (onion's lossless vref projection) and
//! returns *ranges into that text*. It reads no files and calls no onion:
//! onion is the single segmenter of record for **source → verse text**, and
//! core never re-derives that. Core *does* segment verse text into grapheme
//! clusters internally (ADR 0021) to drive the grapheme-level rules, but that
//! is over the text it was handed and its findings are still ranges into that
//! same text. Mapping a returned range back to a DOM node or source offset is
//! the orchestrator's job, via onion's `segments`. See ADR 0010 and
//! `documentation/overview/v1-reset-design.md`.

pub mod analysis;
mod cache;
pub mod catalog;
pub mod census;
pub mod charclass;
mod charclass_table;
pub mod config;
pub mod corpus;
pub mod diagnostics;
mod evidence;
pub mod grapheme;
pub mod identity;
mod interner;
pub mod key;
pub mod rule;
pub mod script;
pub mod signals;
pub mod span;
pub mod stats;
mod stream;
mod substrate;
mod tape;
pub mod token;
pub mod unicode;

pub use cache::AnalysisCache;
#[cfg(any(test, feature = "test-probes"))]
pub use cache::CacheProbe;

/// Coarse map/reduce/judge phase timings for the warm-path measurement
/// harness (the `bench-probes` feature only — never compiled into a release
/// build, so there are no unconditional production timers). `transition`
/// records the most recent call's phase split into a thread-local; a serial
/// bench (the warm ladder) reads it right after `analyze`.
#[cfg(feature = "bench-probes")]
pub mod bench {
    use std::cell::Cell;
    use std::time::Duration;

    /// One analyze call's coarse phase split. `map` covers the per-verse lane
    /// plus the fused walk and slotting; `reduce` covers stats/site assembly,
    /// the token cache, and project emission; `judge` covers the stateful
    /// judge loop. Pack/reconcile do not exist until Phase A-W.
    #[derive(Clone, Copy, Default, Debug)]
    pub struct PhaseTimings {
        pub map: Duration,
        pub reduce: Duration,
        pub judge: Duration,
    }

    thread_local! {
        static LAST: Cell<PhaseTimings> = const {
            Cell::new(PhaseTimings {
                map: Duration::ZERO,
                reduce: Duration::ZERO,
                judge: Duration::ZERO,
            })
        };
    }

    pub(crate) fn record(t: PhaseTimings) {
        LAST.with(|c| c.set(t));
    }

    /// The phase split of the most recent `transition` on this thread.
    pub fn last() -> PhaseTimings {
        LAST.with(Cell::get)
    }

    /// The shipped chapter-fan-out work threshold, and the override the
    /// serial-vs-chapter-parallel calibration uses to time both routes in one
    /// alternating run. A route is a wall-clock decision only — moving this
    /// cannot change a finding — which is why an override is safe to expose to a
    /// measurement build and to nothing else.
    pub use crate::rule::{PARALLEL_MIN_CHAPTER_MAP_BYTES, set_chapter_map_min_bytes};
}
#[cfg(feature = "bench-probes")]
pub use stream::{FloorNeeds, walk_floor};
pub use catalog::{RuleCard, SENSITIVITY_STOPS, Verdict, rule_cards};
pub use census::{CensusOptions, Inventory, census};
pub use config::{
    BracketBalanceConfig, CasingConfig, Config, ProportionalityConfig, PunctOnlyTokenConfig,
    PunctuationAdjacencyConfig, PunctuationSpacingConfig, RepeatedCharacterRunConfig,
};
pub use corpus::{BookBlock, ChapterBlock, Corpus, CorpusError, KeyIdx, MutationEffect};
pub use diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, InputDependency,
    LengthRatioScope, RuleId, Severity,
};
pub use identity::{ANALYSIS_ENGINE_STAMP, AnalysisId, TargetContextId};
pub use span::{GraphemeSpan, Span, Utf16Span};
pub use stats::{RuleStats, SOURCE_NONE, Stats, Tally};

use corpus::LocalKeyIdx;

/// Analyze a corpus with the shipped default rule set.
///
/// Convenience over [`analyze_with_config`] with [`Config::v1_defaults`]
/// (language-agnostic rules on; convention-dependent rules off, opt-in).
/// `target` is the verses to check; `source` is an optional parallel
/// corpus for source-relative rules (proportionality). The map's scope is
/// the analysis scope: pass a verse, a book, or a whole project.
pub fn analyze(target: &Corpus, source: Option<&Corpus>) -> Vec<Finding> {
    analyze_with_config(target, source, &Config::v1_defaults())
}

/// One chapter's direct (per-verse) rule records, addressed **chapter-locally**
/// — the verse's index within this chapter plus the verse-local span. No global
/// index is computed, so the product is position-independent: it stays valid and
/// correctly addressed wherever the chapter later sits in the corpus.
///
/// Each verse's scalar tape (ADR 0045) is built once into the reused `tape`
/// buffer and shared by every per-verse rule. Records come out in emission order:
/// verse ascending, then per-verse registry order within a verse — the order the
/// stable final sort must preserve among equal keys.
fn chapter_verse_records(
    texts: &[String],
    per_verse: &[Box<dyn rule::PerVerseRule>],
) -> Vec<cache::CachedPerVerseFinding> {
    let mut out = Vec::new();
    let mut tape = Vec::new();
    for (vi, text) in texts.iter().enumerate() {
        let local_idx = LocalKeyIdx::from_usize(vi);
        let mask = tape::build_masked(text, &mut tape);
        for r in per_verse {
            // Skip the clean majority: a rule runs only when the verse's
            // dirty-bits mask opens its gate (ADR 0046). The gate is a safe
            // superset of the fire set, so this never drops a finding.
            if !mask.opens(r.gate()) {
                continue;
            }
            let (code, severity) = (r.id(), r.severity());
            for range in r.check(text, &tape) {
                out.push(cache::CachedPerVerseFinding {
                    local_idx,
                    code,
                    severity,
                    range,
                });
            }
        }
    }
    out
}

/// One dirty chapter's direct-lane map work: its identity, its validity hash, and
/// the verse texts to map. It carries no book position and no global base —
/// mapping a chapter cannot depend on where the chapter sits.
struct DirectWork<'a> {
    slug: &'a str,
    chapter: &'a str,
    hash: u128,
    texts: &'a [String],
}

/// Analyze a corpus, running only the rules `config` enables.
///
/// A rule the config disables is skipped *before it runs* — disabling
/// saves the compute, it isn't a post-filter on findings (ADR 0012).
pub fn analyze_with_config(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
) -> Vec<Finding> {
    // The one-shot sugar over the stateful entry point: no prior, discard
    // the returned stats (ADR 0017).
    analyze_stateful(target, source, config, None, None).0
}

/// A resident analysis attempt that did not run to completion.
///
/// In a released engine the map/reduce/judge transition is *total* — it has no
/// failure path — so this is only ever produced by the test-only [`fault`] hook
/// (present under `test`/`test-probes`, absent from release builds). The type
/// itself is unconditional so the resident entrypoint ([`analyze_resident`]) and
/// [`Galley`](../ssc_galley/struct.Galley.html)'s fallible analyze keep one
/// signature across every build; in release it is simply never constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnalyzeError {
    /// The pipeline boundary the attempt stopped at: `"map"`, `"reduce"`, or
    /// `"judge"`. Diagnostic only.
    pub phase: &'static str,
}

/// Test-only fault injection for the resident analysis transition.
///
/// The whole module is gated behind `test`/`test-probes`, so it does **not**
/// exist in release builds — a released [`analyze_resident`] therefore has no
/// failure path at all (the fault polls in the transition compile to nothing).
/// It exists so tests can prove the [`Galley`](../ssc_galley/struct.Galley.html)
/// lifecycle's retry-safety: a failed attempt commits no partial semantic state,
/// and a retry with no further mutation still reaches the cold result.
#[cfg(any(test, feature = "test-probes"))]
pub mod fault {
    use std::cell::Cell;

    /// The pipeline boundary at which to inject a one-shot failure.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Phase {
        /// After the fused map walk completes (per-verse findings + substrate
        /// observations), before any reduce.
        Map,
        /// After reduce (fresh substrate stats folded), before judging.
        Reduce,
        /// After the stateful judge loop and provenance stamping — the deepest
        /// semantic boundary. Judging ran and the working stats were fully
        /// built; the fault proves none of it escapes as a published result.
        Judge,
    }

    thread_local! {
        static ARMED: Cell<Option<Phase>> = const { Cell::new(None) };
    }

    /// Arm a one-shot fault at `phase` on the current thread. The returned
    /// guard disarms on drop, so a fault that is never reached (e.g. its phase
    /// is skipped because every consuming rule is disabled) cannot leak into a
    /// later call on the same thread. Bind it to a named local (`let _guard =`),
    /// never `let _ =`, or it disarms immediately.
    #[must_use]
    pub fn arm(phase: Phase) -> Guard {
        ARMED.with(|c| c.set(Some(phase)));
        Guard
    }

    /// Disarms the thread-local fault on drop.
    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            ARMED.with(|c| c.set(None));
        }
    }

    /// Fire-once: returns `true` (and immediately disarms) the first time the
    /// armed phase is polled by the transition. Crate-internal — only the engine
    /// polls it, and the fire-once + guard-on-drop pair keep an armed fault from
    /// reaching a later cold-referee call.
    pub(crate) fn fires(phase: Phase) -> bool {
        ARMED.with(|c| {
            if c.get() == Some(phase) {
                c.set(None);
                true
            } else {
                false
            }
        })
    }
}

/// One book's fused-walk products for this analysis, sourced two ways:
/// - [`Walked`](BookProducts::Walked): a freshly walked book — owns its
///   [`BookOut`](stream::BookOut) (fresh stats + sites), consumed this call.
/// - [`Clean`](BookProducts::Clean): a clean cache-hit book — borrows its
///   resident [`BookEntry`](cache::BookEntry) read-only. Its walk products are
///   never copied out of the cache; the judge reads a view.
///
/// The two variants differ only in the site lanes' shape — a walked book still
/// carries the fresh per-book *stats* half beside its sites, while the cache
/// stores sites alone — so stats/site extraction matches per variant. The
/// project lanes (bracket / duplicate / normalization / tokens) share one type
/// across both and are read uniformly by reference.
enum BookProducts<'c> {
    Walked(stream::BookOut),
    Clean(&'c cache::BookEntry),
}

/// The one core map/reduce/judge transition. Both the one-shot path
/// ([`analyze_stateful`], which hands it a fresh transient cache) and the
/// resident path ([`analyze_resident`], which hands it `Galley`'s owned cache
/// and prior) flow through this exact function — there is no second, "simpler"
/// analyzer with its own rule logic (plan §1 decision 16).
///
/// Returns the corpus [`Stats`] so a resident caller can carry it as `prior`
/// for incremental re-analysis (ADR 0017). It is fallible only to carry a
/// test-injected [`fault`]; a released build has no failure path and always
/// returns `Ok`. On the error path it hands the untouched `prior` back so the
/// resident caller can restore it and retry — no partial semantic commit.
///
/// `target` is the **complete** corpus this call answers for. With
/// `prior = Some`, each book present in `target` **supersedes** its prior entry
/// (book granularity) and any prior book **absent** from `target` is dropped —
/// there is no echo carry-forward. A resident `Galley` always supplies its complete
/// corpus, so `prior` is purely an incremental-reuse aid: unchanged books skip
/// re-reduction, they are never a way to answer for a subset.
///
/// **All returned findings cover exactly `target`'s verses.** Stateful rules
/// judge against the merged corpus stats — which now describe exactly
/// `target`'s books — and emit for `target`. (This keeps every finding
/// projectable: the caller hands in the text for the verses it asked about.)
///
/// **Counting is proof-driven, never declared (supersedes ADR 0043's
/// `changed`).** With a `prior`, each supplied book is re-reduced iff its
/// current provenance — content hash, same-slug source hash, and enabled-rule
/// fingerprint — differs from the [`Tally`] the prior recorded for that slug;
/// every matching book carries its prior counts. Judging and emission cover all
/// of `target`, so a convention an edit tips re-emits across every book in one
/// call. There is no `changed` parameter: each book's content hash — owned by
/// `Corpus` and maintained at construction/mutation, so reading it here costs
/// no re-hash — is compared every call, buying a correctness no promise could:
/// the caller cannot under-declare an edit. Without a `prior` there is nothing
/// to carry, so every supplied book counts.
fn transition(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    prior: Option<Stats>,
    cache: &mut AnalysisCache,
) -> Result<(Vec<Finding>, Stats), (AnalyzeError, Option<Stats>)> {
    use std::borrow::Cow;
    use std::collections::BTreeMap;

    let all_per_verse = rule::per_verse_rules();
    // Every direct-lane rule id, enabled or not: the complete set of partitions
    // the direct lane owns. Taken from the registry (not a hand-kept list) so a
    // new per-verse rule cannot silently fall between the two partition lanes.
    let direct_ids: Vec<RuleId> = all_per_verse.iter().map(|r| r.id()).collect();
    let per_verse: Vec<_> = all_per_verse
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    let stateful: Vec<_> = rule::stateful_rules(config)
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();

    // Classification is a static fused table (ADR 0021, amending 0020): a
    // process-wide `class_of` lookup, so there is nothing to build or thread
    // per analyze.

    // The fused walk plan: which listeners the enabled rule set puts on the
    // one book walk (the event-stream engine — see `stream`). Both casing
    // rules share one word-table walk; the project rules (bracket-balance,
    // duplicate-word) ride the same walk as always-on listeners over every
    // supplied book. `collect_tokens` retains each verse's tokenization as
    // the shared cache for the token-consuming judges (repeated-run's
    // containing-word lookup; rare-glyph / mixed-case / mixed-script re-scans).
    // Which typed observation substrates are active — computed once, before
    // mapping, from the closed registry and the final coalesced config (plan
    // §5.3). `punct.spacing-anomaly` is a substrate now, so it is absent from
    // the fused walk plan below; it maps/reduces through its own stamp-derived
    // cache after the stateful judge loop.
    let active = substrate::ActiveSubstrates::from_config(config);
    let plan = stream::WalkPlan {
        adjacency: config.is_enabled(RuleId::PunctuationAdjacencyAnomaly),
        repeated_run: config.is_enabled(RuleId::RepeatedCharacterRun),
        punct_only: config.is_enabled(RuleId::PunctOnlyToken),
        mixed_script: config.is_enabled(RuleId::MixedScriptInToken),
        rare_glyph: config.is_enabled(RuleId::RareGlyph),
        mixed_case: config.is_enabled(RuleId::MixedCaseWord),
        proportionality: config.is_enabled(RuleId::ProjectLengthRatio),
        bracket: config.is_enabled(RuleId::BracketBalance),
        normalization: config.is_enabled(RuleId::MixedNormalization),
        collect_tokens: config.is_enabled(RuleId::RepeatedCharacterRun)
            || config.is_enabled(RuleId::RareGlyph)
            || config.is_enabled(RuleId::MixedCaseWord)
            || config.is_enabled(RuleId::MixedScriptInToken),
    };

    // The book view is shared by both analysis lanes. `source` is intentionally
    // not part of the cache fingerprint: it feeds only proportionality counting,
    // counting never reads the cache, and no cached lane depends on source.
    let books = corpus::by_book(target);
    cache.ensure_fingerprint(config);
    // Every supplied book's content hash, read from the corpus's OWNED layout
    // rather than re-hashed per call: `Corpus` computes these at construction
    // and every mutation, so a stateless construction is still fresh proof and
    // the analysis path no longer walks verse text to hash it. `books` is
    // `by_book(target)`, which reads the same layout, so it is index-aligned
    // with the layout's presented order.
    let hashes: Vec<u128> = target.book_layout().iter().map(|b| b.hash).collect();
    // Source book hashes by slug, for per-book source provenance, read from the
    // reference corpus's owned layout. A target book pairs only with the
    // same-slug source book (its keys parse to its slug), so that is the only
    // source text its counts depend on.
    let source_hashes: Option<BTreeMap<&str, u128>> = source.map(|s| {
        s.book_layout()
            .iter()
            .map(|b| (&*b.slug, b.hash))
            .collect()
    });
    // Fingerprint of the enabled counting-rule set: records which rules'
    // contributions a book's counts include, so toggling any rule re-tallies.
    let rules_fp = rules_fp(&stateful);

    // Coarse phase decomposition for the warm-path harness (`bench-probes`
    // only — no release timers). `map` runs from here through the MAP boundary.
    #[cfg(feature = "bench-probes")]
    let bench_map_start = std::time::Instant::now();

    // The per-verse phase is embarrassingly parallel — each verse is judged
    // from its own text by `Sync` rules. Under the `parallel` feature it fans
    // out over rayon (ADR 0018); otherwise it stays serial. Output is the same
    // either way: `out` is sorted before return, so order never depends on the
    // feature.
    // The verse's scalar tape (ADR 0045) is built once per verse into a reused
    // buffer — a `map_init` per-worker buffer under `parallel`, a plain reused
    // `Vec` serially — and shared by every per-verse rule, replacing their ~10
    // separate `char_indices()` walks with one decode+classify pass.
    // The one core transition always maps through the cache — the one-shot path
    // hands in a fresh empty one (plan §1 decision 16), so it is all misses and
    // maps every book exactly as a no-cache walk would, then drops the cache.
    // The direct (per-verse) lane's planning pass. A per-verse rule reads one
    // verse and nothing else, so its map unit is the smallest thing a mutation
    // replaces: a chapter. A chapter is dirty iff its cached product was not
    // derived from this exact chapter content — stamp-derived, never a caller
    // dirty hint, because `Corpus` owns the chapter hashes and maintains them at
    // every mutation. A cold call finds nothing cached and so marks every
    // chapter dirty; a one-chapter edit marks exactly that chapter.
    // Two independent stamps, two dirty sets, unioned: a chapter is *mapped* when
    // its cached product does not match this chapter's content, and its committed
    // records are *patched* when the partition lane's own stamp does not — which
    // a failed attempt can leave behind after warming prep (§3.3 retry safety).
    // Mapping is therefore never inferred from the partition stamp, nor patching
    // from prep's warm state.
    //
    // The pass visits every chapter of every book, so it must stay cheap per
    // chapter: both lanes' per-book maps are hoisted out of the inner loop (one
    // slug hash per book, not per chapter), and nothing whole-corpus is built
    // unless the O(1) count check below proves something stale is retained.
    let target_texts = target.texts();
    let mut direct_work: Vec<DirectWork<'_>> = Vec::new();
    let mut direct_book_runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut direct_dirty: Vec<(Box<str>, Box<str>, u128)> = Vec::new();
    // The dirty work's size, for the map seam's route decision only. Summing
    // already-known string lengths, so it costs one integer add per dirty verse
    // and reads no text.
    let mut direct_bytes = 0usize;
    let mut chapter_count = 0usize;
    #[cfg(any(test, feature = "test-probes"))]
    let mut direct_hits = 0usize;
    for book in target.book_layout() {
        let run_start = direct_work.len();
        let cached = cache.prep.direct_book(&book.slug);
        let stamps = cache.findings.direct_stamps_for(&book.slug);
        for chapter in &book.chapters {
            chapter_count += 1;
            let map_needed = cached
                .and_then(|book| book.get(&*chapter.chapter))
                .is_none_or(|c| !c.matches(chapter.hash));
            if map_needed {
                let texts = &target_texts[chapter.range.clone()];
                direct_bytes += texts.iter().map(String::len).sum::<usize>();
                direct_work.push(DirectWork {
                    slug: &book.slug,
                    chapter: &chapter.chapter,
                    hash: chapter.hash,
                    texts,
                });
            } else {
                #[cfg(any(test, feature = "test-probes"))]
                {
                    direct_hits += 1;
                }
            }
            let committed = stamps.and_then(|book| book.get(&*chapter.chapter));
            if map_needed || committed != Some(&chapter.hash) {
                direct_dirty.push((
                    Box::from(&*book.slug),
                    Box::from(&*chapter.chapter),
                    chapter.hash,
                ));
            }
        }
        if direct_work.len() > run_start {
            direct_book_runs.push(run_start..direct_work.len());
        }
    }
    #[cfg(any(test, feature = "test-probes"))]
    cache.note_direct(direct_hits, direct_work.len());
    // Map the dirty chapters (one Rayon grain — see `map_chapter_work`) and warm
    // each product into the lane. The records are chapter-local, so they are
    // stored exactly as produced; nothing is rebased here.
    let direct_route = rule::map_route(&direct_book_runs, direct_work.len(), direct_bytes);
    #[cfg(any(test, feature = "test-probes"))]
    cache.note_direct_route(direct_route);
    let fresh = rule::map_chapter_work(&direct_work, &direct_book_runs, direct_route, |w| {
        chapter_verse_records(w.texts, &per_verse)
    });
    for (w, records) in direct_work.iter().zip(fresh) {
        cache.store_direct_chapter(w.slug, w.chapter, w.hash, records);
    }
    // Removal invalidation, entered only when it can possibly apply. Every
    // chapter the corpus presents is now resident in both lanes, so a resident
    // count above the corpus's chapter count is exactly the signal that a chapter
    // left the corpus (a whole-book replacement dropping one) and stale products,
    // records and stamps must go. Equal counts prove there is nothing stale, in
    // O(1) — a whole-corpus chapter set built every analyze would be a fixed cost
    // on every edit, however small.
    let direct_stale = cache.prep.direct_chapter_count() > chapter_count
        || cache.findings.direct_stamp_count() > chapter_count;
    let direct_present: Option<std::collections::BTreeSet<(&str, &str)>> = direct_stale.then(|| {
        target
            .book_layout()
            .iter()
            .flat_map(|b| b.chapters.iter().map(|c| (&*b.slug, &*c.chapter)))
            .collect()
    });
    if let Some(present) = direct_present.as_ref() {
        cache.retain_direct(|slug, chapter| present.contains(&(slug, chapter)));
    }

    // The direct lane's findings never enter this working buffer: they live in
    // their rules' partitions, patched per chapter below. `out` collects only the
    // batch-lane rules' findings.
    let mut out: Vec<Finding> = Vec::new();

    // The by-book view, computed once and shared by the project and stateful
    // phases — the book is the corpus-scoped unit (supersede granularity,
    // cross-verse seams, and the `parallel` fan-out; ADR 0042). Rules never
    // rebuild this grouping.
    // The reduce scope, now derived from provenance instead of declared: a book
    // is counted iff its current Tally differs from the prior's record for that
    // slug (a missing entry is a mismatch). The walk still visits every supplied
    // book (the project listeners' and the token cache's scope); `counted` only
    // gates the counting listeners. Without a prior there is nothing to carry,
    // so every supplied book is counted.
    let current: Vec<Tally> = books
        .iter()
        .enumerate()
        .map(|(i, g)| Tally {
            text: hashes[i],
            source: source_hashes
                .as_ref()
                .and_then(|m| m.get(g.slug).copied())
                .unwrap_or(SOURCE_NONE),
            rules: rules_fp,
        })
        .collect();
    let stale: Option<Vec<&str>> = prior.as_ref().map(|p| {
        books
            .iter()
            .enumerate()
            .filter(|&(i, g)| p.tallied.get(g.slug) != Some(&current[i]))
            .map(|(_, g)| g.slug)
            .collect()
    });
    let counted: Option<&[&str]> = stale.as_deref();

    // A walk-product hit is safe only for a clean book in the complete
    // snapshot shape. Cold calls (no prior) must walk every supplied book so
    // their counting and emission semantics remain exactly unchanged.
    //
    // `fused` ends up index-aligned with `books` (its presented order): a
    // walked book lands at its original position via `walk_positions`; a
    // cache-hit book is synthesized directly into that position. Never
    // reassembled by book identity — `walk_fused`'s output is aligned only
    // to whatever subset of `books` it was given.
    // Decide per book: reuse a clean resident walk, or walk it fresh. A book is
    // clean only when it is outside the counted (stale) set AND its cache entry
    // already holds every lane this plan needs. A clean book is reused by
    // BORROWING its resident `BookEntry` into the judge phase — the old
    // `cloned_walk` per-book copy is gone: the cache keeps the
    // single owned copy of a clean book's products, and the judge reads a view.
    let mut books_to_walk: corpus::Books<'_> = Vec::new();
    let mut walk_positions: Vec<usize> = Vec::new();
    let mut clean_positions: Vec<usize> = Vec::new();
    for (i, group) in books.iter().enumerate() {
        if counted.is_some_and(|list| !list.contains(&group.slug))
            && cache.walk_lanes_ready(group.slug, hashes[i], &plan)
        {
            clean_positions.push(i);
        } else {
            books_to_walk.push(*group);
            walk_positions.push(i);
        }
    }

    // ONE walk per verse per book (the event-stream engine): tape, graphemes
    // and tokens are each built once per verse and every enabled listener is
    // fed in-pass. Fan-out per book under `parallel` (ADR 0042).
    let walked = stream::walk_fused(&books_to_walk, counted, source, &plan);

    // Warm each freshly walked book into the (self-validating) cache BEFORE any
    // clean book is borrowed out of it: after this loop the cache is read-only
    // for the rest of the call, so the clean-book borrows below cannot alias a
    // pending write.
    for ((&i, group), output) in walk_positions
        .iter()
        .zip(books_to_walk.iter())
        .zip(walked.iter())
    {
        cache.store_walk(group.slug, hashes[i], output);
    }

    // Counting-side probe: count the books whose site-free counting
    // accumulators actually ran (`counting_accs_ran`), observed from the
    // accumulators — not from the `counted` decision flag. Only a freshly
    // walked book can count; a clean cache-hit book did no counting work.
    // Recorded before the shared cache reborrow below (the last `&mut cache`).
    #[cfg(any(test, feature = "test-probes"))]
    cache.note_retallied(walked.iter().filter(|o| o.counting_accs_ran).count());

    // MAP boundary. Mapping is complete: per-verse findings plus every book's
    // fused walk products — freshly walked books owned in `walked`, clean books
    // resident in the cache, all warmed. No reduce/judge has run, and `prior` is
    // untouched — a test-injected fault here hands `prior` straight back so the
    // resident caller restores it and a retry reuses the warmed entries.
    // Compiles to nothing off `test-probes`.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Map) {
        return Err((AnalyzeError { phase: "map" }, prior));
    }

    // MAP boundary reached: `reduce` runs from here through the JUDGE boundary.
    #[cfg(feature = "bench-probes")]
    let bench_reduce_start = std::time::Instant::now();

    // The prep section is now read-only for the rest of the call. Split the
    // cache into its independently-borrowable sections: `prep` stays shared for
    // the whole reduce+judge phase (that it compiles is the proof no judge
    // mutates a cached map product — every cached lane a judge sees is behind a
    // `&`), while `finding_lane` stays mutable so the new partitions can be
    // committed AFTER judge succeeds (the atomic finding boundary). They are
    // disjoint fields, so the shared prep borrow and the mutable finding borrow
    // coexist.
    let cache::AnalysisCache {
        prep,
        substrates,
        findings: finding_lane,
    } = &mut *cache;
    let prep: &cache::PrepSection = prep;

    // Slot every book's products in presented order: a freshly walked book owns
    // its `BookOut`; a clean book borrows its resident `BookEntry`.
    let mut slots: Vec<Option<BookProducts<'_>>> = (0..books.len()).map(|_| None).collect();
    for (&pos, output) in walk_positions.iter().zip(walked) {
        slots[pos] = Some(BookProducts::Walked(output));
    }
    for &pos in &clean_positions {
        slots[pos] = Some(BookProducts::Clean(prep.walk_entry(books[pos].slug)));
    }
    let mut slots: Vec<BookProducts<'_>> = slots
        .into_iter()
        .map(|s| s.expect("every book walked or clean-reused"))
        .collect();

    // Assemble each rule's fresh stats + forwarded sites (ADR 0044) from the
    // fused per-book outputs. A rule enabled this call always gets an entry —
    // possibly empty — exactly as its own reduce produced. A book outside the
    // `counted` scope contributes **sites only** (the walk visited it for
    // anchors; its counts carry from the prior through the supersede merge),
    // so the judge phase is site-driven for every supplied book and never
    // re-scans — except the deliberately site-free rules (proportionality
    // never scans; rare-glyph / mixed-case re-scan by design, ADR 0053/0055).
    // A walked book contributes fresh stats (moved out, owned) and its fresh
    // sites (`Cow::Owned`); a clean book contributes NO stats (its counts carry
    // from the prior through the supersede merge) and its sites as a
    // `Cow::Borrowed` view into the resident cache — never copied (Phase A
    // step 7). Site-free rules (rare-glyph / mixed-case / proportionality) only
    // ever count on walked books, so a clean book contributes nothing to them.
    // A book outside the `counted` scope contributes sites only, so the judge
    // phase stays site-driven for every supplied book (ADR 0044).
    let mut adjacency_fresh = plan.adjacency.then(|| {
        let mut pb = BTreeMap::new();
        let mut st: BTreeMap<Box<str>, Cow<'_, [corpus::SiteAddr]>> = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            match slot {
                BookProducts::Walked(o) => {
                    if let Some((bc, s)) = o.adjacency.take() {
                        if o.counted {
                            pb.insert(Box::from(group.slug), bc);
                        }
                        st.insert(Box::from(group.slug), Cow::Owned(s));
                    }
                }
                BookProducts::Clean(e) => {
                    let e: &cache::BookEntry = e;
                    if let Some(s) = e.adjacency.as_ref() {
                        st.insert(Box::from(group.slug), Cow::Borrowed(s.as_slice()));
                    }
                }
            }
        }
        (
            RuleStats::PunctuationAdjacency(signals::punctuation::PunctuationAdjacencyStats {
                per_book: pb,
            }),
            rule::RuleSites::PunctuationAdjacency(st),
        )
    });
    let mut repeated_fresh = plan.repeated_run.then(|| {
        let mut pb = BTreeMap::new();
        let mut st: BTreeMap<Box<str>, Cow<'_, [corpus::SiteAddr]>> = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            match slot {
                BookProducts::Walked(o) => {
                    if let Some((bc, s)) = o.repeated_run.take() {
                        if o.counted {
                            pb.insert(Box::from(group.slug), bc);
                        }
                        st.insert(Box::from(group.slug), Cow::Owned(s));
                    }
                }
                BookProducts::Clean(e) => {
                    let e: &cache::BookEntry = e;
                    if let Some(s) = e.repeated_run.as_ref() {
                        st.insert(Box::from(group.slug), Cow::Borrowed(s.as_slice()));
                    }
                }
            }
        }
        (
            RuleStats::RepeatedCharacterRun(signals::lexical::RepeatedCharacterRunStats {
                per_book: pb,
            }),
            rule::RuleSites::RepeatedCharacterRun(st),
        )
    });
    let mut punct_only_fresh = plan.punct_only.then(|| {
        let mut pb = BTreeMap::new();
        let mut st: BTreeMap<Box<str>, Cow<'_, [corpus::SiteAddr]>> = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            match slot {
                BookProducts::Walked(o) => {
                    if let Some((bc, s)) = o.punct_only.take() {
                        if o.counted {
                            pb.insert(Box::from(group.slug), bc);
                        }
                        st.insert(Box::from(group.slug), Cow::Owned(s));
                    }
                }
                BookProducts::Clean(e) => {
                    let e: &cache::BookEntry = e;
                    if let Some(s) = e.punct_only.as_ref() {
                        st.insert(Box::from(group.slug), Cow::Borrowed(s.as_slice()));
                    }
                }
            }
        }
        (
            RuleStats::PunctOnlyToken(signals::lexical::PunctOnlyTokenStats { per_book: pb }),
            rule::RuleSites::PunctOnlyToken(st),
        )
    });
    let mut mixed_script_fresh = plan.mixed_script.then(|| {
        let mut pb = BTreeMap::new();
        let mut st: BTreeMap<Box<str>, Cow<'_, [signals::script_mixing::MixedScriptSite]>> =
            BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            match slot {
                BookProducts::Walked(o) => {
                    if let Some((bc, s)) = o.mixed_script.take() {
                        if o.counted {
                            pb.insert(Box::from(group.slug), bc);
                        }
                        st.insert(Box::from(group.slug), Cow::Owned(s));
                    }
                }
                BookProducts::Clean(e) => {
                    let e: &cache::BookEntry = e;
                    if let Some(s) = e.mixed_script.as_ref() {
                        st.insert(Box::from(group.slug), Cow::Borrowed(s.as_slice()));
                    }
                }
            }
        }
        (
            RuleStats::MixedScript(signals::script_mixing::MixedScriptStats { per_book: pb }),
            rule::RuleSites::MixedScript(st),
        )
    });
    let mut rare_glyph_fresh = plan.rare_glyph.then(|| {
        let mut pb = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            if let BookProducts::Walked(o) = slot
                && let Some(bg) = o.rare_glyph.take()
            {
                pb.insert(Box::from(group.slug), bg);
            }
        }
        (
            RuleStats::GlyphInventory(signals::rare_glyph::RareGlyphStats { per_book: pb }),
            rule::RuleSites::RareGlyph,
        )
    });
    let mut mixed_case_fresh = plan.mixed_case.then(|| {
        let mut pb = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            if let BookProducts::Walked(o) = slot
                && let Some(bmc) = o.mixed_case.take()
            {
                pb.insert(Box::from(group.slug), bmc);
            }
        }
        (
            RuleStats::MixedCase(signals::mixed_case::MixedCaseStats { per_book: pb }),
            rule::RuleSites::MixedCase,
        )
    });
    let mut proportionality_fresh = plan.proportionality.then(|| {
        let mut pb = BTreeMap::new();
        for (group, slot) in books.iter().zip(slots.iter_mut()) {
            if let BookProducts::Walked(o) = slot
                && let Some(bucket) = o.proportionality.take()
            {
                pb.insert(Box::from(group.slug), bucket);
            }
        }
        (
            RuleStats::Proportionality(signals::proportionality::ProportionalityStats {
                per_book: pb,
            }),
            rule::RuleSites::Proportionality,
        )
    });

    // The shared token cache is assembled by BORROWING each book's per-verse
    // token slices — a walked book's owned `BookOut`, a clean book's resident
    // `BookEntry` — and rebasing the local index to this call's global `KeyIdx`.
    // Nothing is copied out of the cache: the cache holds
    // `&[Token]` views. Built after site extraction so it can share `slots`.
    let token_cache: Option<rule::TokenCache> = plan.collect_tokens.then(|| {
        let mut tc = rule::TokenCache::default();
        for (group, slot) in books.iter().zip(slots.iter()) {
            let toks = match slot {
                BookProducts::Walked(o) => o.tokens.as_ref(),
                BookProducts::Clean(e) => e.tokens.as_ref(),
            };
            if let Some(vs) = toks {
                for (local, t) in vs {
                    tc.insert(corpus::rebase(group.base, *local), t.as_slice());
                }
            }
        }
        tc
    });

    // Project findings, read by reference from each book's products (walked or
    // clean). `out` is sorted before return, so appending these before the
    // stateful judges below does not affect the final order.
    if plan.bracket {
        let matches: Vec<&signals::bracket_balance::BookMatch> = slots
            .iter()
            .map(|slot| {
                match slot {
                    BookProducts::Walked(o) => o.bracket.as_ref(),
                    BookProducts::Clean(e) => e.bracket.as_ref(),
                }
                .expect("bracket listener ran on every book")
            })
            .collect();
        out.extend(signals::bracket_balance::emit(
            &books,
            &matches,
            &config.bracket_balance,
        ));
    }
    if plan.normalization {
        let summaries: Vec<&signals::mixed_normalization::BookNormalization> = slots
            .iter()
            .map(|slot| {
                match slot {
                    BookProducts::Walked(o) => o.normalization.as_ref(),
                    BookProducts::Clean(e) => e.normalization.as_ref(),
                }
                .expect("normalization listener ran on every book")
            })
            .collect();
        out.extend(signals::mixed_normalization::emit(&books, &summaries));
    }

    // REDUCE boundary. Every substrate's fresh book/corpus stats are folded and
    // the project findings are emitted into the local `out`; nothing resident is
    // committed yet and `prior` is still untouched. A fault here hands `prior`
    // back intact — retry reuses the warm map cache and re-runs reduce/judge.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Reduce) {
        return Err((AnalyzeError { phase: "reduce" }, prior));
    }

    // JUDGE boundary reached: `judge` runs from here through the stateful loop.
    // (The judge fault fires AFTER the loop and provenance stamping — the
    // rollback copy taken below is what makes that deep injection safe.)
    #[cfg(feature = "bench-probes")]
    let bench_judge_start = std::time::Instant::now();

    // Stateful rules: supersede the prior cache at book granularity, judge the
    // whole merged corpus from the cache.
    //
    // Deliberately sequential over rules: pooling all rules' reduces/judges
    // into two rule×book task pools was tried (2026-07-07) and measured at
    // parity-to-slightly-worse (see ADR 0042's rejected alternatives). The
    // counting itself now happens once, fused, above.
    // Test-only rollback copy: the judge fault fires AFTER judging and
    // provenance stamping, by which point `prior` is long consumed into the
    // working stats. The clone exists only under test cfgs — release builds
    // carry no copy and no judge failure path.
    #[cfg(any(test, feature = "test-probes"))]
    let fault_rollback = prior.clone();

    let mut stats = prior.unwrap_or_default();

    // Complete-snapshot semantics: a target answers for EXACTLY its books. Any prior book absent from this target is
    // dropped before merge/judge — there is no echo carry-forward of
    // old-not-current contributions. Every resident caller (`Galley`) supplies
    // its complete corpus and drops deleted books explicitly, so this only ever
    // prunes a genuinely deleted book; the merged corpus, findings, and
    // provenance for present books are unchanged (the byte-identical oracle
    // gate confirms). `tallied` is authoritative for a prior book's presence:
    // every counted book is stamped there, so pruning by it also clears that
    // book from each rule's per-book aggregate.
    let present: std::collections::BTreeSet<&str> = books.iter().map(|g| g.slug).collect();
    let absent: Vec<Box<str>> = stats
        .tallied
        .keys()
        .filter(|slug| !present.contains(slug.as_ref()))
        .cloned()
        .collect();
    for slug in &absent {
        stats.remove_book(slug);
    }

    for r in &stateful {
        let id = r.id();
        let sites_slot;
        // The fused walk's fresh stats + forwarded sites for this rule (ADR
        // 0044). Both casing rules share the one word-table walk: each takes a
        // clone of the stats (the wire shape keeps one entry per rule id, as
        // before) and judges from the same site list.
        let (fresh, sites_ref): (RuleStats, &rule::RuleSites<'_>) = match id {
            RuleId::PunctuationAdjacencyAnomaly => {
                let (st, ss) = adjacency_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::RepeatedCharacterRun => {
                let (st, ss) = repeated_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::PunctOnlyToken => {
                let (st, ss) = punct_only_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::MixedScriptInToken => {
                let (st, ss) = mixed_script_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::RareGlyph => {
                let (st, ss) = rare_glyph_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::MixedCaseWord => {
                let (st, ss) = mixed_case_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::ProjectLengthRatio => {
                let (st, ss) = proportionality_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            other => unreachable!("{other:?} is not a stateful rule"),
        };
        let merged = match stats.take(id) {
            Some(prev) => prev.merge(fresh),
            None => fresh,
        };
        // Judge against the whole merged corpus, but emit only for `target`
        // — keeping the returned findings to one scope and projectable
        // against the text the caller supplied this call. `judge` itself
        // iterates the current call's `books`, so its emission is already
        // scoped to `target`; there is no prior-call index to filter by.
        out.extend(r.judge(&merged, &books, token_cache.as_ref(), Some(sites_ref)));
        stats.insert(id, merged);
    }

    // Typed observation substrates (plan §5.2). Each owns stamp-derived validity
    // independent of the Tally counting model above: it maps only chapters whose
    // observation input stamp changed and re-reduces an owning book only when a
    // chapter changed, so a judging-knob change reuses every observation and
    // reduction (maps/reduces zero) and re-judges from the cached corpus
    // aggregate. A disabled substrate (no active consumer) drops its products so
    // edits while it is inactive do no work for it.
    signals::punctuation::drive_spacing(
        active.spacing,
        &mut substrates.spacing,
        target,
        &config.punctuation_spacing,
        &mut out,
    );
    signals::lexical::drive_duplicate_word(
        active.duplicate_word,
        &mut substrates.duplicate_word,
        target,
        &mut out,
    );
    signals::casing::drive_casing(
        config.is_enabled(RuleId::SentenceInitialLowercase),
        config.is_enabled(RuleId::InconsistentWordCasing),
        signals::casing::CasingState {
            cache: &mut substrates.casing,
            retained: &mut substrates.casing_model,
            symbols: &substrates.words,
        },
        target,
        &config.casing,
        &mut out,
    );

    // Stamp per-book provenance for every supplied book: a freshly-counted book
    // gets its new Tally; a non-stale supplied book gets the identical value it
    // already carried (a no-op by construction). Prior books absent from
    // `target` were already dropped above — the returned `tallied` describes
    // exactly this target's books, nothing more.
    for (i, group) in books.iter().enumerate() {
        stats.tallied.insert(Box::from(group.slug), current[i]);
    }

    // JUDGE boundary. Fires AFTER the stateful judge loop and provenance
    // stamping — the deepest semantic failure point: judging ran and the
    // working stats were fully built, and none of it may escape. The rollback
    // copy (taken before `prior` was consumed) hands back exactly what
    // arrived; the working stats and findings drop right here. The prep cache
    // may stay warm — it is self-validating — but no correctness state was
    // consumed and nothing was published.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Judge) {
        return Err((AnalyzeError { phase: "judge" }, fault_rollback));
    }

    // JUDGE done: record the coarse phase split for the warm-path harness.
    #[cfg(feature = "bench-probes")]
    bench::record(bench::PhaseTimings {
        map: bench_reduce_start - bench_map_start,
        reduce: bench_judge_start - bench_reduce_start,
        judge: std::time::Instant::now() - bench_judge_start,
    });

    // Commit the complete semantic candidate. Both partition lanes commit here,
    // only after map/reduce/judge have all succeeded (past every fault seam
    // above), so a failed analyze leaves the PREVIOUS partitions intact and
    // current — no partial commit. The batch lane decomposes this call's freshly
    // judged findings into chapter-local records; the direct lane replaces the
    // records of exactly the chapters it re-derived and leaves every other
    // chapter's alone.
    finding_lane.rebuild_batch(&out, target, &direct_ids);
    finding_lane.patch_direct(
        &direct_ids,
        prep,
        &direct_dirty,
        direct_dirty.len() == chapter_count,
        direct_present.as_ref(),
    );

    // Assemble the returned findings ONLY from the resident partitions, rebasing
    // each chapter-local record to a global `KeyIdx` against the current corpus.
    // A partition stores no global index — the rebase happens here, once — and
    // the round-trip through partitions is exact, so this is byte-identical to
    // returning `out` directly.
    let mut out = finding_lane.assemble(target);

    // Deterministic order, independent of the `parallel` feature (ADR 0018) and
    // of partition/chapter iteration order. Findings that tie on
    // `(key_idx, range.start, code)` are always one rule at one site; assembly
    // preserved that rule's within-chapter emission order, and this stable sort
    // preserves it among the ties. Cheap against the analysis: one O(n log n).
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));

    Ok((out, stats))
}

/// One-shot / oracle analysis over the [one core transition](transition).
///
/// `cache` is the *transient* half of decision 16: `Some` reuses a caller-owned
/// cache (rare — most callers pass `None`), `None` spins up a fresh empty
/// [`AnalysisCache`] for this one call and drops it. Either way the identical
/// transition runs to completion. This path arms no [`fault`], so the transition
/// is total here — an injected fault reaching it is a test misuse and panics.
///
/// See [`transition`] for the complete-snapshot / provenance / counting
/// semantics (ADR 0017, ADR 0043 supersession).
pub fn analyze_stateful(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    prior: Option<Stats>,
    cache: Option<&mut AnalysisCache>,
) -> (Vec<Finding>, Stats) {
    let outcome = match cache {
        Some(cache) => transition(target, source, config, prior, cache),
        None => transition(target, source, config, prior, &mut AnalysisCache::new()),
    };
    match outcome {
        Ok(result) => result,
        Err((e, _)) => {
            unreachable!("one-shot analysis is total; unexpected injected fault at {}", e.phase)
        }
    }
}

/// Resident analysis over the [one core transition](transition) — the entry the
/// resident shell ([`Galley`](../ssc_galley/struct.Galley.html)) drives.
///
/// It owns and passes its resident `prior` and `cache` (ADR 0010: no engine
/// state lives here). Fallible so the shell can implement a retry-safe
/// clean/dirty lifecycle (plan §3.3): on error the untouched `prior` comes back
/// in the error tuple, so the shell restores it, keeps its (self-validating)
/// warm cache, and a retry with no further mutation reaches the cold result.
/// In release there is no failure path (see [`fault`]); the `Result` shape is
/// uniform so the shell's signature does not change between builds.
pub fn analyze_resident(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    prior: Option<Stats>,
    cache: &mut AnalysisCache,
) -> Result<(Vec<Finding>, Stats), (AnalyzeError, Option<Stats>)> {
    transition(target, source, config, prior, cache)
}

/// Fingerprint the enabled counting-rule set: xxh3-64 over the rules' canonical
/// string ids, sorted and length-prefixed (u8 length + bytes) so distinct id
/// sets cannot collide by textual concatenation. Knob values are excluded —
/// knobs affect judging, not tallying, so a knob-only config change leaves every
/// `Tally.rules` valid and re-tallies nothing.
fn rules_fp(stateful: &[Box<dyn rule::StatefulRule>]) -> u64 {
    let mut ids: Vec<&str> = stateful.iter().map(|r| r.id().code()).collect();
    ids.sort_unstable();
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    for id in ids {
        hasher.update(&[id.len() as u8]);
        hasher.update(id.as_bytes());
    }
    hasher.digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One book's verses (chapter 1, verses 1..) as parallel key/text
    /// vectors — the contiguous block `corpus_of` concatenates. Generic over
    /// `&str`/`String` verse slices so it backs both `mk` and `mks`.
    fn keyed<S: AsRef<str>>(book: &str, verses: &[S]) -> (Vec<String>, Vec<String>) {
        (
            (1..=verses.len())
                .map(|v| format!("{book} 1:{v}"))
                .collect(),
            verses.iter().map(|s| s.as_ref().to_string()).collect(),
        )
    }

    /// Concatenate already-`keyed` book blocks into one `Corpus`, in the
    /// order given — the caller picks that order, and each block stays
    /// contiguous (`Corpus`'s reopened-book invariant).
    fn corpus_of(parts: Vec<(Vec<String>, Vec<String>)>) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for (k, t) in parts {
            keys.extend(k);
            texts.extend(t);
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    fn map(pairs: &[(&str, &str)]) -> Corpus {
        let texts: Vec<&str> = pairs.iter().map(|(_label, text)| *text).collect();
        mk("GEN", &texts)
    }

    /// Verses 1.. of a named book.
    fn mk(book: &str, verses: &[&str]) -> Corpus {
        corpus_of(vec![keyed(book, verses)])
    }

    fn casing_on(emit_score_min: f32, confidence_z: f32) -> Config {
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::SentenceInitialLowercase, true);
        cfg.casing = CasingConfig {
            emit_score_min,
            recurrence_k: 32.0,
            confidence_z,
            ..CasingConfig::default()
        };
        cfg
    }

    /// Verses of a book that fire `case.sentence-initial-lowercase` under the
    /// ADR 0051/0052 model: `n` clean verses teach a capital-after-`.` habit on
    /// the lexicon-lowercase word "the" (every sentence starts "The", "the" also
    /// recurs mid-flow), then one verse writes "the" lowercase after a period.
    /// `n ≥ 30` so the `.` boundary class clears ADR 0052's trust event floor.
    fn casing_fire(n: usize) -> Vec<String> {
        let mut v: Vec<String> = (0..n)
            .map(|_| "The men saw the gate.".to_string())
            .collect();
        v.push("He fell. the gate stood.".to_string());
        v
    }

    fn mks(book: &str, verses: &[String]) -> Corpus {
        corpus_of(vec![keyed(book, verses)])
    }

    /// Findings come back in a stable `(key_idx, range.start, code)` order
    /// regardless of the `parallel` feature (ADR 0018), and analysis is
    /// deterministic across runs. Under `--features parallel` this is the
    /// real guard that the end-of-pipeline sort tames rayon's nondeterministic
    /// collection order; under the serial default it pins the contract. The
    /// cross-*build* equality (feature-on findings == feature-off findings) is
    /// asserted in CI by running the suite under both feature sets.
    /// The closed `InputDependency` registry classifies every `RuleId` exactly
    /// once (the exhaustive match is compiler-enforced; this proves the total
    /// call is panic-free over `RuleId::ALL`), and exactly one rule —
    /// `prop.length-ratio` — reads the reference.
    #[test]
    fn input_dependency_covers_every_rule() {
        let ref_dependent: Vec<RuleId> = RuleId::ALL
            .iter()
            .copied()
            .filter(|r| {
                r.input_dependency() == InputDependency::TargetAndReferenceSilentWhenAbsent
            })
            .collect();
        assert_eq!(ref_dependent, vec![RuleId::ProjectLengthRatio]);
        // Every other rule is TargetOnly (total coverage, no panic).
        for &r in RuleId::ALL {
            let dep = r.input_dependency();
            if r != RuleId::ProjectLengthRatio {
                assert_eq!(dep, InputDependency::TargetOnly, "{r:?}");
            }
        }
    }

    /// Every rule classified `TargetAndReferenceSilentWhenAbsent` emits no
    /// findings at all when no reference is present — the contract the
    /// reference-removal persisted-findings salvage relies on (§5.2 / §A.5).
    #[test]
    fn reference_silent_rules_emit_nothing_without_reference() {
        // A rich, long-enough target so proportionality's min-verse floor is
        // cleared and it *would* fire were a reference present.
        let mut verses: Vec<String> = (0..60)
            .map(|i| format!("verse number {i} with some words to vary the length a bit"))
            .collect();
        verses[0] = "x".to_string(); // a wildly short verse, a ratio outlier
        let target = mks("GEN", &verses);

        for &rule in RuleId::ALL {
            if rule.input_dependency() != InputDependency::TargetAndReferenceSilentWhenAbsent {
                continue;
            }
            let mut cfg = Config::all();
            cfg.rules.insert(rule, true);
            // No reference supplied.
            let findings = analyze_with_config(&target, None, &cfg);
            assert!(
                findings.iter().all(|f| f.code != rule),
                "{rule:?} must emit nothing with no reference"
            );
        }
    }

    #[test]
    fn findings_are_sorted_and_deterministic() {
        // A multi-verse, multi-book corpus that trips several default rules.
        let target = corpus_of(vec![
            keyed("GEN", &["a  b", "x\ty", "p  q  r"]),
            keyed("EXO", &["m  n", "\u{200b}lead"]),
        ]);

        let a = analyze(&target, None);
        let b = analyze(&target, None);
        assert_eq!(a, b, "analysis must be deterministic across runs");
        assert!(a.len() >= 5, "expected several findings, got {}", a.len());

        let keys: Vec<_> = a
            .iter()
            .map(|f| (f.key_idx, f.range.start, f.code))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(
            keys, sorted,
            "findings must be in (key_idx, start, code) order"
        );
    }

    /// Two verses sharing the exact same key string are still independently
    /// addressable: both get analyzed, and each occurrence keeps its own
    /// `KeyIdx` rather than collapsing onto one.
    #[test]
    fn duplicate_key_entries_are_both_analyzed_with_distinct_key_idx() {
        let target = Corpus::try_from_parts(
            vec!["GEN 1:1".to_string(), "GEN 1:1".to_string()],
            vec!["a  b".to_string(), "c  d".to_string()],
        )
        .unwrap();
        let findings = analyze(&target, None);
        let hits: Vec<_> = findings
            .iter()
            .filter(|f| target.key(f.key_idx) == "GEN 1:1")
            .collect();
        assert_eq!(hits.len(), 2, "both duplicate-key verses are analyzed");
        assert_ne!(
            hits[0].key_idx, hits[1].key_idx,
            "each occurrence keeps a distinct KeyIdx"
        );
    }

    /// A sub-verse key token (`1a`) survives unchanged through the whole
    /// pipeline, and a finding against it resolves back to that exact key.
    #[test]
    fn sub_verse_key_survives_and_a_finding_resolves_to_it() {
        let target =
            Corpus::try_from_parts(vec!["GEN 1:1a".to_string()], vec!["a  b".to_string()]).unwrap();
        let findings = analyze(&target, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(target.key(findings[0].key_idx), "GEN 1:1a");
    }

    /// `Books` (and therefore emission order) follows the corpus's
    /// *presented* order, not canonical/alphabetical book order: REV placed
    /// before GEN in the input must emit REV's finding first.
    #[test]
    fn presented_book_order_not_canonical_order_determines_emission_order() {
        let target = corpus_of(vec![keyed("REV", &["a  b"]), keyed("GEN", &["c\td"])]);
        let findings = analyze(&target, None);
        assert!(findings.len() >= 2);
        assert!(
            target.key(findings[0].key_idx).starts_with("REV"),
            "REV's finding emits first, matching presented order"
        );
        assert!(
            target
                .key(findings.last().unwrap().key_idx)
                .starts_with("GEN"),
            "GEN's finding emits last"
        );
    }

    #[test]
    fn cached_per_verse_lane_reuses_content_keyed_findings() {
        let target = corpus_of(vec![
            keyed("GEN", &["a  b", "hello"]),
            keyed("EXO", &["x\ty", "clean"]),
        ]);
        let cfg = Config::v1_defaults();
        let mut cache = AnalysisCache::new();

        let (cold_findings, cold_stats) =
            analyze_stateful(&target, None, &cfg, None, Some(&mut cache));
        let misses_after_cold = cache.direct_miss_count();
        let (warm_findings, warm_stats) =
            analyze_stateful(&target, None, &cfg, None, Some(&mut cache));

        assert_eq!(cold_findings, warm_findings);
        assert_eq!(cold_stats, warm_stats);
        assert_eq!(
            misses_after_cold, 2,
            "one direct-lane miss per chapter on cold call (one chapter per book here)"
        );
        assert_eq!(
            cache.direct_hit_count(),
            2,
            "warm call should hit both books"
        );
    }

    #[test]
    fn cached_fingerprint_change_rewarms_both_lanes() {
        let target = corpus_of(vec![
            keyed("GEN", &["a  b", "hello"]),
            keyed("EXO", &["x\ty", "clean"]),
        ]);
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();

        let (_, _) = analyze_stateful(&target, None, &cfg, None, Some(&mut cache));
        let mut changed_cfg = cfg.clone();
        changed_cfg.rules.insert(RuleId::BracketBalance, false);
        let (_, changed_prior) =
            analyze_stateful(&target, None, &changed_cfg, None, Some(&mut cache));

        // A config change clears the old products, so the first call under the
        // new fingerprint warms both books instead of reading either lane.
        assert_eq!(cache.direct_hit_count(), 0);
        assert_eq!(cache.walk_hit_count(), 0);

        // Content and enabled set are unchanged since the rewarm, so nothing is
        // stale: both books reuse both re-warmed lanes.
        let (_, _) = analyze_stateful(
            &target,
            None,
            &changed_cfg,
            Some(changed_prior),
            Some(&mut cache),
        );
        assert_eq!(cache.direct_hit_count(), 2);
        assert_eq!(
            cache.walk_hit_count(),
            2,
            "both books reuse their re-warmed walk (nothing stale)"
        );
    }

    #[test]
    fn cached_snapshot_matches_cold_snapshot_across_all_walk_lanes() {
        // GEN, EXO, LEV — contiguous per-book blocks (Corpus requires it).
        let original = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx"]),
            keyed("LEV", &["He said. The gate.", "clean text"]),
        ]);

        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (cold_cached_findings, cold_cached_stats) =
            analyze_stateful(&original, None, &cfg, None, Some(&mut cache));
        let (cold_findings, cold_stats) = analyze_stateful(&original, None, &cfg, None, None);
        assert_eq!(cold_cached_findings, cold_findings);
        assert_eq!(cold_cached_stats, cold_stats);

        // EXO's second verse edited; everything else unchanged.
        let edited = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx edited"]),
            keyed("LEV", &["He said. The gate.", "clean text"]),
        ]);
        let (scratch_findings, scratch_stats) =
            analyze_stateful(&edited, None, &cfg, Some(cold_stats.clone()), None);
        let (cached_findings, cached_stats) =
            analyze_stateful(&edited, None, &cfg, Some(cold_cached_stats), Some(&mut cache));

        assert_eq!(cached_findings, scratch_findings);
        assert_eq!(cached_stats, scratch_stats);
        assert_eq!(
            cache.walk_hit_count(),
            2,
            "clean books should reuse walk products"
        );
        assert_eq!(
            cache.walk_miss_count(),
            0,
            "changed book is walked without a cache probe"
        );
    }

    /// Retained per-book cache products are local (`LocalKeyIdx`), rebased to
    /// a global `KeyIdx` only against the *current* call's `BookGroup::base`
    /// — never stored as a stale global address. Growing an earlier book
    /// shifts every later book's base forward; a cache hit on the later book
    /// must still resolve to its new, shifted keys.
    #[test]
    fn cache_rebases_correctly_when_an_earlier_book_grows() {
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let original = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, Some(&mut cache));

        // GEN grows by one verse: EXO's global KeyIdx base shifts forward.
        let grown = corpus_of(vec![
            keyed("GEN", &["a  b", "one", "extra  space"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let (cached, cached_stats) =
            analyze_stateful(&grown, None, &cfg, Some(prior.clone()), Some(&mut cache));
        let (cold, cold_stats) = analyze_stateful(&grown, None, &cfg, Some(prior), None);

        assert_eq!(
            cached, cold,
            "cache-hit EXO findings must rebase to the shifted keys"
        );
        assert_eq!(cached_stats, cold_stats);
        assert_eq!(
            cache.walk_hit_count(),
            1,
            "EXO reuses its walk product across GEN's growth"
        );

        let exo_hit = cached
            .iter()
            .find(|f| grown.key(f.key_idx) == "EXO 1:1")
            .expect("EXO's tab-in-body finding resolves to its shifted key");
        assert_eq!(exo_hit.code, signals::hygiene::TAB_IN_BODY);
    }

    /// The mirror of the growth case: shrinking an earlier book shifts every
    /// later book's base *backward*, and a cache hit must still rebase
    /// correctly.
    #[test]
    fn cache_rebases_correctly_when_an_earlier_book_shrinks() {
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let original = corpus_of(vec![
            keyed("GEN", &["a  b", "one", "extra  space"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, Some(&mut cache));

        // GEN shrinks by one verse: EXO's global KeyIdx base shifts backward.
        let shrunk = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let (cached, cached_stats) =
            analyze_stateful(&shrunk, None, &cfg, Some(prior.clone()), Some(&mut cache));
        let (cold, cold_stats) = analyze_stateful(&shrunk, None, &cfg, Some(prior), None);

        assert_eq!(
            cached, cold,
            "cache-hit EXO findings must rebase to the shifted keys"
        );
        assert_eq!(cached_stats, cold_stats);
        assert_eq!(
            cache.walk_hit_count(),
            1,
            "EXO reuses its walk product across GEN's shrink"
        );

        let exo_hit = cached
            .iter()
            .find(|f| shrunk.key(f.key_idx) == "EXO 1:1")
            .expect("EXO's tab-in-body finding resolves to its shifted key");
        assert_eq!(exo_hit.code, signals::hygiene::TAB_IN_BODY);
    }

    #[test]
    fn cached_content_invalidation_replaces_one_book_and_keeps_sibling_warm() {
        let original = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
            keyed("LEV", &["clean text", "three"]),
        ]);
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, Some(&mut cache));
        let old_gen_hash = cache.entry_hash("GEN").unwrap();
        let old_exo_hash = cache.entry_hash("EXO").unwrap();
        let old_lev_hash = cache.entry_hash("LEV").unwrap();

        let edited = corpus_of(vec![
            keyed("GEN", &["changed ,, text", "one"]),
            keyed("EXO", &["x\ty", "two"]),
            keyed("LEV", &["clean text", "three"]),
        ]);
        // Only GEN's content changed. Its hash mismatch makes it stale, so it is
        // walked directly (never probed); EXO and LEV are clean and reuse their
        // walk products. GEN's cache entry is replaced by hash; siblings' hold.
        let (cold_findings, cold_stats) =
            analyze_stateful(&edited, None, &cfg, Some(prior.clone()), None);
        let (cached_findings, cached_stats) =
            analyze_stateful(&edited, None, &cfg, Some(prior), Some(&mut cache));

        assert_eq!(cached_findings, cold_findings);
        assert_eq!(cached_stats, cold_stats);
        assert_ne!(cache.entry_hash("GEN"), Some(old_gen_hash));
        assert_eq!(cache.entry_hash("EXO"), Some(old_exo_hash));
        assert_eq!(cache.entry_hash("LEV"), Some(old_lev_hash));
        assert_eq!(
            cache.walk_miss_count(),
            0,
            "the stale book is walked directly, never probed"
        );
        assert_eq!(
            cache.walk_hit_count(),
            2,
            "both clean siblings reuse lane 2"
        );
    }

    #[test]
    fn cached_empty_and_prior_none_calls_reuse_caseless_sentinel() {
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let empty = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        let (_, _) = analyze_stateful(&empty, None, &cfg, None, Some(&mut cache));
        assert_eq!(cache.book_count(), 0);

        let caseless = mk("GEN", &["你好"]);
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, Some(&mut cache));
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, Some(&mut cache));
        assert_eq!(
            cache.direct_hit_count(),
            1,
            "prior-none calls still reuse pure findings"
        );

        let full = corpus_of(vec![keyed("GEN", &["你好"]), keyed("EXO", &["a  b"])]);
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, Some(&mut cache));
        let edited = corpus_of(vec![keyed("GEN", &["你好"]), keyed("EXO", &["edited"])]);
        let walk_hits_before = cache.walk_hit_count();
        let (_, _) = analyze_stateful(&edited, None, &cfg, Some(prior), Some(&mut cache));
        assert_eq!(cache.walk_hit_count(), walk_hits_before + 1);
    }

    #[test]
    fn cached_snapshot_never_reads_default_stats_for_clean_books() {
        let original = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx"]),
        ]);
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, Some(&mut cache));
        assert_ne!(
            prior,
            Stats::default(),
            "the clean sibling must carry real prior stats"
        );

        let edited = corpus_of(vec![
            keyed("GEN", &["changed text", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx"]),
        ]);
        let (_, cold_stats) = analyze_stateful(&edited, None, &cfg, Some(prior.clone()), None);
        let (_, cached_stats) =
            analyze_stateful(&edited, None, &cfg, Some(prior), Some(&mut cache));
        assert_eq!(cached_stats, cold_stats);
    }

    #[test]
    fn analyze_flags_double_space() {
        let target = map(&[("v1", "a  b")]);
        let findings = analyze(&target, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, signals::whitespace::EXCESS_H_WHITESPACE);
        // The range slices the offending run out of that verse's text.
        let text = &target.texts()[0];
        assert_eq!(findings[0].range.slice(text), "  ");
    }

    #[test]
    fn analyze_clean_corpus_is_empty() {
        assert!(analyze(&map(&[("v1", "a b c")]), None).is_empty());
    }

    #[test]
    fn disabled_rule_does_not_fire() {
        let target = map(&[("v1", "a  b")]);
        let config = Config::disabling(&[signals::whitespace::EXCESS_H_WHITESPACE]);
        assert!(analyze_with_config(&target, None, &config).is_empty());
        // Other rules still run under the same config.
        let tabbed = map(&[("v1", "a\tb")]);
        let findings = analyze_with_config(&tabbed, None, &config);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, signals::hygiene::TAB_IN_BODY);
    }

    #[test]
    fn repeated_character_run_is_default_on_stateful_and_disableable() {
        // Spread the 50,000 "word" tokens across many short verses rather
        // than one giant verse: `SiteAddr` packs a verse-relative offset into
        // `u16`, and a single 250 KiB verse would overflow it. The
        // corpus-wide rarity math is book-scoped, not per-verse, so this is
        // the identical statistical shape.
        let mut verses: Vec<String> = (0..5_000).map(|_| "word ".repeat(10)).collect();
        verses.push("joyfullly".to_string());
        let target = mks("GEN", &verses);
        let findings = analyze(&target, None);
        let repeated: Vec<_> = findings
            .iter()
            .filter(|finding| finding.code == RuleId::RepeatedCharacterRun)
            .collect();
        assert_eq!(repeated.len(), 1);
        assert!(repeated[0].score.unwrap() > 0.85);

        let off = Config::disabling(&[RuleId::RepeatedCharacterRun]);
        assert!(
            analyze_with_config(&target, None, &off)
                .iter()
                .all(|finding| finding.code != RuleId::RepeatedCharacterRun)
        );
    }

    /// Proportionality runs through the entry like any other rule: fires
    /// against a reference, honours enable/disable and its typed knobs.
    #[test]
    fn proportionality_runs_through_analyze() {
        let mut keys = Vec::new();
        let mut source_texts = Vec::new();
        let mut target_texts = Vec::new();
        for v in 1..=60u16 {
            keys.push(format!("GEN 1:{v}"));
            let base = "word ".repeat(8 + (v as usize % 3));
            source_texts.push(base.clone());
            // Mild target-side jitter keeps the book's MAD nonzero.
            let jittered = format!("{base}{}", "x".repeat(v as usize % 5));
            target_texts.push(if v == 7 { base.repeat(4) } else { jittered });
        }
        let target = Corpus::try_from_parts(keys.clone(), target_texts).unwrap();
        let source = Corpus::try_from_parts(keys, source_texts).unwrap();

        let findings = analyze(&target, Some(&source));
        let prop: Vec<_> = findings
            .iter()
            .filter(|f| f.code == RuleId::ProjectLengthRatio)
            .collect();
        assert_eq!(prop.len(), 1);
        assert_eq!(target.key(prop[0].key_idx), "GEN 1:7");
        assert!(prop[0].score.is_some());
        assert!(matches!(
            prop[0].args,
            Some(FindingArgs::LengthRatio { .. })
        ));

        // Disabled ⇒ silent.
        let off = Config::disabling(&[RuleId::ProjectLengthRatio]);
        assert!(
            analyze_with_config(&target, Some(&source), &off)
                .iter()
                .all(|f| f.code != RuleId::ProjectLengthRatio)
        );

        // min_verses above the corpus size ⇒ book skipped.
        let strict = Config {
            proportionality: ProportionalityConfig {
                min_verses: 1000,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            analyze_with_config(&target, Some(&source), &strict)
                .iter()
                .all(|f| f.code != RuleId::ProjectLengthRatio)
        );
    }

    /// `punct.adjacency-anomaly` is default-on and, unlike before, actually
    /// wired into the stateful registry (it was implemented but unregistered,
    /// so it never ran through `analyze`). A rare mixed run against many period
    /// run-starts must surface as an Info finding through the default entry.
    #[test]
    fn adjacency_anomaly_runs_through_analyze() {
        let mut pairs: Vec<(&str, &str)> = (0..200).map(|_| ("v", "He said. She left.")).collect();
        pairs.push(("v", "word., word")); // one rare `.,`
        let target = map(&pairs);
        let f = analyze(&target, None);
        let adj: Vec<_> = f
            .iter()
            .filter(|f| f.code == RuleId::PunctuationAdjacencyAnomaly)
            .collect();
        assert_eq!(adj.len(), 1, "the rare `.,` surfaces through analyze");
        assert_eq!(adj[0].severity, Severity::Info);
        assert!(adj[0].score.unwrap() > 0.9);
    }

    /// The shipped defaults keep convention-dependent (P2) rules off;
    /// an explicit config entry opts in.
    #[test]
    fn p2_rules_are_default_disabled_and_opt_in() {
        // `punct.spacing-anomaly` is corpus-relative (ADR 0029): it needs a
        // dominant convention to judge against, so build one — commas attached
        // corpus-wide, one spaced minority to surface.
        let mut pairs: Vec<(&str, &str)> = (0..100).map(|_| ("v", "word, word")).collect();
        pairs.push(("v", "word , word"));
        let target = map(&pairs);
        assert!(
            analyze(&target, None)
                .iter()
                .all(|f| f.code != RuleId::PunctuationSpacingAnomaly),
            "default-disabled"
        );

        let mut on = Config::v1_defaults();
        on.rules.insert(RuleId::PunctuationSpacingAnomaly, true);
        let findings = analyze_with_config(&target, None, &on);
        let spacing: Vec<_> = findings
            .iter()
            .filter(|f| f.code == RuleId::PunctuationSpacingAnomaly)
            .collect();
        assert_eq!(spacing.len(), 1, "the lone spaced comma surfaces");
        assert_eq!(spacing[0].severity, Severity::Info);
        assert!(spacing[0].score.unwrap() > 0.85);

        // Casing is corpus-observed (ADR 0017, 0051): both rules default-off,
        // and once opted in the positional rule fires only where the corpus's
        // own lexicon-lowercase words establish a capital-after-`.` habit.
        let casing = mks("GEN", &casing_fire(40));
        assert!(analyze(&casing, None).is_empty());
        let on = casing_on(0.5, 0.0);
        assert!(
            analyze_with_config(&casing, None, &on)
                .iter()
                .any(|f| f.code == RuleId::SentenceInitialLowercase)
        );
    }

    /// Re-supplying the same complete corpus as `prior` supersedes each book
    /// and yields identical findings — the incremental-reuse path is
    /// observationally equal to a cold analyze.
    #[test]
    fn stateful_supersede_matches_cold() {
        let cfg = casing_on(0.5, 0.0);
        let target = mks("GEN", &casing_fire(40));

        let (f1, stats) = analyze_stateful(&target, None, &cfg, None, None);
        assert!(
            f1.iter()
                .any(|f| f.code == RuleId::SentenceInitialLowercase)
        );

        let (f2, _) = analyze_stateful(&target, None, &cfg, Some(stats), None);
        assert_eq!(f1, f2);
    }

    /// Complete-snapshot semantics — no echo: a target that omits a prior book
    /// answers for EXACTLY its books. The absent book is dropped from the
    /// returned stats, and the result equals a cold analyze of just the supplied
    /// books — never the union with the carried prior.
    #[test]
    fn complete_snapshot_drops_prior_books_absent_from_target() {
        let cfg = Config::all();
        let full = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, None);

        // Supply only GEN, with the full prior: EXO must be dropped, not echoed.
        let gen_only = mk("GEN", &["a  b", "one"]);
        let (f_inc, s_inc) = analyze_stateful(&gen_only, None, &cfg, Some(prior), None);
        assert!(!s_inc.tallied.contains_key("EXO"), "absent EXO is dropped (no echo)");
        assert!(s_inc.tallied.contains_key("GEN"), "the supplied book is answered for");

        let (f_cold, s_cold) = analyze_stateful(&gen_only, None, &cfg, None, None);
        assert_eq!(f_inc, f_cold, "answers for exactly the supplied books");
        assert_eq!(s_inc, s_cold, "and its stats equal a cold GEN-only analyze");
    }

    /// A whole-corpus incremental call — prior supplied, only the edited book
    /// stale by content hash — must produce findings AND stats identical to a
    /// from-scratch recompute of the edited corpus, including findings that
    /// *moved in the untouched book* because the edit tipped a pooled
    /// convention (the copy-paste-a-new-Genesis case).
    #[test]
    fn incremental_snapshot_matches_full_recompute() {
        let cfg = casing_on(0.5, 0.0);
        // Clean establishing verses (a capital-after-`.` habit, no anomaly).
        // ≥ 30 so the `.` boundary class clears ADR 0052's event floor.
        let clean: Vec<String> = (0..40)
            .map(|_| "The men saw the gate.".to_string())
            .collect();
        let original = corpus_of(vec![keyed("GEN", &clean), keyed("EXO", &clean)]);
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None);

        // Edit GEN only: introduce a lowercase-after-terminal anomaly.
        let edited = corpus_of(vec![keyed("GEN", &casing_fire(40)), keyed("EXO", &clean)]);

        let (f_scratch, s_scratch) = analyze_stateful(&edited, None, &cfg, None, None);
        // GEN's hash changed, so it re-tallies; EXO carries — yet the tipped
        // convention re-emits in EXO too, matching the from-scratch snapshot.
        let (f_inc, s_inc) = analyze_stateful(&edited, None, &cfg, Some(prior), None);
        assert_eq!(f_scratch, f_inc);
        assert_eq!(s_scratch, s_inc);
    }

    /// `Stats::remove_book` drops a book's contribution to the corpus
    /// aggregate, not just its findings: here EXO's anomaly clears the
    /// dominance floor only while GEN's observations back it, so removing
    /// GEN silences it.
    #[test]
    fn remove_book_drops_contribution_to_corpus_stats() {
        let cfg = casing_on(0.7, 1.0);
        // GEN establishes the '.'→capital habit on the lexicon-lowercase "the".
        // ≥ 30 so the `.` boundary class clears ADR 0052's event floor.
        let gen_clean: Vec<String> = (0..40)
            .map(|_| "The men saw the gate.".to_string())
            .collect();
        // EXO alone holds one lowercase "the" after a period — but with no
        // mid-flow "the" of its own it is unclassifiable without GEN's lexicon.
        let exo_anom = ["He fell. the gate stood.".to_string()];
        let full = corpus_of(vec![keyed("GEN", &gen_clean), keyed("EXO", &exo_anom)]);

        let (f_full, mut stats) = analyze_stateful(&full, None, &cfg, None, None);
        assert!(
            f_full.iter().any(|f| {
                crate::key::parse_key(full.key(f.key_idx)).unwrap().book == "EXO"
                    && f.code == RuleId::SentenceInitialLowercase
            }),
            "EXO's `the` fires while GEN backs the lexicon + habit"
        );

        stats.remove_book("GEN");
        let (f_after, _) =
            analyze_stateful(&mks("EXO", &exo_anom), None, &cfg, Some(stats), None);
        // EXO's own few observations can't back a confident dominance now.
        assert!(
            f_after
                .iter()
                .all(|f| f.code != RuleId::SentenceInitialLowercase)
        );
    }

    /// `uni.redundant-zero-width-space` runs through `analyze` as a default-on
    /// per-verse rule: a doubled U+200B run surfaces at Info, while a single
    /// U+200B — even space-adjacent — and a legitimate in-token word break stay
    /// silent (only exact duplicates are provably redundant).
    #[test]
    fn redundant_zero_width_space_runs_through_analyze() {
        const ZW: &str = "\u{200B}";
        let full = Corpus::try_from_parts(
            vec![
                "GEN 1:1".to_string(),
                "GEN 1:2".to_string(),
                "GEN 1:3".to_string(),
            ],
            vec![
                format!("word{ZW}{ZW}next"), // doubled run → redundant
                format!("word {ZW}next"),    // single, space-adjacent → NOT flagged
                format!("ក{ZW}ក"),           // Khmer word break → silent
            ],
        )
        .unwrap();

        let f = analyze(&full, None);
        let hits: Vec<_> = f
            .iter()
            .filter(|f| f.code == RuleId::RedundantZeroWidthSpace)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "only the doubled run surfaces; single ZWSP (even space-adjacent) does not"
        );
        assert_eq!(full.key(hits[0].key_idx), "GEN 1:1");
        assert_eq!(hits[0].severity, Severity::Info);
    }

    /// Registry completeness: every declared `RuleId` must be produced by
    /// exactly one runner registry OR consumed by exactly one typed observation
    /// substrate. A rule that is implemented but never wired in — the ADR-0031
    /// P0, where `punct.adjacency-anomaly` ran in calibration but was absent from
    /// `stateful_rules`, so it never fired through `analyze` — surfaces here as a
    /// count of zero. `punct.spacing-anomaly` and `struct.duplicate-word` are
    /// claimed by their substrates (each its sole consumer), not a rule registry.
    #[test]
    fn every_rule_id_is_claimed_by_exactly_one_registry_or_substrate() {
        use std::collections::BTreeMap;
        let cfg = Config::v1_defaults();
        // Registries are membership-complete (they include rules `v1_defaults`
        // disables); config only feeds knobs, so any config yields the full set.
        let pv = rule::per_verse_rules();
        let pr = rule::project_rules(&cfg);
        let sf = rule::stateful_rules(&cfg);
        let mut seen: BTreeMap<RuleId, u32> = BTreeMap::new();
        for id in pv
            .iter()
            .map(|r| r.id())
            .chain(pr.iter().map(|r| r.id()))
            .chain(sf.iter().map(|r| r.id()))
        {
            *seen.entry(id).or_default() += 1;
        }
        // Substrate consumers claim their rule ids too — exactly once, across the
        // closed substrate registry.
        for &sid in substrate::SubstrateId::ALL {
            for &id in substrate::consumers_of(sid) {
                *seen.entry(id).or_default() += 1;
            }
        }
        for &id in RuleId::ALL {
            assert_eq!(
                seen.get(&id).copied().unwrap_or(0),
                1,
                "{} must be wired into exactly one runner registry or substrate",
                id.code()
            );
        }
        assert_eq!(
            seen.len(),
            RuleId::ALL.len(),
            "a registry or substrate emitted an unknown id"
        );
    }

    /// Guards the `RuleId` wire format: the serde rename must match
    /// `code()` and must not drift from the v0.0.1 strings the consumer
    /// keys config/localisation off.
    #[test]
    fn rule_id_wire_strings_are_stable() {
        for &id in RuleId::ALL {
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.code()));
        }
        assert_eq!(
            serde_json::to_string(&RuleId::ExcessHWhitespace).unwrap(),
            "\"lex.excess-h-whitespace\""
        );
    }

    /// `uni.mixed-normalization` is default-**off** (ADR 0063's perf
    /// adjudication: the detector's own no-unsafe-skip cost measured a real
    /// warm-path regression even after the `NORM_RELEVANT` prefilter, so it
    /// ships cold/explicit-only rather than as a further hot-path redesign)
    /// and, like every rule, an explicit `true` in the config's `rules` map
    /// turns it on.
    #[test]
    fn mixed_normalization_is_default_off_and_explicitly_enableable() {
        assert!(!Config::v1_defaults().is_enabled(RuleId::MixedNormalization));
        let mut on = Config::v1_defaults();
        on.rules.insert(RuleId::MixedNormalization, true);
        assert!(on.is_enabled(RuleId::MixedNormalization));
    }

    // ── Per-book `Tally` provenance (hash-derived counting) ──────────────────

    /// Cold → edit the middle book → incremental call (uncached AND cached)
    /// both reproduce a from-scratch analyze of the edited corpus, findings and
    /// stats. The single strongest per-book-provenance equivalence check.
    #[test]
    fn tally_incremental_equivalence_uncached_and_cached() {
        let original = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly", "A1 α qQx"]),
            keyed("LEV", &["He said. The gate.", "clean text"]),
        ]);
        let cfg = Config::all();
        let (_, prior_uncached) = analyze_stateful(&original, None, &cfg, None, None);
        let mut cache = AnalysisCache::new();
        let (_, prior_cached) = analyze_stateful(&original, None, &cfg, None, Some(&mut cache));

        let edited = corpus_of(vec![
            keyed("GEN", &["(He said. the gate stood.", "one) word word 12"]),
            keyed("EXO", &["a  b, joyfullly edited", "A1 α qQx"]),
            keyed("LEV", &["He said. The gate.", "clean text"]),
        ]);
        let (f_cold, s_cold) = analyze_stateful(&edited, None, &cfg, None, None);
        let (f_unc, s_unc) = analyze_stateful(&edited, None, &cfg, Some(prior_uncached), None);
        let (f_cac, s_cac) =
            analyze_stateful(&edited, None, &cfg, Some(prior_cached), Some(&mut cache));
        assert_eq!((f_unc, s_unc), (f_cold.clone(), s_cold.clone()), "uncached ≡ cold");
        assert_eq!((f_cac, s_cac), (f_cold, s_cold), "cached ≡ cold");
    }

    /// Editing one book re-tallies exactly that book; the others carry
    /// their prior `Tally` untouched.
    #[test]
    fn tally_derived_stale_set_is_exact() {
        let cfg = Config::all();
        let original = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
            keyed("LEV", &["clean", "three"]),
        ]);
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None);
        let edited = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two edited"]),
            keyed("LEV", &["clean", "three"]),
        ]);
        let (_, stats) = analyze_stateful(&edited, None, &cfg, Some(prior.clone()), None);
        assert_ne!(
            stats.tallied["EXO"].text, prior.tallied["EXO"].text,
            "the edited book re-tallies to a new text hash"
        );
        assert_eq!(stats.tallied["GEN"], prior.tallied["GEN"], "clean book carries");
        assert_eq!(stats.tallied["LEV"], prior.tallied["LEV"], "clean book carries");
    }

    /// `Tally.source` granularity — editing one source book stales only its
    /// same-slug target book; a same-content source re-supply stales nothing;
    /// dropping the source flips every book to `SOURCE_NONE`.
    #[test]
    fn tally_source_granularity() {
        let cfg = Config::all();
        let target = corpus_of(vec![
            keyed("GEN", &["aa bb", "cc dd"]),
            keyed("EXO", &["ee ff", "gg hh"]),
        ]);
        let source_v1 = corpus_of(vec![
            keyed("GEN", &["s1 s2", "s3 s4"]),
            keyed("EXO", &["t1 t2", "t3 t4"]),
        ]);
        let (_, prior) = analyze_stateful(&target, Some(&source_v1), &cfg, None, None);
        assert_ne!(prior.tallied["GEN"].source, SOURCE_NONE, "source present ⇒ real hash");

        let source_v2 = corpus_of(vec![
            keyed("GEN", &["s1 s2 CHANGED", "s3 s4"]),
            keyed("EXO", &["t1 t2", "t3 t4"]),
        ]);
        let (_, stats) = analyze_stateful(&target, Some(&source_v2), &cfg, Some(prior.clone()), None);
        assert_ne!(
            stats.tallied["GEN"].source, prior.tallied["GEN"].source,
            "editing source GEN stales target GEN"
        );
        assert_eq!(stats.tallied["EXO"], prior.tallied["EXO"], "EXO's source unchanged ⇒ carries");

        let (_, stats2) = analyze_stateful(&target, Some(&source_v2), &cfg, Some(stats.clone()), None);
        assert_eq!(stats2.tallied, stats.tallied, "same-content source re-supply stales nothing");

        let (_, stats3) = analyze_stateful(&target, None, &cfg, Some(stats), None);
        assert_eq!(stats3.tallied["GEN"].source, SOURCE_NONE, "dropping source ⇒ SOURCE_NONE");
        assert_eq!(stats3.tallied["EXO"].source, SOURCE_NONE, "dropping source ⇒ SOURCE_NONE");

        // None → Some: re-adding a source re-tallies affected books off SOURCE_NONE.
        let (_, stats4) =
            analyze_stateful(&target, Some(&source_v2), &cfg, Some(stats3.clone()), None);
        assert_ne!(stats4.tallied["GEN"].source, SOURCE_NONE, "re-adding source re-tallies GEN");
        assert_ne!(stats4.tallied["EXO"].source, SOURCE_NONE, "re-adding source re-tallies EXO");

        // A present source lacking the target's slug ⇒ that book's source is
        // SOURCE_NONE even though a source corpus is supplied.
        let gen_only_source = mk("GEN", &["s1 s2 CHANGED", "s3 s4"]);
        let (_, stats5) =
            analyze_stateful(&target, Some(&gen_only_source), &cfg, Some(stats4.clone()), None);
        assert_ne!(stats5.tallied["GEN"].source, SOURCE_NONE, "GEN has a same-slug source book");
        assert_eq!(
            stats5.tallied["EXO"].source, SOURCE_NONE,
            "EXO absent from the source ⇒ SOURCE_NONE despite a present source"
        );
    }

    /// A prior from one text lineage with a corpus from another re-tallies
    /// every mismatched book; output equals a cold analyze of the new lineage.
    #[test]
    fn tally_lineage_mismatch_is_self_healing() {
        let cfg = Config::all();
        let x = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let y = corpus_of(vec![keyed("GEN", &["p  q", "three"]), keyed("EXO", &["r\ts", "four"])]);
        let (_, prior_x) = analyze_stateful(&x, None, &cfg, None, None);
        let (f_inc, s_inc) = analyze_stateful(&y, None, &cfg, Some(prior_x), None);
        let (f_cold, s_cold) = analyze_stateful(&y, None, &cfg, None, None);
        assert_eq!(f_inc, f_cold);
        assert_eq!(s_inc, s_cold);
    }

    /// A supplied book absent from the prior's `tallied` is stale by
    /// definition (a missing entry is a mismatch) and is tallied fresh.
    #[test]
    fn tally_new_book_absent_from_prior_is_tallied_fresh() {
        let cfg = Config::all();
        let one = mk("GEN", &["a  b", "one"]);
        let (_, prior) = analyze_stateful(&one, None, &cfg, None, None);
        assert!(!prior.tallied.contains_key("EXO"));
        let two = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let (f_inc, s_inc) = analyze_stateful(&two, None, &cfg, Some(prior), None);
        assert!(s_inc.tallied.contains_key("EXO"), "new book gets a fresh Tally");
        let (f_cold, s_cold) = analyze_stateful(&two, None, &cfg, None, None);
        assert_eq!(f_inc, f_cold);
        assert_eq!(s_inc, s_cold);
    }

    /// The enabled-set regression — a prior built with rule R disabled,
    /// text unchanged, re-analyzed with R enabled must equal cold-with-R (every
    /// `Tally.rules` mismatches, so R's counts appear).
    #[test]
    fn tally_enabled_set_regression() {
        let target = mks("GEN", &casing_fire(40));
        let cfg_off = Config::v1_defaults(); // SentenceInitialLowercase off
        let cfg_on = casing_on(0.5, 0.0); // SentenceInitialLowercase on
        let (_, prior_off) = analyze_stateful(&target, None, &cfg_off, None, None);
        let (f_on, s_on) = analyze_stateful(&target, None, &cfg_on, Some(prior_off), None);
        let (f_cold, s_cold) = analyze_stateful(&target, None, &cfg_on, None, None);
        assert_eq!(f_on, f_cold);
        assert_eq!(s_on, s_cold);
        assert!(
            f_on.iter().any(|f| f.code == RuleId::SentenceInitialLowercase),
            "the re-enabled rule's finding appears"
        );
    }

    /// A knob-only config change re-tallies nothing while findings track the
    /// new knobs — judging moves, counting doesn't. `Config::all` so a site-free
    /// counting rule backs the probe, which observes actual counting-accumulator
    /// runs (not the decision flag): a listener that counted a clean book would
    /// make it read nonzero.
    #[test]
    fn tally_knob_only_change_retallies_nothing() {
        let target = mk("GEN", &["a  b", "A1 α qQx joyfullly"]);
        let cfg1 = Config::all();
        let mut cfg2 = Config::all();
        cfg2.casing.emit_score_min = 0.9; // knob-only: same enabled set, stricter knob
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&target, None, &cfg1, None, Some(&mut cache));
        assert_eq!(cache.retallied_count(), 1, "the cold call counts the one book");
        let (f_inc, s_inc) =
            analyze_stateful(&target, None, &cfg2, Some(prior.clone()), Some(&mut cache));
        assert_eq!(cache.retallied_count(), 0, "knob-only change did no counting work");
        assert_eq!(s_inc.tallied, prior.tallied, "provenance unchanged");
        let (f_cold, _) = analyze_stateful(&target, None, &cfg2, None, None);
        assert_eq!(f_inc, f_cold, "findings track the new knobs (judging moved, counting didn't)");
    }

    /// The disable→re-enable retention invariant on complete snapshots: a
    /// complete corpus analyzed with rule R disabled, then re-analyzed with R
    /// enabled (text unchanged), reproduces cold-with-R — the carried per-book
    /// aggregates rejudge correctly once R is back on.
    #[test]
    fn tally_disable_reenable_matches_cold() {
        let cfg_on = casing_on(0.5, 0.0);
        let mut cfg_off = cfg_on.clone();
        cfg_off.rules.insert(RuleId::SentenceInitialLowercase, false);
        let a = casing_fire(40);
        let b = casing_fire(40);
        let full = corpus_of(vec![keyed("GEN", &a), keyed("EXO", &b)]);

        // Analyze the complete corpus with R off, then re-supply the complete
        // corpus with R on — no echo subset; the whole corpus is answered each
        // call.
        let (_, prior_off) = analyze_stateful(&full, None, &cfg_off, None, None);
        let (f_re, s_re) = analyze_stateful(&full, None, &cfg_on, Some(prior_off), None);
        let (f_cold, s_cold) = analyze_stateful(&full, None, &cfg_on, None, None);
        assert_eq!(f_re, f_cold, "re-enabling R rebuilds its contribution for every book");
        assert_eq!(s_re, s_cold);
    }

    /// `Stats::remove_book` drops the slug from `tallied` as
    /// well as from every rule variant — a removed book leaves no provenance
    /// and no corpus-stats contribution behind.
    #[test]
    fn remove_book_also_drops_the_tallied_entry() {
        let cfg = Config::all();
        let corpus =
            corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let (_, mut stats) = analyze_stateful(&corpus, None, &cfg, None, None);
        assert!(stats.tallied.contains_key("GEN") && stats.tallied.contains_key("EXO"));

        stats.remove_book("GEN");
        assert!(!stats.tallied.contains_key("GEN"), "provenance entry removed");
        assert!(stats.tallied.contains_key("EXO"), "sibling provenance retained");

        // The rule-side removal (every variant) equals a corpus that never had
        // GEN: an EXO-only analyze with the pruned prior matches cold EXO.
        let exo = mk("EXO", &["x\ty", "two"]);
        let (_, s_after) = analyze_stateful(&exo, None, &cfg, Some(stats), None);
        let (_, s_cold) = analyze_stateful(&exo, None, &cfg, None, None);
        assert_eq!(s_after, s_cold, "no GEN contribution survives in any rule");
    }

    // ── Resident finding partitions: the atomic finding boundary (§3.3) ──────

    /// The returned findings come ONLY from the resident partition lane:
    /// assembling the committed partitions (chapter-local, rebased to global)
    /// reproduces the returned findings exactly, over a multi-book,
    /// multi-chapter corpus with a duplicate key. The oracle gate proves the
    /// same over the fleet (including the adjacency/spacing/duplicate-word tie
    /// cases); this is the focused in-crate witness that assembly reads the lane.
    #[test]
    fn returned_findings_come_only_from_the_partition_lane() {
        let cfg = Config::all();
        let target = Corpus::try_from_parts(
            vec![
                "GEN 1:1".into(),
                "GEN 1:2".into(),
                "GEN 2:1".into(),
                "EXO 1:1".into(),
                "EXO 1:1".into(), // a duplicate key, both independently addressed
            ],
            vec![
                "a  b, joyfullly".into(),
                "word word here".into(),
                "one) two".into(),
                "x\ty".into(),
                "A1 α qQx".into(),
            ],
        )
        .unwrap();
        let mut cache = AnalysisCache::new();
        let (findings, _) = analyze_resident(&target, None, &cfg, None, &mut cache).unwrap();
        assert!(!findings.is_empty(), "the corpus fires several rules");
        assert_eq!(
            cache.partition_findings(&target),
            findings,
            "assembly reads only the resident partition lane, chapter-local round-trip exact"
        );
    }

    /// A fault at any of map/reduce/judge leaves the PREVIOUS partitions intact
    /// and current — they still describe the last successful analyze, not a
    /// half-built candidate — and a retry with no further mutation rebuilds them
    /// to the cold result. Proves no partial finding layer is ever exposed as
    /// current (plan §3.3 / §16).
    #[test]
    fn fault_leaves_previous_partitions_intact_and_current() {
        use fault::Phase;
        let cfg = Config::all();
        let a = corpus_of(vec![
            keyed("GEN", &["a  b, joyfullly", "word word here"]),
            keyed("EXO", &["x\ty", "one) two"]),
        ]);
        // Edit GEN's first verse so B's findings differ from A's.
        let b = corpus_of(vec![
            keyed("GEN", &["clean and tidy text", "word word here"]),
            keyed("EXO", &["x\ty", "one) two"]),
        ]);
        let cold_b = analyze_with_config(&b, None, &cfg);

        for phase in [Phase::Map, Phase::Reduce, Phase::Judge] {
            let mut cache = AnalysisCache::new();
            let (findings_a, prior) = analyze_resident(&a, None, &cfg, None, &mut cache).unwrap();
            assert_eq!(cache.partition_findings(&a), findings_a);

            // A faulted analyze of B publishes no partitions.
            let outcome = {
                let _guard = fault::arm(phase);
                analyze_resident(&b, None, &cfg, Some(prior), &mut cache)
            };
            let prior = match outcome {
                Err((_, p)) => p,
                Ok(_) => panic!("{phase:?} fault did not fire"),
            };
            assert_eq!(
                cache.partition_findings(&a),
                findings_a,
                "a {phase:?} fault leaves the previous partitions intact and current (still A)"
            );

            // Retry, no further mutation: reaches the cold result, assembled from
            // the now-rebuilt partitions.
            let (findings_b, _) = analyze_resident(&b, None, &cfg, prior, &mut cache).unwrap();
            assert_eq!(findings_b, cold_b, "retry after a {phase:?} fault equals cold");
            assert_eq!(
                cache.partition_findings(&b),
                findings_b,
                "the partitions now describe B"
            );
        }
    }

    /// Shrink witness: a retained partition record whose chapter has since
    /// shrunk must fail loud at assembly, never rebase silently into the next
    /// chapter. Chapter *existence* is not containment proof — after the
    /// shrink, `base + local` for the stale record is globally in-bounds but
    /// addresses the following chapter's verses. The full-rebuild batch path
    /// re-partitions on every analyze and can never hit this; the check is
    /// the tripwire armed for retained/partially-patched partitions.
    #[test]
    #[should_panic(expected = "stale partition record")]
    fn shrunk_chapter_trips_the_rebase_containment_check() {
        let cfg = Config::all();
        let mut corpus = Corpus::try_from_parts(
            vec![
                "GEN 1:1".to_string(),
                "GEN 1:2".to_string(),
                "GEN 1:3".to_string(),
            ],
            vec![
                "clean text".to_string(),
                "clean text".to_string(),
                "a\tb".to_string(),
            ],
        )
        .unwrap();
        let mut cache = AnalysisCache::new();
        let (findings, _) = analyze_resident(&corpus, None, &cfg, None, &mut cache).unwrap();
        assert!(
            findings.iter().any(|f| corpus.key(f.key_idx) == "GEN 1:3"),
            "witness finding sits at local index 2 of the chapter"
        );

        // Shrink the chapter under the retained partitions, without the
        // analyze that would rebuild them.
        corpus
            .replace_chapter(crate::corpus::ChapterBlock {
                slug: "GEN".into(),
                chapter: "1".into(),
                keys: vec!["GEN 1:1".to_string()],
                texts: vec!["clean text".to_string()],
            })
            .unwrap();
        let _ = cache.partition_findings(&corpus);
    }

    /// A multi-chapter book with `verses` verses per chapter, verse text supplied
    /// per (chapter, verse) so a test can edit one chapter in isolation.
    fn chaptered(book: &str, chapters: &[(&str, Vec<&str>)]) -> (Vec<String>, Vec<String>) {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for (chapter, verses) in chapters {
            for (i, text) in verses.iter().enumerate() {
                keys.push(format!("{book} {chapter}:{}", i + 1));
                texts.push((*text).to_string());
            }
        }
        (keys, texts)
    }

    /// The direct (per-verse) lane's granularity, witnessed by the work probes: a
    /// one-chapter edit re-derives that chapter's per-verse findings and nothing
    /// else, and replaces exactly that chapter's direct-rule partition records.
    ///
    /// The fused walk is a separate lane at a separate granularity and honestly
    /// re-walks the whole edited book — its listeners carry discourse state across
    /// every verse seam in the book, so a chapter is not a reusable unit for them
    /// until they become observation substrates.
    #[test]
    fn one_chapter_edit_maps_and_patches_exactly_that_chapter() {
        let cfg = Config::all();
        let target = corpus_of(vec![
            chaptered(
                "GEN",
                &[
                    ("1", vec!["a  b, joyfullly", "word word here"]),
                    ("2", vec!["x\ty", "one) two"]),
                    ("3", vec!["He said. the gate", "clean text"]),
                ],
            ),
            chaptered("EXO", &[("1", vec!["A1 α qQx"]), ("2", vec!["b  c"])]),
        ]);
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_resident(&target, None, &cfg, None, &mut cache).unwrap();
        let cold = cache.probe();
        assert_eq!(cold.direct_misses, 5, "a cold call maps every chapter");
        assert_eq!(cold.direct_hits, 0);
        assert_eq!(cold.direct_chapters_patched, 5);

        // Edit GEN 2 only.
        let mut edited = target.clone();
        edited
            .replace_chapter(crate::corpus::ChapterBlock {
                slug: "GEN".into(),
                chapter: "2".into(),
                keys: vec!["GEN 2:1".to_string(), "GEN 2:2".to_string()],
                texts: vec!["p  q, sadlyy".to_string(), "one) two".to_string()],
            })
            .unwrap();

        let before = cache.probe();
        let (findings, _) = analyze_resident(&edited, None, &cfg, Some(prior), &mut cache).unwrap();
        let after = cache.probe();

        assert_eq!(
            after.direct_misses - before.direct_misses,
            1,
            "exactly one chapter re-derives its per-verse findings"
        );
        assert_eq!(
            after.direct_hits - before.direct_hits,
            4,
            "every other chapter's records are reused, not recomputed"
        );
        assert_eq!(
            after.direct_chapters_patched, 1,
            "exactly one chapter's direct-rule partition records are replaced"
        );
        assert_eq!(
            after.retallied, 1,
            "the fused walk is book-grained: the edited book — and only it — re-walks"
        );
        assert_eq!(
            after.walk_hits - before.walk_hits,
            1,
            "the untouched book reuses its walk products"
        );
        assert_eq!(
            findings,
            analyze_with_config(&edited, None, &cfg),
            "the patched result equals a cold complete analysis"
        );
    }

    /// A one-book corpus of `chapters` chapters × `verses` verses, each verse
    /// padded to `pad` bytes so a test can put the map seam above or below its
    /// work threshold deliberately.
    fn wide_book(slug: &str, chapters: usize, verses: usize, pad: usize) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for c in 1..=chapters {
            for v in 1..=verses {
                keys.push(format!("{slug} {c}:{v}"));
                // Deliberately dirty text (double space, stray bracket) so the
                // per-verse rules actually fire, then padding for bulk.
                texts.push(format!("a  b) {}", "word ".repeat(pad / 5)));
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
    }

    /// One dirty book with many dirty chapters takes the chapter grain; several
    /// dirty books take the book grain; one dirty chapter stays serial. The
    /// engine-level witness that the routing table is actually reached (the
    /// routing decision itself is unit-tested in `rule`).
    #[test]
    fn the_direct_lane_routes_by_dirty_map_scope() {
        let cfg = Config::v1_defaults();
        fn expect(route: &'static str) -> &'static str {
            if cfg!(feature = "parallel") { route } else { "serial" }
        }

        // Cold, one book, 40 chapters, well over the byte threshold.
        let one_book = wide_book("PSA", 40, 6, 400);
        let mut cache = AnalysisCache::new();
        analyze_resident(&one_book, None, &cfg, None, &mut cache).unwrap();
        assert_eq!(cache.probe().direct_map_route, expect("chapters"));

        // Cold, several books.
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for slug in ["GEN", "EXO", "LEV"] {
            let b = wide_book(slug, 10, 6, 400);
            keys.extend(b.keys().to_vec());
            texts.extend(b.texts().to_vec());
        }
        let many_books = Corpus::try_from_parts(keys, texts).unwrap();
        let mut cache = AnalysisCache::new();
        analyze_resident(&many_books, None, &cfg, None, &mut cache).unwrap();
        assert_eq!(cache.probe().direct_map_route, expect("books"));

        // Warm, one chapter edited: one map task, so nothing to fan out.
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_resident(&one_book, None, &cfg, None, &mut cache).unwrap();
        let mut edited = one_book.clone();
        edited
            .replace_chapter(crate::corpus::ChapterBlock {
                slug: "PSA".into(),
                chapter: "7".into(),
                keys: (1..=6).map(|v| format!("PSA 7:{v}")).collect(),
                texts: (1..=6).map(|_| "edited  text)".to_string()).collect(),
            })
            .unwrap();
        analyze_resident(&edited, None, &cfg, Some(prior), &mut cache).unwrap();
        assert_eq!(cache.probe().direct_map_route, "serial");
    }

    /// The spacing substrate's chapter map goes through the same ordered seam,
    /// with the same routing table: several dirty books take the book grain, one
    /// dirty book with many dirty chapters takes the chapter grain, and one dirty
    /// chapter stays serial. Reduction is a carry fold and stays sequential per
    /// book regardless of the route.
    #[test]
    fn the_spacing_substrate_routes_by_dirty_map_scope() {
        let mut cfg = Config::v1_defaults();
        cfg.rules
            .insert(crate::diagnostics::RuleId::PunctuationSpacingAnomaly, true);
        fn expect(route: &'static str) -> &'static str {
            if cfg!(feature = "parallel") { route } else { "serial" }
        }

        // Cold, one book, 40 chapters, well over the byte threshold.
        let one_book = wide_book("PSA", 40, 6, 400);
        let mut cache = AnalysisCache::new();
        analyze_resident(&one_book, None, &cfg, None, &mut cache).unwrap();
        assert_eq!(cache.probe().spacing_map_route, expect("chapters"));
        assert_eq!(cache.probe().spacing_mapped, 40, "cold maps every chapter");

        // Cold, several books: the substrate plans across books, so it sees a
        // multi-book scope (which the per-book Phase C loop could not).
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for slug in ["GEN", "EXO", "LEV"] {
            let b = wide_book(slug, 10, 6, 400);
            keys.extend(b.keys().to_vec());
            texts.extend(b.texts().to_vec());
        }
        let many_books = Corpus::try_from_parts(keys, texts).unwrap();
        let mut cache = AnalysisCache::new();
        analyze_resident(&many_books, None, &cfg, None, &mut cache).unwrap();
        assert_eq!(cache.probe().spacing_map_route, expect("books"));

        // Warm, one chapter edited: one map task, so nothing to fan out.
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_resident(&one_book, None, &cfg, None, &mut cache).unwrap();
        let mut edited = one_book.clone();
        edited
            .replace_chapter(crate::corpus::ChapterBlock {
                slug: "PSA".into(),
                chapter: "7".into(),
                keys: (1..=6).map(|v| format!("PSA 7:{v}")).collect(),
                texts: (1..=6).map(|_| "edited  text)".to_string()).collect(),
            })
            .unwrap();
        analyze_resident(&edited, None, &cfg, Some(prior), &mut cache).unwrap();
        let p = cache.probe();
        assert_eq!(p.spacing_map_route, "serial");
        assert_eq!(p.spacing_mapped, 1, "a one-chapter edit maps one chapter");
        assert!(
            p.spacing_reduced < 40,
            "the replay converges instead of re-reducing the whole book, got {}",
            p.spacing_reduced
        );
    }

    /// Mapper output is identical regardless of thread count. The chapter route
    /// writes each result back into the caller-order slot it came from, so
    /// completion order cannot reach the answer.
    #[cfg(feature = "parallel")]
    #[test]
    fn mapper_output_is_identical_regardless_of_thread_count() {
        let cfg = Config::all();
        let one_book = wide_book("PSA", 40, 6, 400);
        let reference = analyze_with_config(&one_book, None, &cfg);
        assert!(!reference.is_empty(), "the fixture fires rules");
        for threads in [1usize, 2, 3, 7, 16] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let got = pool.install(|| analyze_with_config(&one_book, None, &cfg));
            assert_eq!(got, reference, "{threads} threads changed the answer");
        }
    }

    /// The two lanes' stamps are independent, and that is what makes a retry
    /// safe: a faulted attempt maps chapters and warms prep without ever
    /// committing partitions, so the retry maps *nothing* and yet must still
    /// replace every chapter's records. Deriving the patch set from prep's warm
    /// state would publish the previous input's findings.
    #[test]
    fn retry_after_a_faulted_attempt_patches_without_remapping() {
        let cfg = Config::all();
        let a = corpus_of(vec![chaptered(
            "GEN",
            &[("1", vec!["a  b, joyfullly"]), ("2", vec!["x\ty"])],
        )]);
        let b = corpus_of(vec![chaptered(
            "GEN",
            &[("1", vec!["clean and tidy"]), ("2", vec!["one) two"])],
        )]);
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_resident(&a, None, &cfg, None, &mut cache).unwrap();

        let prior = {
            let _guard = fault::arm(fault::Phase::Judge);
            match analyze_resident(&b, None, &cfg, Some(prior), &mut cache) {
                Err((_, p)) => p,
                Ok(_) => panic!("the judge fault did not fire"),
            }
        };
        let before = cache.probe();
        let (findings, _) = analyze_resident(&b, None, &cfg, prior, &mut cache).unwrap();
        let after = cache.probe();

        assert_eq!(
            after.direct_misses - before.direct_misses,
            0,
            "the faulted attempt already warmed both chapters' products"
        );
        assert_eq!(
            after.direct_chapters_patched, 2,
            "both chapters' records are still the previous input's, so both are patched"
        );
        assert_eq!(findings, analyze_with_config(&b, None, &cfg));
    }

    /// The Phase C gate for the direct lane: under a randomized sequence of
    /// chapter replacements, whole-book replacements, book removals and
    /// re-insertions, the incrementally patched direct partitions equal a full
    /// batch rebuild (a cold complete analysis) at **every** step.
    #[test]
    fn direct_partitions_equal_a_full_rebuild_under_randomized_edits() {
        let cfg = Config::all();
        // A small deterministic LCG: readable, reproducible, no dev-dependency.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n
        };
        let bodies = [
            "a  b, joyfullly",
            "word word here",
            "x\ty",
            "one) two",
            "He said. the gate",
            "clean text",
            "A1 α qQx",
            "",
        ];

        let mut corpus = corpus_of(vec![
            chaptered(
                "GEN",
                &[
                    ("1", vec![bodies[0], bodies[1]]),
                    ("2", vec![bodies[2]]),
                    ("3", vec![bodies[4], bodies[5]]),
                ],
            ),
            chaptered("EXO", &[("1", vec![bodies[3]]), ("2", vec![bodies[6]])]),
            chaptered("LEV", &[("1", vec![bodies[7], bodies[0]])]),
        ]);
        let mut cache = AnalysisCache::new();
        let mut prior = None;
        for step in 0..40 {
            match next(4) {
                // Replace one existing chapter with a fresh verse count/text.
                0 => {
                    let books = corpus::by_book(&corpus);
                    let g = books[next(books.len())];
                    let slug = g.slug.to_string();
                    let chapter = corpus
                        .locate(g.base)
                        .chapter
                        .to_string();
                    let n = 1 + next(3);
                    let keys: Vec<String> =
                        (1..=n).map(|v| format!("{slug} {chapter}:{v}")).collect();
                    let texts: Vec<String> =
                        (0..n).map(|_| bodies[next(bodies.len())].to_string()).collect();
                    corpus
                        .replace_chapter(crate::corpus::ChapterBlock {
                            slug: slug.clone().into(),
                            chapter: chapter.into(),
                            keys,
                            texts,
                        })
                        .unwrap();
                }
                // Replace a whole book, reshaping its chapter set.
                1 => {
                    let books = corpus::by_book(&corpus);
                    let slug = books[next(books.len())].slug.to_string();
                    let chapters = 1 + next(3);
                    let mut keys = Vec::new();
                    let mut texts = Vec::new();
                    for c in 1..=chapters {
                        for v in 1..=(1 + next(2)) {
                            keys.push(format!("{slug} {c}:{v}"));
                            texts.push(bodies[next(bodies.len())].to_string());
                        }
                    }
                    corpus
                        .replace_books(vec![crate::corpus::BookBlock {
                            slug: slug.clone().into(),
                            keys,
                            texts,
                        }])
                        .unwrap();
                }
                // Remove a book (and drop it from the cache, as the shell does).
                2 => {
                    let books = corpus::by_book(&corpus);
                    if books.len() > 1 {
                        let slug = books[next(books.len())].slug.to_string();
                        assert!(corpus.remove_book(&slug));
                        cache.remove_book(&slug);
                        if let Some(p) = prior.as_mut() {
                            let p: &mut Stats = p;
                            p.remove_book(&slug);
                        }
                    }
                }
                // Append a book back (or reshape the last one if it is present).
                _ => {
                    let slug = ["GEN", "EXO", "LEV", "NUM"][next(4)];
                    let mut keys = Vec::new();
                    let mut texts = Vec::new();
                    for v in 1..=(1 + next(3)) {
                        keys.push(format!("{slug} 1:{v}"));
                        texts.push(bodies[next(bodies.len())].to_string());
                    }
                    let _ = corpus.replace_books(vec![crate::corpus::BookBlock {
                        slug: slug.into(),
                        keys,
                        texts,
                    }]);
                }
            }

            let (findings, next_prior) =
                analyze_resident(&corpus, None, &cfg, prior, &mut cache).unwrap();
            prior = Some(next_prior);
            assert_eq!(
                findings,
                analyze_with_config(&corpus, None, &cfg),
                "step {step}: patched partitions must equal a full batch rebuild"
            );
            assert_eq!(
                cache.partition_findings(&corpus),
                findings,
                "step {step}: the returned answer comes only from the partition lane"
            );
        }
    }

    /// An empty corpus (and a finding-free analyze) is valid: it yields empty
    /// findings and an empty partition lane, and assembly agrees.
    #[test]
    fn empty_corpus_and_zero_findings_are_valid() {
        let cfg = Config::all();
        let empty = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        let mut cache = AnalysisCache::new();
        let (findings, _) = analyze_resident(&empty, None, &cfg, None, &mut cache).unwrap();
        assert!(findings.is_empty(), "empty corpus yields no findings");
        assert!(
            cache.partition_findings(&empty).is_empty(),
            "empty corpus yields an empty partition lane"
        );

        // A clean verse: whatever it fires, assembly still equals the return.
        let clean = mk("GEN", &["clean and tidy text"]);
        let (f2, _) = analyze_resident(&clean, None, &cfg, None, &mut cache).unwrap();
        assert_eq!(cache.partition_findings(&clean), f2);
    }

    /// Removing a book drops its partition records from the finding lane, so a
    /// removal cannot resurrect a partition: even assembled against a corpus
    /// that still contains the book, the lane holds none of its records.
    #[test]
    fn removal_drops_partition_records_and_cannot_resurrect() {
        let cfg = Config::all();
        let ab = corpus_of(vec![
            keyed("GEN", &["a  b, joyfullly"]),
            keyed("EXO", &["x\ty word word"]),
        ]);
        let mut cache = AnalysisCache::new();
        analyze_resident(&ab, None, &cfg, None, &mut cache).unwrap();
        assert!(
            cache
                .partition_findings(&ab)
                .iter()
                .any(|f| ab.key(f.key_idx).starts_with("EXO")),
            "EXO fired and is in the lane"
        );

        // Drop EXO via the finding-lane removal entry point.
        cache.remove_book("EXO");

        // The corpus still contains EXO, but the lane holds no EXO records —
        // a removal cannot resurrect a partition.
        assert!(
            cache
                .partition_findings(&ab)
                .iter()
                .all(|f| !ab.key(f.key_idx).starts_with("EXO")),
            "removed book's partition records are gone from the lane"
        );
        assert!(
            cache
                .partition_findings(&ab)
                .iter()
                .any(|f| ab.key(f.key_idx).starts_with("GEN")),
            "the sibling book's partition survives"
        );
    }
}
