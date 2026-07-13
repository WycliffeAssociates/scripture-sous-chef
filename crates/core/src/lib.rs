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
pub mod catalog;
pub mod census;
pub mod charclass;
mod cache;
mod charclass_table;
pub mod config;
pub mod diagnostics;
mod evidence;
pub mod grapheme;
pub mod rule;
pub mod script;
pub mod sid;
pub mod signals;
pub mod span;
pub mod stats;
mod stream;
mod tape;
pub mod token;
pub mod unicode;
pub mod verse;

pub use cache::AnalysisCache;
pub use catalog::{RuleCard, SENSITIVITY_STOPS, Verdict, rule_cards};
pub use census::{CensusOptions, Inventory, census};
pub use config::{
    BracketBalanceConfig, CasingConfig, Config, ProportionalityConfig, PunctOnlyTokenConfig,
    PunctuationAdjacencyConfig, PunctuationSpacingConfig, RepeatedCharacterRunConfig,
};
pub use diagnostics::{
    BracketMeasure, DelimObservation, DelimRole, Finding, FindingArgs, LengthRatioScope, RuleId,
    Severity,
};
pub use sid::{BookId, Sid};
pub use span::{GraphemeSpan, Span, Utf16Span};
pub use stats::{RuleStats, Stats};
pub use verse::VerseMap;

/// Analyze a corpus with the shipped default rule set.
///
/// Convenience over [`analyze_with_config`] with [`Config::v1_defaults`]
/// (language-agnostic rules on; convention-dependent rules off, opt-in).
/// `target` is the verses to check; `source` is an optional parallel
/// corpus for source-relative rules (proportionality). The map's scope is
/// the analysis scope: pass a verse, a book, or a whole project.
pub fn analyze(target: &VerseMap, source: Option<&VerseMap>) -> Vec<Finding> {
    analyze_with_config(target, source, &Config::v1_defaults())
}

