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

    /// One analyze call's coarse phase split. `map` covers direct findings,
    /// fused shared preparation, and substrate observations; `reduce` covers
    /// ordered substrate reduction plus corpus evidence updates; `judge`
    /// covers outcome evaluation and resident finding-partition patching.
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

    /// The per-substrate × per-phase split of the most recent `transition` on
    /// this thread: `[substrate][phase]`, rows indexed by
    /// [`SUBSTRATE_NAMES`] and columns by [`DRIVE_PHASE_NAMES`].
    ///
    /// The coarse `judge` figure above is one window covering every substrate's
    /// whole `drive_*` — planning, mapping, ordered reduction, judge-key
    /// discovery, judging, and materialization all land in it. This table is
    /// that window separated, so a per-substrate fixed cost can be attributed
    /// to a phase rather than to the drive as a whole.
    pub use crate::substrate::{
        DRIVE_PHASE_NAMES, SUBSTRATE_NAMES, drive_phase_table as drive_phases,
    };
}

#[cfg(test)]
mod phase_f_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn corpus(book: &str, verses: &[&str]) -> Corpus {
        Corpus::try_from_parts(
            (1..=verses.len()).map(|verse| format!("{book} 1:{verse}")).collect(),
            verses.iter().map(|text| (*text).to_owned()).collect(),
        )
        .unwrap()
    }

    #[test]
    fn every_rule_has_one_executable_owner() {
        let mut owners: BTreeMap<RuleId, u8> = BTreeMap::new();
        for rule in rule::per_verse_rules() {
            *owners.entry(rule.id()).or_default() += 1;
        }
        for &substrate in substrate::SubstrateId::ALL {
            for &rule in substrate::consumers_of(substrate) {
                *owners.entry(rule).or_default() += 1;
            }
        }
        for rule in RuleId::ALL {
            assert_eq!(owners.get(rule), Some(&1), "{}", rule.code());
        }
        assert_eq!(owners.len(), RuleId::ALL.len());
    }

    #[test]
    fn resident_mutation_matches_cold_and_removal_cannot_resurrect() {
        let cfg = Config::all();
        let initial = Corpus::try_from_parts(
            vec!["GEN 1:1".into(), "EXO 1:1".into()],
            vec!["a  b".into(), "word word".into()],
        )
        .unwrap();
        let mut cache = AnalysisCache::new();
        analyze_resident(&initial, None, &cfg, &mut cache).unwrap();
        cache.remove_book("EXO");
        assert!(cache
            .partition_findings(&initial)
            .iter()
            .all(|finding| !initial.key(finding.key_idx).starts_with("EXO")));

        let edited = corpus("GEN", &["clean text"]);
        let resident = analyze_resident(&edited, None, &cfg, &mut cache).unwrap();
        assert_eq!(resident, analyze_with_config(&edited, None, &cfg));
        assert_eq!(cache.partition_findings(&edited), resident);
    }

    #[test]
    fn faults_keep_published_partitions_and_retry_reaches_cold() {
        let cfg = Config::all();
        let before = corpus("GEN", &["a  b", "word word"]);
        let after = corpus("GEN", &["clean text", "one) two"]);
        let cold = analyze_with_config(&after, None, &cfg);

        for phase in [fault::Phase::Map, fault::Phase::Reduce, fault::Phase::Judge] {
            let mut cache = AnalysisCache::new();
            let published = analyze_resident(&before, None, &cfg, &mut cache).unwrap();
            let failed = {
                let _armed = fault::arm(phase);
                analyze_resident(&after, None, &cfg, &mut cache)
            };
            assert!(failed.is_err(), "{phase:?}");
            assert_eq!(cache.partition_findings(&before), published, "{phase:?}");
            let retried = analyze_resident(&after, None, &cfg, &mut cache).unwrap();
            assert_eq!(retried, cold, "{phase:?}");
            assert_eq!(cache.partition_findings(&after), retried, "{phase:?}");
        }
    }
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
    transition(target, source, config, &mut AnalysisCache::new())
        .expect("one-shot analysis is total without an injected fault")
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

