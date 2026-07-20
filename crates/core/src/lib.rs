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
//! `documentation/v1-reset-design.md`.

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
pub mod key;
pub mod rule;
pub mod script;
pub mod signals;
pub mod span;
pub mod stats;
mod stream;
mod tape;
pub mod token;
pub mod unicode;

pub use cache::PrepCache;
#[cfg(any(test, feature = "test-probes"))]
pub use cache::CacheProbe;
#[cfg(feature = "bench-probes")]
pub use stream::{FloorNeeds, walk_floor};
pub use catalog::{RuleCard, SENSITIVITY_STOPS, Verdict, rule_cards};
pub use census::{CensusOptions, Inventory, census};
pub use config::{
    BracketBalanceConfig, CasingConfig, Config, ProportionalityConfig, PunctOnlyTokenConfig,
    PunctuationAdjacencyConfig, PunctuationSpacingConfig, RepeatedCharacterRunConfig,
};
pub use corpus::{BookBlock, Corpus, CorpusError, KeyIdx};
pub use diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, LengthRatioScope, RuleId,
    Severity,
};
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

/// All findings for one verse from the per-verse rules. The verse's scalar
/// tape (ADR 0045) is built once into the caller's reused `tape` buffer and
/// shared by every per-verse rule.
fn verse_findings(
    key_idx: KeyIdx,
    text: &str,
    tape: &[tape::TapeEntry],
    mask: tape::Mask,
    per_verse: &[Box<dyn rule::PerVerseRule>],
) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in per_verse {
        // Skip the clean majority: a rule runs only when the verse's dirty-bits
        // mask opens its gate (ADR 0046). The gate is a safe superset of the
        // fire set, so this never drops a finding.
        if !mask.opens(r.gate()) {
            continue;
        }
        let (code, severity) = (r.id(), r.severity());
        for range in r.check(text, tape) {
            out.push(Finding {
                key_idx,
                code,
                severity,
                range,
                score: None,
                args: None,
            });
        }
    }
    out
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

