//! `ssc-core` — a pure, addressable content analyzer.
//!
//! sous receives verse *text* (onion's lossless vref projection) and
//! returns *ranges into that text*. It reads no files, calls no onion,
//! and runs no segmentation of its own — onion is the single segmenter
//! of record. Mapping a returned range back to a DOM node or source
//! offset is the orchestrator's job, via onion's `segments`. See
//! ADR 0010 and `documentation/v1-reset-design.md`.

pub mod config;
pub mod diagnostics;
pub mod rule;
pub mod script;
pub mod sid;
pub mod signals;
pub mod span;
pub mod stats;
pub mod token;
pub mod unicode;
pub mod verse;

pub use config::{BracketBalanceConfig, CasingConfig, Config, ProportionalityConfig};
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
    let mut out = Vec::new();

    let per_verse: Vec<_> = rule::per_verse_rules()
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    for (sid, text) in target {
        for r in &per_verse {
            let code = r.id();
            let severity = r.severity();
            for range in r.check(text) {
                out.push(Finding {
                    sid: *sid,
                    code,
                    severity,
                    range,
                    score: None,
                    args: None,
                });
            }
        }
    }

    for r in rule::project_rules(config) {
        if !config.is_enabled(r.id()) {
            continue;
        }
        out.extend(r.check(target, source));
    }

    // Stateful rules: reduce this call's verses, supersede the prior cache at
    // book granularity, judge the whole merged corpus from the cache.
    let mut stats = prior.unwrap_or_default();
    for r in rule::stateful_rules(config) {
        if !config.is_enabled(r.id()) {
            continue;
        }
        let fresh = r.reduce(target, source);
        let merged = match stats.take(r.id()) {
            Some(prev) => prev.merge(fresh),
            None => fresh,
        };
        // Judge against the whole merged corpus, but emit only for `target`
        // — keeping the returned findings to one scope and projectable
        // against the text the caller supplied this call.
        out.extend(
            r.judge(&merged)
                .into_iter()
                .filter(|f| target.contains_key(&f.sid)),
        );
        stats.insert(r.id(), merged);
    }

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

    fn casing_on(threshold: f32, min_samples: u32) -> Config {
        let mut cfg = Config::v1_defaults();
        cfg.rules.insert(RuleId::SentenceInitialLowercase, true);
        cfg.casing = CasingConfig {
            threshold,
            min_samples,
        };
        cfg
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

    /// The shipped defaults keep convention-dependent (P2) rules off;
    /// an explicit config entry opts in.
    #[test]
    fn p2_rules_are_default_disabled_and_opt_in() {
        let target = map(&[("v1", "word ,word")]);
        assert!(analyze(&target, None).is_empty());

        let mut on = Config::v1_defaults();
        on.rules.insert(RuleId::SpaceBeforePunct, true);
        let findings = analyze_with_config(&target, None, &on);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, RuleId::SpaceBeforePunct);

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
            threshold: 0.5,
            min_samples: 1,
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
            threshold: 0.5,
            min_samples: 1,
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
        let cfg = casing_on(0.5, 1);
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
    /// aggregate, not just its findings: here EXO's anomaly only clears
    /// `min_samples` while GEN is cached, so removing GEN silences it.
    #[test]
    fn remove_book_drops_contribution_to_corpus_stats() {
        let cfg = casing_on(0.5, 5);
        let gen_map = mk("GEN", &["He spoke. Then he left.", "He spoke. Then he left.", "He spoke. Then he left.", "He spoke. Then he left."]);
        let exo_anom = ["He spoke. Then.", "He spoke. then."];
        let mut full = gen_map.clone();
        full.extend(mk("EXO", &exo_anom));
        let exo = BookId::from_str("EXO").unwrap();

        let (f_full, mut stats) = analyze_stateful(&full, None, &cfg, None);
        assert!(f_full.iter().any(|f| f.sid.book == exo)); // fires on combined samples

        stats.remove_book(BookId::from_str("GEN").unwrap());
        let (f_after, _) = analyze_stateful(&mk("EXO", &exo_anom), None, &cfg, Some(stats));
        // EXO's own samples are below min_samples now, so it no longer fires.
        assert!(f_after.iter().all(|f| f.code != RuleId::SentenceInitialLowercase));
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