/// The one core map/reduce/judge transition. Both the one-shot path
/// (which creates a transient cache) and the resident path (which owns one)
/// flow through this exact function. Every returned finding covers exactly the
/// complete `target` corpus. It is fallible only for the test-only fault hook;
/// map products may warm before a fault, but finding partitions commit only
/// after all phases succeed.
fn transition(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    cache: &mut AnalysisCache,
) -> Result<Vec<Finding>, AnalyzeError> {

    let all_per_verse = rule::per_verse_rules();
    // Every direct-lane rule id, enabled or not: the complete set of partitions
    // the direct lane owns. Taken from the registry (not a hand-kept list) so a
    // new per-verse rule cannot silently fall between the two partition lanes.
    let direct_ids: Vec<RuleId> = all_per_verse.iter().map(|r| r.id()).collect();
    let per_verse: Vec<_> = all_per_verse
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    // Which typed observation substrates are active is computed once from the
    // closed registry and final coalesced config.
    let active = substrate::ActiveSubstrates::from_config(config);

    // The book view is shared by both analysis lanes. `source` is intentionally not
    // part of the PREP cache fingerprint — no prep lane reads it. It is emphatically
    // NOT source-independent overall: `ProportionalitySubstrate` declares a
    // reference input, and its chapter observations are stamped with the paired
    // reference chapter's content hash (or the explicit absent tag), so reference
    // movement invalidates exactly those observations and nothing else. That
    // per-substrate stamp is the source-validity seam; this fingerprint is not.
    cache.ensure_fingerprint(config);

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

    // MAP boundary. The direct lane may be warm now, but no resident finding
    // partition has changed. A failed attempt may retain those self-validating
    // map products; the later commit still owns semantic publication.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Map) {
        return Err(AnalyzeError { phase: "map" });
    }

    // MAP boundary reached: `reduce` runs from here through the JUDGE boundary.
    #[cfg(feature = "bench-probes")]
    let bench_reduce_start = std::time::Instant::now();

    // Split the cache: direct records stay borrowed for the later patch while
    // substrates may update their self-validating products. Finding partitions
    // remain uncommitted until the final atomic boundary.
    let cache::AnalysisCache {
        prep,
        substrates,
        findings: finding_lane,
    } = &mut *cache;
    let prep: &cache::PrepSection = prep;
    let mut out: Vec<Finding> = Vec::new();

    // REDUCE boundary. No resident finding partition has been committed.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Reduce) {
        return Err(AnalyzeError { phase: "reduce" });
    }

    // JUDGE boundary reached. Every typed substrate drives below.
    #[cfg(feature = "bench-probes")]
    let bench_judge_start = std::time::Instant::now();
    // Every substrate drive runs inside the judge window, so its per-phase
    // sub-split is zeroed here — a substrate this call did not drive then reads
    // as zero rather than as the previous call's row.
    #[cfg(feature = "bench-probes")]
    substrate::reset_drive_phases();

    // Typed observation substrates (plan §5.2) own stamp-derived validity: each maps only chapters whose
    // observation input stamp changed and re-reduces an owning book only when a
    // chapter changed, so a judging-knob change reuses every observation and
    // reduction (maps/reduces zero) and re-judges from the cached corpus
    // aggregate. A disabled substrate (no active consumer) drops its products so
    // edits while it is inactive do no work for it.
    // The converted substrates' patches (plan §6.4). They never enter `out`:
    // their records go straight to their own partitions, and only after the judge
    // boundary — so a failed attempt publishes nothing and leaves the previous
    // partitions intact.
    let mut substrate_lane = substrate::SubstrateLane::default();
    signals::punctuation::drive_spacing(
        active.spacing,
        &mut substrates.spacing,
        target,
        &config.punctuation_spacing,
        &mut substrate_lane,
    );
    signals::punctuation::drive_adjacency(
        active.adjacency,
        &mut substrates.adjacency,
        target,
        &config.punctuation_adjacency,
        &mut out,
    );
    signals::lexical::drive_repeated_run(
        active.repeated_run,
        &mut substrates.repeated_run,
        target,
        &config.repeated_character_run,
        &mut out,
    );
    signals::lexical::drive_punct_only(
        active.punct_only,
        &mut substrates.punct_only,
        target,
        &config.punct_only_token,
        &mut out,
    );
    signals::script_mixing::drive_mixed_script(
        active.mixed_script,
        &mut substrates.mixed_script,
        target,
        &config.mixed_script,
        &mut out,
    );
    signals::rare_glyph::drive_rare_glyph(
        active.glyph,
        &mut substrates.glyph,
        target,
        &config.rare_glyph,
        &mut out,
    );
    signals::proportionality::drive_proportionality(
        active.proportionality,
        &mut substrates.proportionality,
        target,
        source,
        &config.proportionality,
        &mut out,
    );
    signals::mixed_normalization::drive_normalization(
        active.normalization,
        &mut substrates.normalization,
        target,
        &mut out,
    );
    signals::bracket_balance::drive_bracket(
        active.bracket,
        &mut substrates.bracket,
        target,
        &config.bracket_balance,
        &mut out,
    );
    signals::lexical::drive_duplicate_word(
        active.duplicate_word,
        &mut substrates.duplicate_word,
        target,
        &mut out,
    );
    signals::mixed_case::drive_mixed_case(
        active.mixed_case,
        signals::mixed_case::MixedCaseState {
            cache: &mut substrates.mixed_case,
            symbols: &substrates.words,
        },
        target,
        &config.mixed_case,
        &mut substrate_lane,
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
        &mut substrate_lane,
    );

    // JUDGE boundary. The products above may be warm, but findings remain
    // unpublished until the partition commit below.
    #[cfg(any(test, feature = "test-probes"))]
    if fault::fires(fault::Phase::Judge) {
        return Err(AnalyzeError { phase: "judge" });
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
    // current — no partial commit. Some substrate drivers patch their owned
    // chapters directly; the others rebuild their own partition from `out`.
    // The latter is still typed-substrate execution, not a fallback rule lane.
    let mut retained_ids = direct_ids.clone();
    retained_ids.extend(substrate_lane.owned_rules());
    finding_lane.rebuild_substrate_outputs(&out, target, &retained_ids);
    finding_lane.patch_direct(
        &direct_ids,
        prep,
        &direct_dirty,
        direct_dirty.len() == chapter_count,
        direct_present.as_ref(),
    );
    finding_lane.commit_substrates(&substrate_lane, target, direct_present.as_ref());
    for patch in &substrate_lane.patches {
        substrates.ack_committed(patch.substrate);
    }

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

    Ok(out)
}

/// Resident analysis over the [one core transition](transition) — the entry the
/// resident shell ([`Galley`](../ssc_galley/struct.Galley.html)) drives.
///
/// It receives Galley's resident cache but owns no analysis history. Fallible
/// only for the test-only fault hook; a failed call leaves finding partitions
/// untouched and a retry can reuse stamp-valid warm products.
pub fn analyze_resident(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    cache: &mut AnalysisCache,
) -> Result<Vec<Finding>, AnalyzeError> {
    transition(target, source, config, cache)
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

    // ── Resident finding partitions: the atomic finding boundary (§3.3) ──────







    /// A one-book corpus large enough to force the chapter-map parallel route.
    #[cfg(feature = "parallel")]
    fn wide_book(slug: &str, chapters: usize, verses: usize, pad: usize) -> Corpus {
        let mut keys = Vec::new();
        let mut texts = Vec::new();
        for c in 1..=chapters {
            for v in 1..=verses {
                keys.push(format!("{slug} {c}:{v}"));
                texts.push(format!("a  b) {}", "word ".repeat(pad / 5)));
            }
        }
        Corpus::try_from_parts(keys, texts).unwrap()
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




}