/// Analyze, returning the corpus [`Stats`] so a caller can cache it and feed
/// it back as `prior` for incremental re-analysis (ADR 0017).
///
/// `target` is the verses provided **this call**. With `prior = None` it is
/// the whole corpus; with `prior = Some`, the books present in `target`
/// **supersede** their prior entries (book granularity) and all other books
/// carry forward — so an edit re-supplies only its book.
///
/// **All returned findings cover exactly `target`'s verses** — a single
/// coherent scope the caller replaces wholesale for those sids. Stateful
/// rules judge against the *whole* merged corpus (so `target`'s verdicts
/// reflect corpus-wide statistics) but emit only for `target`; a pooled
/// statistic shifting a verdict in an untouched book surfaces when that book
/// is next supplied. (This also keeps every finding projectable: the caller
/// need only hand in the text for the verses it asked about.)
///
/// **Counting is proof-driven, never declared (supersedes ADR 0043's
/// `changed`).** With a `prior`, each supplied book is re-reduced iff its
/// current provenance — content hash, same-slug source hash, and enabled-rule
/// fingerprint — differs from the [`Tally`] the prior recorded for that slug;
/// every matching book carries its prior counts, and books absent this call
/// carry untouched (echo semantics). Judging and emission still cover all of
/// `target`, so a convention an edit tips re-emits across every supplied book
/// in one call. There is no `changed` parameter: the ~1 ms of hashing the
/// supplied books each call buys a correctness no promise could — the caller
/// cannot under-declare an edit. Without a `prior` there is nothing to carry,
/// so every supplied book counts.
pub fn analyze_stateful(
    target: &Corpus,
    source: Option<&Corpus>,
    config: &Config,
    prior: Option<Stats>,
    cache: Option<&mut PrepCache>,
) -> (Vec<Finding>, Stats) {
    use std::collections::BTreeMap;

    let per_verse: Vec<_> = rule::per_verse_rules()
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
    let plan = stream::WalkPlan {
        casing: config.is_enabled(RuleId::SentenceInitialLowercase)
            || config.is_enabled(RuleId::InconsistentWordCasing),
        adjacency: config.is_enabled(RuleId::PunctuationAdjacencyAnomaly),
        spacing: config.is_enabled(RuleId::PunctuationSpacingAnomaly),
        repeated_run: config.is_enabled(RuleId::RepeatedCharacterRun),
        punct_only: config.is_enabled(RuleId::PunctOnlyToken),
        mixed_script: config.is_enabled(RuleId::MixedScriptInToken),
        rare_glyph: config.is_enabled(RuleId::RareGlyph),
        mixed_case: config.is_enabled(RuleId::MixedCaseWord),
        proportionality: config.is_enabled(RuleId::ProjectLengthRatio),
        bracket: config.is_enabled(RuleId::BracketBalance),
        duplicate: config.is_enabled(RuleId::DuplicateWord),
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
    let mut cache = cache;
    if let Some(cache) = cache.as_deref_mut() {
        cache.ensure_fingerprint(config);
    }
    // Every supplied book's content hash, on EVERY call (~0.5–1 ms serial on a
    // full Bible): the counting decision is proven from these hashes against the
    // prior's per-book provenance, so there is no zero-hash path — fresh tallies
    // must be stamped even on a cold, cache-less call.
    let hashes: Vec<u128> = books.iter().map(cache::book_hash).collect();
    // Source book hashes by slug, for per-book source provenance. A target book
    // pairs only with the same-slug source book (its keys parse to its slug), so
    // that is the only source text its counts depend on.
    let source_hashes: Option<BTreeMap<&str, u128>> = source.map(|s| {
        corpus::by_book(s)
            .iter()
            .map(|g| (g.slug, cache::book_hash(g)))
            .collect()
    });
    // Fingerprint of the enabled counting-rule set: records which rules'
    // contributions a book's counts include, so toggling any rule re-tallies.
    let rules_fp = rules_fp(&stateful);

    // The per-verse phase is embarrassingly parallel — each verse is judged
    // from its own text by `Sync` rules. Under the `parallel` feature it fans
    // out over rayon (ADR 0018); otherwise it stays serial. Output is the same
    // either way: `out` is sorted before return, so order never depends on the
    // feature.
    // The verse's scalar tape (ADR 0045) is built once per verse into a reused
    // buffer — a `map_init` per-worker buffer under `parallel`, a plain reused
    // `Vec` serially — and shared by every per-verse rule, replacing their ~10
    // separate `char_indices()` walks with one decode+classify pass.
    let mut out: Vec<Finding> = if let Some(cache) = cache.as_deref_mut() {
        let mut out = Vec::new();
        let mut misses: corpus::Books<'_> = Vec::new();
        let mut miss_hashes: Vec<u128> = Vec::new();
        for (group, &hash) in books.iter().zip(hashes.iter()) {
            if let Some(findings) = cache.per_verse_hit(group.slug, hash, group.base) {
                out.extend(findings);
            } else {
                misses.push(*group);
                miss_hashes.push(hash);
            }
        }

        let fresh = rule::map_books(&misses, |group| {
            let mut findings = Vec::new();
            let mut tape_buf = Vec::new();
            for (vi, text) in group.texts.iter().enumerate() {
                let key_idx = corpus::rebase(group.base, LocalKeyIdx::from_usize(vi));
                let mask = tape::build_masked(text, &mut tape_buf);
                findings.extend(verse_findings(key_idx, text, &tape_buf, mask, &per_verse));
            }
            findings
        });
        for ((group, hash), findings) in misses.iter().zip(miss_hashes.iter()).zip(fresh) {
            cache.store_per_verse(group.slug, *hash, group.base, &findings);
            out.extend(findings);
        }
        out
    } else {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            target
                .texts()
                .par_iter()
                .enumerate()
                .map_init(Vec::new, |tape_buf, (i, text)| {
                    let key_idx = KeyIdx::from_usize(i);
                    let mask = tape::build_masked(text, tape_buf);
                    verse_findings(key_idx, text, tape_buf, mask, &per_verse)
                })
                .flatten_iter()
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            let mut out = Vec::new();
            let mut tape_buf = Vec::new();
            for (i, text) in target.texts().iter().enumerate() {
                let key_idx = KeyIdx::from_usize(i);
                let mask = tape::build_masked(text, &mut tape_buf);
                out.extend(verse_findings(key_idx, text, &tape_buf, mask, &per_verse));
            }
            out
        }
    };

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
    // snapshot shape. Echo and cold calls must walk every supplied book so
    // their counting and emission semantics remain exactly unchanged.
    //
    // `fused` ends up index-aligned with `books` (its presented order): a
    // walked book lands at its original position via `walk_positions`; a
    // cache-hit book is synthesized directly into that position. Never
    // reassembled by book identity — `walk_fused`'s output is aligned only
    // to whatever subset of `books` it was given.
    let mut fused: Vec<stream::BookOut> = if let Some(cache) = cache.as_mut() {
        let mut books_to_walk: corpus::Books<'_> = Vec::new();
        let mut walk_positions: Vec<usize> = Vec::new();
        let mut cached_walks: Vec<(usize, cache::CachedWalk)> = Vec::new();
        for (i, group) in books.iter().enumerate() {
            if counted.is_some_and(|list| !list.contains(&group.slug))
                && let Some(cached) = cache.cloned_walk(group.slug, hashes[i], &plan)
            {
                cached_walks.push((i, cached));
            } else {
                books_to_walk.push(*group);
                walk_positions.push(i);
            }
        }

        // ONE walk per verse per book (the event-stream engine): tape,
        // graphemes and tokens are each built once per verse and every
        // enabled listener is fed in-pass. Fan-out per book under `parallel`
        // (ADR 0042).
        let walked = stream::walk_fused(&books_to_walk, counted, source, &plan);

        // Write every walked book before cached books are synthesized in.
        for ((&i, group), output) in walk_positions
            .iter()
            .zip(books_to_walk.iter())
            .zip(walked.iter())
        {
            cache.store_walk(group.slug, hashes[i], output);
        }

        let mut slots: Vec<Option<stream::BookOut>> = (0..books.len()).map(|_| None).collect();
        for (i, output) in walk_positions.into_iter().zip(walked) {
            slots[i] = Some(output);
        }
        for (i, cached) in cached_walks {
            slots[i] = Some(stream::BookOut {
                counted: false,
                // A cache-hit book is clean: it did no counting work.
                #[cfg(any(test, feature = "test-probes"))]
                counting_accs_ran: false,
                casing: plan
                    .casing
                    .then(|| (Default::default(), cached.casing.expect("casing lane hit"))),
                adjacency: plan.adjacency.then(|| {
                    (
                        Default::default(),
                        cached.adjacency.expect("adjacency lane hit"),
                    )
                }),
                spacing: plan.spacing.then(|| {
                    (
                        Default::default(),
                        cached.spacing.expect("spacing lane hit"),
                    )
                }),
                repeated_run: plan.repeated_run.then(|| {
                    (
                        Default::default(),
                        cached.repeated_run.expect("repeated-run lane hit"),
                    )
                }),
                punct_only: plan.punct_only.then(|| {
                    (
                        Default::default(),
                        cached.punct_only.expect("punct-only lane hit"),
                    )
                }),
                mixed_script: plan.mixed_script.then(|| {
                    (
                        Default::default(),
                        cached.mixed_script.expect("mixed-script lane hit"),
                    )
                }),
                rare_glyph: None,
                mixed_case: None,
                proportionality: None,
                bracket: plan
                    .bracket
                    .then(|| cached.bracket.expect("bracket lane hit")),
                duplicate: plan
                    .duplicate
                    .then(|| cached.duplicate.expect("duplicate lane hit")),
                normalization: plan
                    .normalization
                    .then(|| cached.normalization.expect("normalization lane hit")),
                tokens: plan
                    .collect_tokens
                    .then(|| cached.tokens.expect("token lane hit")),
            });
        }
        slots
            .into_iter()
            .map(|s| s.expect("every book walked or cache-hit"))
            .collect()
    } else {
        stream::walk_fused(&books, counted, source, &plan)
    };

    // Counting-side probe: count the books whose site-free counting
    // accumulators actually ran (`counting_accs_ran`), observed from the
    // accumulators — not from the `counted` decision flag. A listener that
    // counted an anchor-mode book (ignoring the stale set) would set this true
    // while `counted` stayed false, so the probe would catch it. Meaningful
    // when at least one site-free counting rule (rare-glyph / mixed-case /
    // proportionality) is enabled.
    #[cfg(any(test, feature = "test-probes"))]
    if let Some(cache) = cache {
        cache.note_retallied(fused.iter().filter(|o| o.counting_accs_ran).count());
    }

    let token_cache: Option<rule::TokenCache> = plan
        .collect_tokens
        .then(|| stream::assemble_token_cache(&mut fused, &books));

    // Project findings, from the fused listeners' per-book outputs.
    if plan.bracket {
        let matches: Vec<_> = fused
            .iter_mut()
            .map(|b| {
                b.bracket
                    .take()
                    .expect("bracket listener ran on every book")
            })
            .collect();
        out.extend(signals::bracket_balance::emit(
            &books,
            &matches,
            &config.bracket_balance,
        ));
    }
    if plan.duplicate {
        for (group, b) in books.iter().zip(fused.iter_mut()) {
            let hits = b
                .duplicate
                .take()
                .expect("duplicate listener ran on every book");
            out.extend(signals::lexical::emit(group, hits));
        }
    }
    if plan.normalization {
        let summaries: Vec<_> = fused
            .iter_mut()
            .map(|b| {
                b.normalization
                    .take()
                    .expect("normalization listener ran on every book")
            })
            .collect();
        out.extend(signals::mixed_normalization::emit(&books, &summaries));
    }

    // Assemble each rule's fresh stats + forwarded sites (ADR 0044) from the
    // fused per-book outputs. A rule enabled this call always gets an entry —
    // possibly empty — exactly as its own reduce produced. A book outside the
    // `counted` scope contributes **sites only** (the walk visited it for
    // anchors; its counts carry from the prior through the supersede merge),
    // so the judge phase is site-driven for every supplied book and never
    // re-scans — except the deliberately site-free rules (proportionality
    // never scans; rare-glyph / mixed-case re-scan by design, ADR 0053/0055).
    let casing_fresh = plan.casing.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.casing.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
            }
        }
        (
            signals::casing::CasingStats { per_book: pb },
            rule::RuleSites::Casing(st),
        )
    });
    let mut adjacency_fresh = plan.adjacency.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.adjacency.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
            }
        }
        (
            RuleStats::PunctuationAdjacency(signals::punctuation::PunctuationAdjacencyStats {
                per_book: pb,
            }),
            rule::RuleSites::PunctuationAdjacency(st),
        )
    });
    let mut spacing_fresh = plan.spacing.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.spacing.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
            }
        }
        (
            RuleStats::PunctuationSpacing(signals::punctuation::PunctuationSpacingStats {
                per_book: pb,
            }),
            rule::RuleSites::PunctuationSpacing(st),
        )
    });
    let mut repeated_fresh = plan.repeated_run.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.repeated_run.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
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
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.punct_only.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
            }
        }
        (
            RuleStats::PunctOnlyToken(signals::lexical::PunctOnlyTokenStats { per_book: pb }),
            rule::RuleSites::PunctOnlyToken(st),
        )
    });
    let mut mixed_script_fresh = plan.mixed_script.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some((bc, s)) = o.mixed_script.take() {
                if o.counted {
                    pb.insert(Box::from(group.slug), bc);
                }
                st.insert(Box::from(group.slug), s);
            }
        }
        (
            RuleStats::MixedScript(signals::script_mixing::MixedScriptStats { per_book: pb }),
            rule::RuleSites::MixedScript(st),
        )
    });
    let mut rare_glyph_fresh = plan.rare_glyph.then(|| {
        let mut pb = BTreeMap::new();
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some(bg) = o.rare_glyph.take() {
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
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some(bmc) = o.mixed_case.take() {
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
        for (group, o) in books.iter().zip(fused.iter_mut()) {
            if let Some(bucket) = o.proportionality.take() {
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
    drop(fused);

    // Stateful rules: supersede the prior cache at book granularity, judge the
    // whole merged corpus from the cache.
    //
    // Deliberately sequential over rules: pooling all rules' reduces/judges
    // into two rule×book task pools was tried (2026-07-07) and measured at
    // parity-to-slightly-worse (see ADR 0042's rejected alternatives). The
    // counting itself now happens once, fused, above.
    let mut stats = prior.unwrap_or_default();
    for r in &stateful {
        let id = r.id();
        let sites_slot;
        // The fused walk's fresh stats + forwarded sites for this rule (ADR
        // 0044). Both casing rules share the one word-table walk: each takes a
        // clone of the stats (the wire shape keeps one entry per rule id, as
        // before) and judges from the same site list.
        let (fresh, sites_ref): (RuleStats, &rule::RuleSites) = match id {
            RuleId::SentenceInitialLowercase | RuleId::InconsistentWordCasing => {
                let (cs, ss) = casing_fresh
                    .as_ref()
                    .expect("enabled casing rule implies the casing listener ran");
                (RuleStats::Casing(cs.clone()), ss)
            }
            RuleId::PunctuationAdjacencyAnomaly => {
                let (st, ss) = adjacency_fresh.take().expect("listener ran");
                sites_slot = ss;
                (st, &sites_slot)
            }
            RuleId::PunctuationSpacingAnomaly => {
                let (st, ss) = spacing_fresh.take().expect("listener ran");
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

    // Stamp per-book provenance for every supplied book: a freshly-counted book
    // gets its new Tally; a non-stale supplied book gets the identical value it
    // already carried (a no-op by construction). Books in the prior but not
    // supplied this call keep their own Tally untouched — nothing global is
    // updated over their heads (echo semantics).
    for (i, group) in books.iter().enumerate() {
        stats.tallied.insert(Box::from(group.slug), current[i]);
    }

    // Deterministic order, independent of the `parallel` feature (ADR 0018):
    // the parallel per-verse phase collects in nondeterministic order, so sort
    // by (key_idx, range start, rule) to make feature-on output byte-identical
    // to serial. Cheap against the analysis: one O(n log n) over the findings.
    out.sort_by_key(|f| (f.key_idx, f.range.start, f.code));

    (out, stats)
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
        let mut cache = PrepCache::new();

        let (cold_findings, cold_stats) =
            analyze_stateful(&target, None, &cfg, None, Some(&mut cache));
        let misses_after_cold = cache.lane1_miss_count();
        let (warm_findings, warm_stats) =
            analyze_stateful(&target, None, &cfg, None, Some(&mut cache));

        assert_eq!(cold_findings, warm_findings);
        assert_eq!(cold_stats, warm_stats);
        assert_eq!(
            misses_after_cold, 2,
            "one lane-1 miss per book on cold call"
        );
        assert_eq!(
            cache.lane1_hit_count(),
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
        let mut cache = PrepCache::new();

        let (_, _) = analyze_stateful(&target, None, &cfg, None, Some(&mut cache));
        let mut changed_cfg = cfg.clone();
        changed_cfg.rules.insert(RuleId::BracketBalance, false);
        let (_, changed_prior) =
            analyze_stateful(&target, None, &changed_cfg, None, Some(&mut cache));

        // A config change clears the old products, so the first call under the
        // new fingerprint warms both books instead of reading either lane.
        assert_eq!(cache.lane1_hit_count(), 0);
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
        assert_eq!(cache.lane1_hit_count(), 2);
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
        let mut cache = PrepCache::new();
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
        let mut cache = PrepCache::new();
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
        let mut cache = PrepCache::new();
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
        let mut cache = PrepCache::new();
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
        let mut cache = PrepCache::new();
        let empty = Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap();
        let (_, _) = analyze_stateful(&empty, None, &cfg, None, Some(&mut cache));
        assert_eq!(cache.book_count(), 0);

        let caseless = mk("GEN", &["你好"]);
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, Some(&mut cache));
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, Some(&mut cache));
        assert_eq!(
            cache.lane1_hit_count(),
            1,
            "prior-none calls still reuse pure findings"
        );
        assert!(
            cache
                .books
                .get("GEN")
                .and_then(|entry| entry.casing.as_ref())
                .is_some_and(|sites| sites.sites.is_empty())
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
        let mut cache = PrepCache::new();
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
    fn echo_subset_keeps_sibling_cache_entries_and_matches_cold_echo() {
        let full = corpus_of(vec![
            keyed("GEN", &["a  b", "one"]),
            keyed("EXO", &["x\ty", "two"]),
        ]);
        let cfg = Config::v1_defaults();
        let mut cache = PrepCache::new();
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, Some(&mut cache));
        let gen_hash = cache.entry_hash("GEN").unwrap();
        let exo_hash = cache.entry_hash("EXO").unwrap();
        let echo = mk("EXO", &["x\ty", "two"]);

        let (cached_findings, cached_stats) =
            analyze_stateful(&echo, None, &cfg, Some(prior.clone()), Some(&mut cache));
        let (cold_findings, cold_stats) = analyze_stateful(&echo, None, &cfg, Some(prior), None);

        assert_eq!(cached_findings, cold_findings);
        assert_eq!(cached_stats, cold_stats);
        assert_eq!(cache.entry_hash("GEN"), Some(gen_hash));
        assert_eq!(cache.entry_hash("EXO"), Some(exo_hash));
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

    /// `Stats` survives a strongly-typed serde round-trip (the wasm-boundary
    /// contract, ADR 0017), and re-supplying the same books as `prior`
    /// supersedes them — yielding identical findings.
    #[test]
    fn stateful_stats_round_trip_and_supersede() {
        let cfg = casing_on(0.5, 0.0);
        let target = mks("GEN", &casing_fire(40));

        let (f1, stats) = analyze_stateful(&target, None, &cfg, None, None);
        assert!(
            f1.iter()
                .any(|f| f.code == RuleId::SentenceInitialLowercase)
        );

        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();

        let (f2, _) = analyze_stateful(&target, None, &cfg, Some(back), None);
        assert_eq!(f1, f2);
    }

    /// All returned findings cover exactly `target` (ADR 0017). An
    /// incremental call for one book never returns another book's findings —
    /// the wasm boundary can then always project them (no out-of-bounds slice
    /// against an empty/absent verse).
    #[test]
    fn incremental_findings_are_scoped_to_target() {
        let cfg = casing_on(0.5, 0.0);
        let anomalous = casing_fire(40);
        let full = corpus_of(vec![keyed("GEN", &anomalous), keyed("EXO", &anomalous)]);

        let (f_full, stats) = analyze_stateful(&full, None, &cfg, None, None);
        assert!(f_full.iter().any(|f| {
            crate::key::parse_key(full.key(f.key_idx)).unwrap().book == "GEN"
                && f.code == RuleId::SentenceInitialLowercase
        }));
        assert!(f_full.iter().any(|f| {
            crate::key::parse_key(full.key(f.key_idx)).unwrap().book == "EXO"
                && f.code == RuleId::SentenceInitialLowercase
        }));

        let exo_only = mks("EXO", &anomalous);
        let (f_inc, _) = analyze_stateful(&exo_only, None, &cfg, Some(stats), None);
        assert!(!f_inc.is_empty());
        assert!(
            f_inc
                .iter()
                .all(|f| { crate::key::parse_key(exo_only.key(f.key_idx)).unwrap().book == "EXO" })
        ); // nothing from GEN
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
    /// exactly one runner registry. A rule that is implemented but never wired
    /// in — the ADR-0031 P0, where `punct.adjacency-anomaly` ran in calibration
    /// but was absent from `stateful_rules`, so it never fired through
    /// `analyze` — surfaces here as a count of zero.
    #[test]
    fn every_rule_id_is_claimed_by_exactly_one_registry() {
        use std::collections::BTreeMap;
        let cfg = Config::v1_defaults();
        // Registries are membership-complete (they include rules `v1_defaults`
        // disables); config only feeds knobs, so any config yields the full set.
        let pv = rule::per_verse_rules();
        let pr = rule::project_rules(&cfg);
        let pt = rule::project_token_rules();
        let sf = rule::stateful_rules(&cfg);
        let mut seen: BTreeMap<RuleId, u32> = BTreeMap::new();
        for id in pv
            .iter()
            .map(|r| r.id())
            .chain(pr.iter().map(|r| r.id()))
            .chain(pt.iter().map(|r| r.id()))
            .chain(sf.iter().map(|r| r.id()))
        {
            *seen.entry(id).or_default() += 1;
        }
        for &id in RuleId::ALL {
            assert_eq!(
                seen.get(&id).copied().unwrap_or(0),
                1,
                "{} must be wired into exactly one runner registry",
                id.code()
            );
        }
        assert_eq!(
            seen.len(),
            RuleId::ALL.len(),
            "a registry emitted an unknown id"
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
        let mut cache = PrepCache::new();
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

    /// An echo subset carries an unsupplied book's `Tally` untouched.
    #[test]
    fn tally_echo_subset_carries_book_and_its_tally() {
        let cfg = Config::all();
        let full = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, None);
        let echo = mk("EXO", &["x\ty", "two"]);
        let (_, stats) = analyze_stateful(&echo, None, &cfg, Some(prior.clone()), None);
        assert_eq!(stats.tallied["GEN"], prior.tallied["GEN"], "unsupplied GEN carries its Tally");
        assert_eq!(stats.tallied["EXO"], prior.tallied["EXO"], "unchanged EXO matches");
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

    /// The serialized `Stats` round-trips with `tallied`, whose hash fields
    /// are fixed-width lowercase hex strings (never a JS number).
    #[test]
    fn tally_wire_round_trips_with_hex_fields() {
        let cfg = Config::all();
        let target = corpus_of(vec![keyed("GEN", &["a  b", "one"]), keyed("EXO", &["x\ty", "two"])]);
        let (_, stats) = analyze_stateful(&target, None, &cfg, None, None);
        assert!(!stats.tallied.is_empty());
        let json = serde_json::to_string(&stats).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let (_, tally) = v["tallied"].as_object().unwrap().iter().next().unwrap();
        assert_eq!(tally["text"].as_str().unwrap().len(), 32, "u128 text ⇒ 32 hex chars");
        assert_eq!(tally["source"].as_str().unwrap().len(), 32, "u128 source ⇒ 32 hex chars");
        assert_eq!(tally["rules"].as_str().unwrap().len(), 16, "u64 rules ⇒ 16 hex chars");
        let back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, back, "Stats round-trips through serde");
    }

    /// The source partial-echo regression — echo one book under a new
    /// source, then a full call; the un-echoed book re-tallies from its OWN
    /// `Tally.source`, so no global field falsely certifies its stale counts.
    #[test]
    fn tally_source_partial_echo_regression() {
        let cfg = Config::all();
        let target = corpus_of(vec![
            keyed("GEN", &["aa bb", "cc dd"]),
            keyed("EXO", &["ee ff", "gg hh"]),
        ]);
        let source_x = corpus_of(vec![
            keyed("GEN", &["x1 x2", "x3 x4"]),
            keyed("EXO", &["x5 x6", "x7 x8"]),
        ]);
        let source_y = corpus_of(vec![
            keyed("GEN", &["y1 y2", "y3 y4"]),
            keyed("EXO", &["y5 y6", "y7 y8"]),
        ]);
        let (_, prior_x) = analyze_stateful(&target, Some(&source_x), &cfg, None, None);
        let gen_only = mk("GEN", &["aa bb", "cc dd"]);
        let (_, after_echo) =
            analyze_stateful(&gen_only, Some(&source_y), &cfg, Some(prior_x), None);
        let (f_full, s_full) =
            analyze_stateful(&target, Some(&source_y), &cfg, Some(after_echo), None);
        let (f_cold, s_cold) = analyze_stateful(&target, Some(&source_y), &cfg, None, None);
        assert_eq!(f_full, f_cold, "un-echoed EXO re-tallies under source Y");
        assert_eq!(s_full, s_cold);
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
        let mut cache = PrepCache::new();
        let (_, prior) = analyze_stateful(&target, None, &cfg1, None, Some(&mut cache));
        assert_eq!(cache.retallied_count(), 1, "the cold call counts the one book");
        let (f_inc, s_inc) =
            analyze_stateful(&target, None, &cfg2, Some(prior.clone()), Some(&mut cache));
        assert_eq!(cache.retallied_count(), 0, "knob-only change did no counting work");
        assert_eq!(s_inc.tallied, prior.tallied, "provenance unchanged");
        let (f_cold, _) = analyze_stateful(&target, None, &cfg2, None, None);
        assert_eq!(f_inc, f_cold, "findings track the new knobs (judging moved, counting didn't)");
    }

    /// The disable→re-enable round trip (the disabled-rule retention invariant) —
    /// a book carried while its rule was disabled keeps that rule's contribution,
    /// so re-enabling reproduces cold-with-R. Then the same with the echoed book
    /// edited while disabled: it re-tallies, the carried book still contributes.
    #[test]
    fn tally_disable_reenable_round_trip_retains_contribution() {
        let cfg_on = casing_on(0.5, 0.0);
        let mut cfg_off = cfg_on.clone();
        cfg_off.rules.insert(RuleId::SentenceInitialLowercase, false);
        let a = casing_fire(40);
        let b = casing_fire(40);
        let full = corpus_of(vec![keyed("GEN", &a), keyed("EXO", &b)]);
        let (_, prior_on) = analyze_stateful(&full, None, &cfg_on, None, None);

        // Disable R, echo GEN only; re-enable R, analyze GEN+EXO.
        let gen_only = mks("GEN", &a);
        let (_, after_disable) =
            analyze_stateful(&gen_only, None, &cfg_off, Some(prior_on.clone()), None);
        let (f_re, s_re) = analyze_stateful(&full, None, &cfg_on, Some(after_disable), None);
        let (f_cold, s_cold) = analyze_stateful(&full, None, &cfg_on, None, None);
        assert_eq!(f_re, f_cold, "EXO's carried R contribution survives the disable");
        assert_eq!(s_re, s_cold);

        // Now GEN edited while R was disabled: GEN re-tallies, EXO still carries.
        let mut a2 = casing_fire(40);
        a2.push("He fell. the extra one.".to_string());
        let full2 = corpus_of(vec![keyed("GEN", &a2), keyed("EXO", &b)]);
        let gen_only2 = mks("GEN", &a2);
        let (_, after_disable2) =
            analyze_stateful(&gen_only2, None, &cfg_off, Some(prior_on), None);
        let (f_re2, s_re2) = analyze_stateful(&full2, None, &cfg_on, Some(after_disable2), None);
        let (f_cold2, s_cold2) = analyze_stateful(&full2, None, &cfg_on, None, None);
        assert_eq!(f_re2, f_cold2);
        assert_eq!(s_re2, s_cold2);
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
}
