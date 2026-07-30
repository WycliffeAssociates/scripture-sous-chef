//! Compact Review Depth calibration survey.
//!
//! This is measurement infrastructure, not runtime policy. Each row records
//! the exact profile anchor, the candidate opportunity population, and the
//! findings produced by that native config. The calibration note can therefore
//! recompute every chosen Rust constant from the TSV without retaining the
//! full finding corpus in memory.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use rayon::prelude::*;

use ssc_core::config::Config;
use ssc_core::diagnostics::{Finding, RuleId};
use ssc_core::review_depth::ReviewDepth;
use ssc_core::signals::{casing, punctuation};
use ssc_core::{analyze_with_config, Corpus};

use crate::oracle::{load_corpora, OracleScope};

const DEPTHS: [u8; 5] = [0, 25, 50, 75, 100];

fn quantile(scores: &mut [f32], p: f32) -> Option<f32> {
    if scores.is_empty() {
        return None;
    }
    scores.sort_by(f32::total_cmp);
    let rank = (p * scores.len() as f32).ceil() as usize;
    Some(scores[rank.saturating_sub(1).min(scores.len() - 1)])
}

fn score_summary(findings: &[Finding], rule: RuleId) -> (usize, String, String) {
    let mut scores: Vec<f32> = findings
        .iter()
        .filter(|f| f.code == rule)
        .filter_map(|f| f.score)
        .collect();
    let count = scores.len();
    let median = quantile(&mut scores, 0.5)
        .map(|v| format!("{v:.6}"))
        .unwrap_or_else(|| "-".into());
    let p90 = quantile(&mut scores, 0.9)
        .map(|v| format!("{v:.6}"))
        .unwrap_or_else(|| "-".into());
    (count, median, p90)
}

fn analyze_rule(corpus: &Corpus, rule: RuleId, config: Config) -> Vec<Finding> {
    let mut config = config;
    // The survey measures one pilot at a time. Disable every other rule so
    // its rows describe the pilot's judge rather than paying for unrelated
    // production findings on each grid cell.
    config.rules = RuleId::ALL
        .iter()
        .map(|&candidate| (candidate, candidate == rule))
        .collect();
    let findings = analyze_with_config(corpus, None, &config);
    findings.into_iter().filter(|f| f.code == rule).collect()
}

fn rows_for_corpus(corpus_id: &str, corpus: &Corpus) -> String {
    let mut rows = String::new();
    for raw_depth in DEPTHS {
        let depth = ReviewDepth::new(raw_depth).unwrap();

        let spacing = punctuation::config_at_review_depth(depth);
        let mut spacing_opportunity = spacing;
        spacing_opportunity.emit_score_min = 0.0;
        let opportunities = punctuation::spacing_findings(corpus, &spacing_opportunity).len();
        let mut spacing_config = Config::v1_defaults();
        spacing_config
            .rules
            .insert(RuleId::PunctuationSpacingAnomaly, true);
        spacing_config.punctuation_spacing = spacing;
        let spacing_findings =
            analyze_rule(corpus, RuleId::PunctuationSpacingAnomaly, spacing_config);
        let (findings, median, p90) =
            score_summary(&spacing_findings, RuleId::PunctuationSpacingAnomaly);
        writeln!(
            rows,
            "punct.spacing-anomaly\t{raw_depth}\t{corpus_id}\t{opportunities}\t{findings}\t{median}\t{p90}\t{:.6}\t{:.6}\t{:.6}\t-\t{:.6}",
            spacing.emit_score_min,
            spacing.confidence_z,
            spacing.minority_recurrence_k,
            spacing.minority_rate_per_10k,
        )
        .unwrap();

        let positional = casing::sentence_initial_config_at_review_depth(depth);
        let mut positional_config = Config::v1_defaults();
        positional_config
            .rules
            .insert(RuleId::SentenceInitialLowercase, true);
        positional_config.casing.sentence_initial = positional;
        let mut positional_eval = ssc_core::CasingConfig::default();
        positional_eval.sentence_initial = positional;
        let opportunities = casing::evaluate(corpus, &positional_eval)
            .into_iter()
            .filter(|site| site.positional.is_some())
            .count();
        let positional_findings =
            analyze_rule(corpus, RuleId::SentenceInitialLowercase, positional_config);
        let (findings, median, p90) =
            score_summary(&positional_findings, RuleId::SentenceInitialLowercase);
        writeln!(
            rows,
            "case.sentence-initial-lowercase\t{raw_depth}\t{corpus_id}\t{opportunities}\t{findings}\t{median}\t{p90}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t-",
            positional.evidence.emit_score_min,
            positional.evidence.confidence_z,
            positional.evidence.recurrence_k,
            positional.trust_gate,
        )
        .unwrap();

        let intrinsic = casing::inconsistent_word_config_at_review_depth(depth);
        let mut intrinsic_config = Config::v1_defaults();
        intrinsic_config
            .rules
            .insert(RuleId::InconsistentWordCasing, true);
        intrinsic_config.casing.inconsistent_word = intrinsic;
        let mut intrinsic_eval = ssc_core::CasingConfig::default();
        intrinsic_eval.inconsistent_word = intrinsic;
        let opportunities = casing::evaluate(corpus, &intrinsic_eval)
            .into_iter()
            .filter(|site| site.intrinsic.is_some())
            .count();
        let intrinsic_findings =
            analyze_rule(corpus, RuleId::InconsistentWordCasing, intrinsic_config);
        let (findings, median, p90) =
            score_summary(&intrinsic_findings, RuleId::InconsistentWordCasing);
        writeln!(
            rows,
            "case.inconsistent-word-casing\t{raw_depth}\t{corpus_id}\t{opportunities}\t{findings}\t{median}\t{p90}\t{:.6}\t{:.6}\t{:.6}\t-\t-",
            intrinsic.evidence.emit_score_min,
            intrinsic.evidence.confidence_z,
            intrinsic.evidence.recurrence_k,
        )
        .unwrap();
    }
    rows
}

/// Run the two pilot profiles over a small/WA/full corpus set.
pub(crate) fn review_depth_survey(path: &Path, out_path: &Path, tier: &str) {
    let scope = match tier {
        "wa" => OracleScope::Wa,
        "full" | "small" => OracleScope::Full,
        other => panic!("unknown Review Depth survey tier {other:?} (want small|wa|full)"),
    };
    let corpora = load_corpora(path, scope);
    if tier == "small" && corpora.len() > 20 {
        panic!(
            "small Review Depth survey input must be a small blob, got {} corpora",
            corpora.len()
        );
    }
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(out_path)
            .unwrap_or_else(|e| panic!("create {}: {e}", out_path.display())),
    );
    writeln!(
        out,
        "# review-depth-survey-v1\ttier={tier}\tcorpora={}",
        corpora.len()
    )
    .unwrap();
    writeln!(
        out,
        "rule\tdepth\tcorpus\topportunities\tfindings\tmedian_score\tp90_score\temit_score_min\tconfidence_z\trecurrence_k\ttrust_gate\tminority_rate_per_10k"
    )
    .unwrap();

    let rows: Vec<String> = corpora
        .par_iter()
        .map(|(corpus_id, corpus)| rows_for_corpus(corpus_id, corpus))
        .collect();
    for rows in rows {
        out.write_all(rows.as_bytes()).unwrap();
    }
    eprintln!(
        "review-depth survey: wrote {} corpora × {} depths × 3 rules to {}",
        if tier == "small" { "small" } else { "selected" },
        DEPTHS.len(),
        out_path.display()
    );
}