/// All findings for one verse from the per-verse rules. The verse's scalar
/// tape (ADR 0045) is built once into the caller's reused `tape` buffer and
/// shared by every per-verse rule.
fn verse_findings(
    sid: Sid,
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
                sid,
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
    target: &VerseMap,
    source: Option<&VerseMap>,
    config: &Config,
) -> Vec<Finding> {
    // The one-shot sugar over the stateful entry point: no prior, discard
    // the returned stats (ADR 0017).
    analyze_stateful(target, source, config, None, None, None).0
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
/// **`changed` narrows the *counting*, never the emission (ADR 0043).** With
/// `prior = Some` and `changed = Some(books)`, only the named books are
/// re-reduced — every other supplied book carries its prior counts — while
/// judging and emission still cover all of `target`. This is the
/// complete-snapshot call for a shell that holds the whole corpus: pass
/// everything, name what was edited, and a convention the edit tipped
/// re-emits in *every* book in this one call, at roughly half the full-pass
/// cost (the counting half). `changed` is a **promise, not a filter**: name
/// fewer books than were actually edited since `prior` and their counts go
/// silently stale. It is ignored without a `prior` (there are no carried
/// counts to reuse — everything must be counted).
pub fn analyze_stateful(
    target: &VerseMap,
    source: Option<&VerseMap>,
    config: &Config,
    prior: Option<Stats>,
    changed: Option<&[BookId]>,
    cache: Option<&mut AnalysisCache>,
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
        collect_tokens: config.is_enabled(RuleId::RepeatedCharacterRun)
            || config.is_enabled(RuleId::RareGlyph)
            || config.is_enabled(RuleId::MixedCaseWord)
            || config.is_enabled(RuleId::MixedScriptInToken),
    };

    // The book view is shared by both analysis lanes. A cache only hashes and
    // fingerprints at call entry; the cache-free path remains the shipped
    // walk and per-verse loop below. `source` is intentionally not part of
    // the fingerprint: it feeds only proportionality counting, counting
    // never reads the cache, and no cached lane depends on source.
    let books = verse::by_book(target);
    let mut cache = cache;
    let hashes: BTreeMap<BookId, u128> = match cache.as_deref_mut() {
        Some(cache) => {
            cache.ensure_fingerprint(config);
            books
                .iter()
                .map(|(&book, verses)| (book, cache::book_hash(verses)))
                .collect()
        }
        None => BTreeMap::new(),
    };

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
        let mut misses = BTreeMap::new();
        for (&book, verses) in &books {
            if let Some(findings) = cache.per_verse_hit(book, hashes[&book]) {
                out.extend(findings);
            } else {
                misses.insert(book, verses.clone());
            }
        }

        let fresh = rule::map_books(&misses, |book, verses| {
            let mut findings = Vec::new();
            let mut tape_buf = Vec::new();
            for &(sid, text) in verses {
                let mask = tape::build_masked(text, &mut tape_buf);
                findings.extend(verse_findings(sid, text, &tape_buf, mask, &per_verse));
            }
            (book, findings)
        });
        for (book, findings) in fresh {
            cache.store_per_verse(book, hashes[&book], findings.clone());
            out.extend(findings);
        }
        out
    } else {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            target
                .par_iter()
                .map_init(Vec::new, |tape_buf, (&sid, text)| {
                    let mask = tape::build_masked(text, tape_buf);
                    verse_findings(sid, text, tape_buf, mask, &per_verse)
                })
                .flatten_iter()
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            let mut out = Vec::new();
            let mut tape_buf = Vec::new();
            for (&sid, text) in target {
                let mask = tape::build_masked(text, &mut tape_buf);
                out.extend(verse_findings(sid, text, &tape_buf, mask, &per_verse));
            }
            out
        }
    };

    // The by-book view, computed once and shared by the project and stateful
    // phases — the book is the corpus-scoped unit (supersede granularity,
    // cross-verse seams, and the `parallel` fan-out; ADR 0042). Rules never
    // rebuild this grouping.
    // The reduce scope (ADR 0043): with a prior and `changed`, only the named
    // books are re-counted — the others' counts carry forward through the
    // supersede merge untouched. The walk still visits every supplied book
    // (the project listeners' and the token cache's scope); `counted` only
    // gates the counting listeners. Without a prior there are no carried
    // counts, so `changed` is ignored for correctness.
    let counted: Option<&[BookId]> = match (&prior, changed) {
        (Some(_), Some(list)) => Some(list),
        _ => None,
    };

    // A walk-product hit is safe only for a clean book in the complete
    // snapshot shape. Echo and cold calls must walk every supplied book so
    // their counting and emission semantics remain exactly unchanged.
    let mut cached_books = Vec::new();
    let mut walk_books = None;
    if let Some(cache) = cache.as_mut() {
        let mut books_to_walk = books.clone();
        for &book in books.keys() {
            if counted.is_some_and(|list| !list.contains(&book))
                && let Some(cached) = cache.cloned_walk(book, hashes[&book], &plan)
            {
                books_to_walk.remove(&book);
                cached_books.push((book, cached));
            }
        }
        walk_books = Some(books_to_walk);
    }

    // ONE walk per verse per book (the event-stream engine): tape, graphemes
    // and tokens are each built once per verse and every enabled listener is
    // fed in-pass — this replaces the per-rule corpus walks the reduce phase
    // used to run. Fan-out per book under `parallel` (ADR 0042).
    let books_to_walk = walk_books.as_ref().unwrap_or(&books);
    let mut fused = stream::walk_fused(books_to_walk, counted, source, &plan);

    // Write every walked book before assembly takes products out of `fused`.
    // Cached books are then synthesized with empty stats halves: the assembly
    // below inserts those halves only when `counted` is true, so carried stats
    // remain authoritative while the cached products still feed judges.
    if let Some(cache) = cache.as_mut() {
        for (&book, output) in &fused {
            cache.store_walk(book, hashes[&book], output);
        }
        for (book, cached) in cached_books {
            fused.insert(
                book,
                stream::BookOut {
                    counted: false,
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
                        (Default::default(), cached.spacing.expect("spacing lane hit"))
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
                    bracket: plan.bracket.then(|| cached.bracket.expect("bracket lane hit")),
                    duplicate: plan
                        .duplicate
                        .then(|| cached.duplicate.expect("duplicate lane hit")),
                    tokens: plan
                        .collect_tokens
                        .then(|| cached.tokens.expect("token lane hit")),
                },
            );
        }
    }

    let token_cache: Option<rule::TokenCache> =
        plan.collect_tokens.then(|| stream::assemble_token_cache(&mut fused));

    // Project findings, from the fused listeners' per-book outputs.
    if plan.bracket {
        let matches: Vec<_> = fused
            .values_mut()
            .map(|b| b.bracket.take().expect("bracket listener ran on every book"))
            .collect();
        out.extend(signals::bracket_balance::emit(matches, &config.bracket_balance));
    }
    if plan.duplicate {
        for b in fused.values_mut() {
            out.extend(b.duplicate.take().expect("duplicate listener ran on every book"));
        }
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
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.casing.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
            }
        }
        (
            signals::casing::CasingStats { per_book: pb },
            rule::RuleSites::Casing(st),
        )
    });
    let mut adjacency_fresh = plan.adjacency.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.adjacency.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
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
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.spacing.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
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
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.repeated_run.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
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
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.punct_only.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
            }
        }
        (
            RuleStats::PunctOnlyToken(signals::lexical::PunctOnlyTokenStats { per_book: pb }),
            rule::RuleSites::PunctOnlyToken(st),
        )
    });
    let mut mixed_script_fresh = plan.mixed_script.then(|| {
        let (mut pb, mut st) = (BTreeMap::new(), BTreeMap::new());
        for (&book, o) in fused.iter_mut() {
            if let Some((bc, s)) = o.mixed_script.take() {
                if o.counted {
                    pb.insert(book, bc);
                }
                st.insert(book, s);
            }
        }
        (
            RuleStats::MixedScript(signals::script_mixing::MixedScriptStats { per_book: pb }),
            rule::RuleSites::MixedScript(st),
        )
    });
    let mut rare_glyph_fresh = plan.rare_glyph.then(|| {
        let mut pb = BTreeMap::new();
        for (&book, o) in fused.iter_mut() {
            if let Some(bg) = o.rare_glyph.take() {
                pb.insert(book, bg);
            }
        }
        (
            RuleStats::GlyphInventory(signals::rare_glyph::RareGlyphStats { per_book: pb }),
            rule::RuleSites::RareGlyph,
        )
    });
    let mut mixed_case_fresh = plan.mixed_case.then(|| {
        let mut pb = BTreeMap::new();
        for (&book, o) in fused.iter_mut() {
            if let Some(bmc) = o.mixed_case.take() {
                pb.insert(book, bmc);
            }
        }
        (
            RuleStats::MixedCase(signals::mixed_case::MixedCaseStats { per_book: pb }),
            rule::RuleSites::MixedCase,
        )
    });
    let mut proportionality_fresh = plan.proportionality.then(|| {
        let mut pb = BTreeMap::new();
        for (&book, o) in fused.iter_mut() {
            if let Some(bucket) = o.proportionality.take() {
                pb.insert(book, bucket);
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
        // against the text the caller supplied this call.
        out.extend(
            r.judge(&merged, &books, token_cache.as_ref(), Some(sites_ref))
                .into_iter()
                .filter(|f| target.contains_key(&f.sid)),
        );
        stats.insert(id, merged);
    }

    // Deterministic order, independent of the `parallel` feature (ADR 0018):
    // the parallel per-verse phase collects in nondeterministic order, so sort
    // by (sid, range start, rule) to make feature-on output byte-identical to
    // serial. Cheap against the analysis: one O(n log n) over the findings.
    out.sort_by_key(|f| (f.sid, f.range.start, f.code));

    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sid::BookId;

    fn map(pairs: &[(&str, &str)]) -> VerseMap {
        pairs
            .iter()
            .enumerate()
            .map(|(i, (_label, text))| {
                (
                    Sid::new(BookId::from_str("GEN").unwrap(), 1, (i + 1) as u16),
                    text.to_string(),
                )
            })
            .collect()
    }

    /// Verses 1.. of a named book.
    fn mk(book: &str, verses: &[&str]) -> VerseMap {
        verses
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    Sid::new(BookId::from_str(book).unwrap(), 1, (i + 1) as u16),
                    t.to_string(),
                )
            })
            .collect()
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

    fn mks(book: &str, verses: &[String]) -> VerseMap {
        verses
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    Sid::new(BookId::from_str(book).unwrap(), 1, (i + 1) as u16),
                    t.clone(),
                )
            })
            .collect()
    }

    /// Findings come back in a stable `(sid, range.start, code)` order
    /// regardless of the `parallel` feature (ADR 0018), and analysis is
    /// deterministic across runs. Under `--features parallel` this is the
    /// real guard that the end-of-pipeline sort tames rayon's nondeterministic
    /// collection order; under the serial default it pins the contract. The
    /// cross-*build* equality (feature-on findings == feature-off findings) is
    /// asserted in CI by running the suite under both feature sets.
    #[test]
    fn findings_are_sorted_and_deterministic() {
        // A multi-verse, multi-book corpus that trips several default rules.
        let mut target = mk("GEN", &["a  b", "x\ty", "p  q  r"]);
        target.extend(mk("EXO", &["m  n", "\u{200b}lead"]));

        let a = analyze(&target, None);
        let b = analyze(&target, None);
        assert_eq!(a, b, "analysis must be deterministic across runs");
        assert!(a.len() >= 5, "expected several findings, got {}", a.len());

        let keys: Vec<_> = a.iter().map(|f| (f.sid, f.range.start, f.code)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "findings must be in (sid, start, code) order");
    }

    #[test]
    fn cached_per_verse_lane_reuses_content_keyed_findings() {
        let mut target = mk("GEN", &["a  b", "hello"]);
        target.extend(mk("EXO", &["x\ty", "clean"]));
        let cfg = Config::v1_defaults();
        let mut cache = AnalysisCache::new();

        let (cold_findings, cold_stats) =
            analyze_stateful(&target, None, &cfg, None, None, Some(&mut cache));
        let misses_after_cold = cache.lane1_miss_count();
        let (warm_findings, warm_stats) =
            analyze_stateful(&target, None, &cfg, None, None, Some(&mut cache));

        assert_eq!(cold_findings, warm_findings);
        assert_eq!(cold_stats, warm_stats);
        assert_eq!(misses_after_cold, 2, "one lane-1 miss per book on cold call");
        assert_eq!(cache.lane1_hit_count(), 2, "warm call should hit both books");
    }

    #[test]
    fn cached_fingerprint_change_rewarms_both_lanes() {
        let mut target = mk("GEN", &["a  b", "hello"]);
        target.extend(mk("EXO", &["x\ty", "clean"]));
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();

        let (_, _) = analyze_stateful(&target, None, &cfg, None, None, Some(&mut cache));
        let mut changed_cfg = cfg.clone();
        changed_cfg.rules.insert(RuleId::BracketBalance, false);
        let (_, changed_prior) =
            analyze_stateful(&target, None, &changed_cfg, None, None, Some(&mut cache));

        // A config change clears the old products, so the first call under the
        // new fingerprint warms both books instead of reading either lane.
        assert_eq!(cache.lane1_hit_count(), 0);
        assert_eq!(cache.walk_hit_count(), 0);

        let gen_id = BookId::from_str("GEN").unwrap();
        let (_, _) = analyze_stateful(
            &target,
            None,
            &changed_cfg,
            Some(changed_prior),
            Some(&[gen_id]),
            Some(&mut cache),
        );
        assert_eq!(cache.lane1_hit_count(), 2);
        assert_eq!(cache.walk_hit_count(), 1, "the clean sibling reuses its re-warmed walk");
    }

    #[test]
    fn cached_snapshot_matches_cold_snapshot_across_all_walk_lanes() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let lev = BookId::from_str("LEV").unwrap();
        let mut original = VerseMap::new();
        original.insert(Sid::new(gen_id, 1, 1), "(He said. the gate stood.".into());
        original.insert(Sid::new(gen_id, 1, 2), "one) word word 12".into());
        original.insert(Sid::new(exo, 1, 1), "a  b, joyfullly".into());
        original.insert(Sid::new(exo, 1, 2), "A1 α qQx".into());
        original.insert(Sid::new(lev, 1, 1), "He said. The gate.".into());
        original.insert(Sid::new(lev, 1, 2), "clean text".into());

        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (cold_cached_findings, cold_cached_stats) =
            analyze_stateful(&original, None, &cfg, None, None, Some(&mut cache));
        let (cold_findings, cold_stats) =
            analyze_stateful(&original, None, &cfg, None, None, None);
        assert_eq!(cold_cached_findings, cold_findings);
        assert_eq!(cold_cached_stats, cold_stats);

        let mut edited = original.clone();
        edited.insert(Sid::new(exo, 1, 2), "A1 α qQx edited".into());
        let (scratch_findings, scratch_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(cold_stats.clone()),
            Some(&[exo]),
            None,
        );
        let (cached_findings, cached_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(cold_cached_stats),
            Some(&[exo]),
            Some(&mut cache),
        );

        assert_eq!(cached_findings, scratch_findings);
        assert_eq!(cached_stats, scratch_stats);
        assert_eq!(cache.walk_hit_count(), 2, "clean books should reuse walk products");
        assert_eq!(cache.walk_miss_count(), 0, "changed book is walked without a cache probe");
    }

    #[test]
    fn cached_content_invalidation_replaces_one_book_and_keeps_sibling_warm() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let mut original = mk("GEN", &["a  b", "one"]);
        original.extend(mk("EXO", &["x\ty", "two"]));
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None, Some(&mut cache));
        let old_gen_hash = cache.entry_hash(gen_id).unwrap();
        let old_exo_hash = cache.entry_hash(exo).unwrap();

        let mut edited = original.clone();
        edited.insert(Sid::new(gen_id, 1, 1), "changed ,, text".into());
        let (cold_findings, cold_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior.clone()),
            Some(&[gen_id]),
            None,
        );
        let (cached_findings, cached_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior),
            Some(&[gen_id]),
            Some(&mut cache),
        );

        assert_eq!(cached_findings, cold_findings);
        assert_eq!(cached_stats, cold_stats);
        assert_ne!(cache.entry_hash(gen_id), Some(old_gen_hash));
        assert_eq!(cache.entry_hash(exo), Some(old_exo_hash));
        assert_eq!(cache.walk_hit_count(), 1, "only the unchanged sibling reuses its walk");
    }

    #[test]
    fn structurally_changed_book_with_identical_text_recounts() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let original: VerseMap = [
            (Sid::new(gen_id, 1, 1), "same".into()),
            (Sid::new(gen_id, 1, 2), "text".into()),
        ]
        .into_iter()
        .collect();
        let restructured: VerseMap = [
            (Sid::new(gen_id, 1, 1), "same".into()),
            (Sid::new(gen_id, 2, 1), "text".into()),
        ]
        .into_iter()
        .collect();
        let cfg = Config::v1_defaults();
        let mut cache = AnalysisCache::new();
        let (_, _) = analyze_stateful(&original, None, &cfg, None, None, Some(&mut cache));
        let old_hash = cache.entry_hash(gen_id).unwrap();

        let (_, _) = analyze_stateful(&restructured, None, &cfg, None, None, Some(&mut cache));

        assert_eq!(cache.lane1_hit_count(), 0, "address changes invalidate the book hash");
        assert_eq!(cache.lane1_miss_count(), 2);
        assert_ne!(cache.entry_hash(gen_id), Some(old_hash));
    }

    #[test]
    fn clean_book_hash_mismatch_forces_a_walk_even_when_not_named_changed() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let mut original = mk("GEN", &["one", "two"]);
        original.extend(mk("EXO", &["three", "four"]));
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None, Some(&mut cache));
        let old_exo_hash = cache.entry_hash(exo).unwrap();

        let mut edited = original.clone();
        edited.insert(Sid::new(exo, 1, 1), "changed text".into());
        // This deliberately lies about the edit. The content hash must still
        // prevent a stale clean-book walk product from being reused.
        let (cold_findings, cold_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior.clone()),
            Some(&[gen_id]),
            None,
        );
        let (cached_findings, cached_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior),
            Some(&[gen_id]),
            Some(&mut cache),
        );

        assert_eq!(cached_findings, cold_findings);
        assert_eq!(cached_stats, cold_stats);
        assert_eq!(cache.walk_hit_count(), 0, "the hash-mismatched book must be walked");
        assert_ne!(cache.entry_hash(exo), Some(old_exo_hash));
    }

    #[test]
    fn cached_empty_and_prior_none_calls_reuse_caseless_sentinel() {
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let empty = VerseMap::new();
        let (_, _) = analyze_stateful(&empty, None, &cfg, None, None, Some(&mut cache));
        assert_eq!(cache.book_count(), 0);

        let gen_id = BookId::from_str("GEN").unwrap();
        let caseless = mk("GEN", &["你好"]);
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, None, Some(&mut cache));
        let (_, _) = analyze_stateful(&caseless, None, &cfg, None, None, Some(&mut cache));
        assert_eq!(cache.lane1_hit_count(), 1, "prior-none calls still reuse pure findings");
        assert!(cache
            .books
            .get(&gen_id)
            .and_then(|entry| entry.casing.as_ref())
            .is_some_and(|sites| sites.sites.is_empty()));

        let mut full = caseless.clone();
        full.extend(mk("EXO", &["a  b"]));
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, None, Some(&mut cache));
        let exo = BookId::from_str("EXO").unwrap();
        let mut edited = full.clone();
        edited.insert(Sid::new(exo, 1, 1), "edited".into());
        let walk_hits_before = cache.walk_hit_count();
        let (_, _) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior),
            Some(&[exo]),
            Some(&mut cache),
        );
        assert_eq!(cache.walk_hit_count(), walk_hits_before + 1);
    }

    #[test]
    fn cached_snapshot_never_reads_default_stats_for_clean_books() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let mut original = mk("GEN", &["(He said. the gate stood.", "one) word word 12"]);
        original.extend(mk("EXO", &["a  b, joyfullly", "A1 α qQx"]));
        let cfg = Config::all();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None, Some(&mut cache));
        assert_ne!(prior, Stats::default(), "the clean sibling must carry real prior stats");

        let mut edited = original.clone();
        edited.insert(Sid::new(gen_id, 1, 1), "changed text".into());
        let (_, cold_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior.clone()),
            Some(&[gen_id]),
            None,
        );
        let (_, cached_stats) = analyze_stateful(
            &edited,
            None,
            &cfg,
            Some(prior),
            Some(&[gen_id]),
            Some(&mut cache),
        );
        assert_eq!(cached_stats, cold_stats);
    }

    #[test]
    fn echo_subset_keeps_sibling_cache_entries_and_matches_cold_echo() {
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();
        let mut full = mk("GEN", &["a  b", "one"]);
        full.extend(mk("EXO", &["x\ty", "two"]));
        let cfg = Config::v1_defaults();
        let mut cache = AnalysisCache::new();
        let (_, prior) = analyze_stateful(&full, None, &cfg, None, None, Some(&mut cache));
        let gen_hash = cache.entry_hash(gen_id).unwrap();
        let exo_hash = cache.entry_hash(exo).unwrap();
        let echo: VerseMap = full
            .iter()
            .filter(|(sid, _)| sid.book == exo)
            .map(|(sid, text)| (*sid, text.clone()))
            .collect();

        let (cached_findings, cached_stats) =
            analyze_stateful(&echo, None, &cfg, Some(prior.clone()), None, Some(&mut cache));
        let (cold_findings, cold_stats) =
            analyze_stateful(&echo, None, &cfg, Some(prior), None, None);

        assert_eq!(cached_findings, cold_findings);
        assert_eq!(cached_stats, cold_stats);
        assert_eq!(cache.entry_hash(gen_id), Some(gen_hash));
        assert_eq!(cache.entry_hash(exo), Some(exo_hash));
    }

    #[test]
    fn analyze_flags_double_space() {
        let target = map(&[("v1", "a  b")]);
        let findings = analyze(&target, None);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, signals::whitespace::EXCESS_H_WHITESPACE);
        // The range slices the offending run out of that verse's text.
        let text = target.values().next().unwrap();
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
        let target = map(&[("v1", &format!("{}joyfullly", "word ".repeat(50_000)))]);
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
        let mut target = VerseMap::new();
        let mut source = VerseMap::new();
        for v in 1..=60u16 {
            let sid = Sid::new(BookId::from_str("GEN").unwrap(), 1, v);
            let base = "word ".repeat(8 + (v as usize % 3));
            source.insert(sid, base.clone());
            // Mild target-side jitter keeps the book's MAD nonzero.
            let jittered = format!("{base}{}", "x".repeat(v as usize % 5));
            target.insert(sid, if v == 7 { base.repeat(4) } else { jittered });
        }

        let findings = analyze(&target, Some(&source));
        let prop: Vec<_> = findings
            .iter()
            .filter(|f| f.code == RuleId::ProjectLengthRatio)
            .collect();
        assert_eq!(prop.len(), 1);
        assert_eq!(prop[0].sid.verse, 7);
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

        let (f1, stats) = analyze_stateful(&target, None, &cfg, None, None, None);
        assert!(
            f1.iter()
                .any(|f| f.code == RuleId::SentenceInitialLowercase)
        );

        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();

        let (f2, _) = analyze_stateful(&target, None, &cfg, Some(back), None, None);
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
        let mut full = mks("GEN", &anomalous);
        full.extend(mks("EXO", &anomalous));
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();

        let (f_full, stats) = analyze_stateful(&full, None, &cfg, None, None, None);
        assert!(
            f_full
                .iter()
                .any(|f| f.sid.book == gen_id && f.code == RuleId::SentenceInitialLowercase)
        );
        assert!(
            f_full
                .iter()
                .any(|f| f.sid.book == exo && f.code == RuleId::SentenceInitialLowercase)
        );

        let (f_inc, _) = analyze_stateful(&mks("EXO", &anomalous), None, &cfg, Some(stats), None, None);
        assert!(!f_inc.is_empty());
        assert!(f_inc.iter().all(|f| f.sid.book == exo)); // nothing from GEN
    }

    /// The `changed` reduce scope (ADR 0043) is exactly a performance hint:
    /// a whole-corpus call naming only the edited book must produce findings
    /// AND stats identical to a from-scratch recompute of the edited corpus —
    /// including findings that *moved in the untouched book* because the edit
    /// tipped a pooled convention (the copy-paste-a-new-Genesis case).
    #[test]
    fn changed_scope_matches_full_recompute() {
        let cfg = casing_on(0.5, 0.0);
        // Clean establishing verses (a capital-after-`.` habit, no anomaly).
        // ≥ 30 so the `.` boundary class clears ADR 0052's event floor.
        let clean: Vec<String> = (0..40)
            .map(|_| "The men saw the gate.".to_string())
            .collect();
        let mut original = mks("GEN", &clean);
        original.extend(mks("EXO", &clean));
        let (_, prior) = analyze_stateful(&original, None, &cfg, None, None, None);

        // Edit GEN only: introduce a lowercase-after-terminal anomaly.
        let mut edited = mks("GEN", &casing_fire(40));
        edited.extend(mks("EXO", &clean));

        let (f_scratch, s_scratch) = analyze_stateful(&edited, None, &cfg, None, None, None);
        let gen_id = BookId::from_str("GEN").unwrap();
        let (f_changed, s_changed) =
            analyze_stateful(&edited, None, &cfg, Some(prior.clone()), Some(&[gen_id]), None);
        assert_eq!(f_scratch, f_changed);
        assert_eq!(s_scratch, s_changed);

        // Without a prior, `changed` is ignored (nothing to carry): still a
        // full recompute, never a tiny-counts corpus.
        let (f_no_prior, s_no_prior) = analyze_stateful(&edited, None, &cfg, None, Some(&[gen_id]), None);
        assert_eq!(f_scratch, f_no_prior);
        assert_eq!(s_scratch, s_no_prior);
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
        let gen_map = mks("GEN", &gen_clean);
        // EXO alone holds one lowercase "the" after a period — but with no
        // mid-flow "the" of its own it is unclassifiable without GEN's lexicon.
        let exo_anom = ["He fell. the gate stood.".to_string()];
        let mut full = gen_map.clone();
        full.extend(mks("EXO", &exo_anom));
        let exo = BookId::from_str("EXO").unwrap();

        let (f_full, mut stats) = analyze_stateful(&full, None, &cfg, None, None, None);
        assert!(
            f_full
                .iter()
                .any(|f| f.sid.book == exo && f.code == RuleId::SentenceInitialLowercase),
            "EXO's `the` fires while GEN backs the lexicon + habit"
        );

        stats.remove_book(BookId::from_str("GEN").unwrap());
        let (f_after, _) = analyze_stateful(&mks("EXO", &exo_anom), None, &cfg, Some(stats), None, None);
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
        let gen_id = BookId::from_str("GEN").unwrap();
        let full: VerseMap = [
            (Sid::new(gen_id, 1, 1), format!("word{ZW}{ZW}next")), // doubled run → redundant
            (Sid::new(gen_id, 1, 2), format!("word {ZW}next")), // single, space-adjacent → NOT flagged
            (Sid::new(gen_id, 1, 3), format!("ក{ZW}ក")),        // Khmer word break → silent
        ]
        .into_iter()
        .collect();

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
        assert_eq!(hits[0].sid.verse, 1);
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
}
