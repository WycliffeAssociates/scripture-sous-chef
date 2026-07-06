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

pub mod charclass;
mod charclass_table;
pub mod config;
pub mod diagnostics;
pub mod grapheme;
pub mod rule;
pub mod script;
mod evidence;
pub mod sid;
pub mod signals;
pub mod span;
pub mod stats;
pub mod token;
pub mod unicode;
pub mod verse;

pub use config::{
    BracketBalanceConfig, CasingConfig, Config, ProportionalityConfig, PunctOnlyTokenConfig,
    PunctuationAdjacencyConfig, PunctuationSpacingConfig, RepeatedCharacterRunConfig,
};
pub use diagnostics::{Finding, FindingArgs, LengthRatioScope, RuleId, Severity};
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

/// Tokenize every verse once, keyed by `Sid`, so token-consuming rules share
/// a single UAX #29 word scan instead of each repeating it.
fn build_token_cache(target: &VerseMap) -> rule::TokenCache {
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        target
            .par_iter()
            .map(|(&sid, text)| (sid, crate::token::tokenize(text)))
            .collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        target
            .iter()
            .map(|(&sid, text)| (sid, crate::token::tokenize(text)))
            .collect()
    }
}

/// All findings for one verse from the per-verse and token rules. Token rules
/// read the shared [`rule::TokenCache`] when present (else tokenize inline).
fn verse_findings(
    sid: Sid,
    text: &str,
    per_verse: &[Box<dyn rule::PerVerseRule>],
    token_rules: &[Box<dyn rule::TokenRule>],
    token_cache: &Option<rule::TokenCache>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in per_verse {
        let (code, severity) = (r.id(), r.severity());
        for range in r.check(text) {
            out.push(Finding { sid, code, severity, range, score: None, args: None });
        }
    }
    if !token_rules.is_empty() {
        let owned;
        let tokens: &[crate::token::Token] = match token_cache {
            Some(c) => c.get(&sid).map(Vec::as_slice).unwrap_or(&[]),
            None => {
                owned = crate::token::tokenize(text);
                &owned
            }
        };
        for r in token_rules {
            let (code, severity) = (r.id(), r.severity());
            for range in r.check(text, tokens) {
                out.push(Finding { sid, code, severity, range, score: None, args: None });
            }
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
    analyze_stateful(target, source, config, None).0
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
pub fn analyze_stateful(
    target: &VerseMap,
    source: Option<&VerseMap>,
    config: &Config,
    prior: Option<Stats>,
) -> (Vec<Finding>, Stats) {
    let per_verse: Vec<_> = rule::per_verse_rules()
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    let token_rules: Vec<_> = rule::token_rules()
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    let project: Vec<_> = rule::project_rules(config)
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    let project_token: Vec<_> = rule::project_token_rules()
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

    // Tokenize the corpus once and share it whenever ≥2 rules would otherwise
    // each re-tokenize every verse — the UAX #29 word scan is a top cost on
    // space-free / non-Latin scripts. With 0–1 token consumers the lone rule
    // tokenizes inline, so default configs pay no cache overhead.
    let token_cache: Option<rule::TokenCache> =
        (token_rules.len() + project_token.len() >= 2).then(|| build_token_cache(target));

    // The per-verse phase is embarrassingly parallel — each verse is judged
    // from its own text by `Sync` rules. Under the `parallel` feature it fans
    // out over rayon (ADR 0018); otherwise it stays serial. Output is the same
    // either way: `out` is sorted before return, so order never depends on the
    // feature.
    #[cfg(feature = "parallel")]
    let mut out: Vec<Finding> = {
        use rayon::prelude::*;
        target
            .par_iter()
            .map(|(&sid, text)| verse_findings(sid, text, &per_verse, &token_rules, &token_cache))
            .flatten_iter()
            .collect()
    };
    #[cfg(not(feature = "parallel"))]
    let mut out: Vec<Finding> = {
        let mut out = Vec::new();
        for (&sid, text) in target {
            out.extend(verse_findings(sid, text, &per_verse, &token_rules, &token_cache));
        }
        out
    };

    for r in &project {
        out.extend(r.check(target, source));
    }
    for r in &project_token {
        out.extend(r.check(target, source, token_cache.as_ref()));
    }

    // Stateful rules: reduce this call's verses, supersede the prior cache at
    // book granularity, judge the whole merged corpus from the cache.
    let mut stats = prior.unwrap_or_default();
    for r in &stateful {
        let fresh = r.reduce(target, source);
        let merged = match stats.take(r.id()) {
            Some(prev) => prev.merge(fresh),
            None => fresh,
        };
        // Judge against the whole merged corpus, but emit only for `target`
        // — keeping the returned findings to one scope and projectable
        // against the text the caller supplied this call.
        out.extend(
            r.judge(&merged, target)
                .into_iter()
                .filter(|f| target.contains_key(&f.sid)),
        );
        stats.insert(r.id(), merged);
    }

    // Deterministic order, independent of the `parallel` feature (ADR 0018):
    // the parallel per-verse phase collects in nondeterministic order, so sort
    // by (sid, range start, rule) to make feature-on output byte-identical to
    // serial. Cheap against the analysis: one O(n log n) over the findings.
    out.sort_by(|a, b| {
        (a.sid, a.range.start, a.code).cmp(&(b.sid, b.range.start, b.code))
    });

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
            confidence_z,
        };
        cfg
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
        let target = map(&[(
            "v1",
            &format!("{}joyfullly", "word ".repeat(50_000)),
        )]);
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
        assert!(matches!(prop[0].args, Some(FindingArgs::LengthRatio { .. })));

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

        // Casing is corpus-observed (ADR 0017): default-off, and once opted
        // in it fires only where enough observations establish a
        // high-precision boundary — never from a lone verse.
        let casing = map(&[
            ("v1", "He spoke. Then he left."),
            ("v2", "He spoke. Then he left."),
            ("v3", "He spoke. then he left."),
        ]);
        assert!(analyze(&casing, None).is_empty());
        let mut on = Config::v1_defaults();
        on.rules.insert(RuleId::SentenceInitialLowercase, true);
        on.casing = CasingConfig {
            emit_score_min: 0.5,
            confidence_z: 0.0,
        };
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
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::SentenceInitialLowercase, true);
        cfg.casing = CasingConfig {
            emit_score_min: 0.5,
            confidence_z: 0.0,
        };

        let mut pairs: Vec<(&str, &str)> = (0..10).map(|_| ("v", "He spoke. Then he left.")).collect();
        pairs.push(("v", "He spoke. then he left."));
        let target = map(&pairs);

        let (f1, stats) = analyze_stateful(&target, None, &cfg, None);
        assert!(f1.iter().any(|f| f.code == RuleId::SentenceInitialLowercase));

        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();

        let (f2, _) = analyze_stateful(&target, None, &cfg, Some(back));
        assert_eq!(f1, f2);
    }

    /// All returned findings cover exactly `target` (ADR 0017). An
    /// incremental call for one book never returns another book's findings —
    /// the wasm boundary can then always project them (no out-of-bounds slice
    /// against an empty/absent verse).
    #[test]
    fn incremental_findings_are_scoped_to_target() {
        let cfg = casing_on(0.5, 0.0);
        let anomalous = ["He spoke. Then he left.", "He spoke. Then he left.", "He spoke. then he left."];
        let mut full = mk("GEN", &anomalous);
        full.extend(mk("EXO", &anomalous));
        let gen_id = BookId::from_str("GEN").unwrap();
        let exo = BookId::from_str("EXO").unwrap();

        let (f_full, stats) = analyze_stateful(&full, None, &cfg, None);
        assert!(f_full.iter().any(|f| f.sid.book == gen_id && f.code == RuleId::SentenceInitialLowercase));
        assert!(f_full.iter().any(|f| f.sid.book == exo && f.code == RuleId::SentenceInitialLowercase));

        let (f_inc, _) = analyze_stateful(&mk("EXO", &anomalous), None, &cfg, Some(stats));
        assert!(!f_inc.is_empty());
        assert!(f_inc.iter().all(|f| f.sid.book == exo)); // nothing from GEN
    }

    /// `Stats::remove_book` drops a book's contribution to the corpus
    /// aggregate, not just its findings: here EXO's anomaly clears the
    /// dominance floor only while GEN's observations back it, so removing
    /// GEN silences it.
    #[test]
    fn remove_book_drops_contribution_to_corpus_stats() {
        let cfg = casing_on(0.7, 1.0);
        let gen_map = mk("GEN", &["He spoke. Then he left.", "He spoke. Then he left.", "He spoke. Then he left.", "He spoke. Then he left."]);
        let exo_anom = ["He spoke. Then.", "He spoke. then."];
        let mut full = gen_map.clone();
        full.extend(mk("EXO", &exo_anom));
        let exo = BookId::from_str("EXO").unwrap();

        let (f_full, mut stats) = analyze_stateful(&full, None, &cfg, None);
        assert!(f_full.iter().any(|f| f.sid.book == exo)); // fires on combined samples

        stats.remove_book(BookId::from_str("GEN").unwrap());
        let (f_after, _) = analyze_stateful(&mk("EXO", &exo_anom), None, &cfg, Some(stats));
        // EXO's own few observations can't back a confident dominance now.
        assert!(f_after.iter().all(|f| f.code != RuleId::SentenceInitialLowercase));
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
            (Sid::new(gen_id, 1, 2), format!("word {ZW}next")),    // single, space-adjacent → NOT flagged
            (Sid::new(gen_id, 1, 3), format!("ក{ZW}ក")),          // Khmer word break → silent
        ]
        .into_iter()
        .collect();

        let f = analyze(&full, None);
        let hits: Vec<_> = f.iter().filter(|f| f.code == RuleId::RedundantZeroWidthSpace).collect();
        assert_eq!(hits.len(), 1, "only the doubled run surfaces; single ZWSP (even space-adjacent) does not");
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
        let tk = rule::token_rules();
        let pr = rule::project_rules(&cfg);
        let pt = rule::project_token_rules();
        let sf = rule::stateful_rules(&cfg);
        let mut seen: BTreeMap<RuleId, u32> = BTreeMap::new();
        for id in pv
            .iter()
            .map(|r| r.id())
            .chain(tk.iter().map(|r| r.id()))
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
        assert_eq!(seen.len(), RuleId::ALL.len(), "a registry emitted an unknown id");
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
