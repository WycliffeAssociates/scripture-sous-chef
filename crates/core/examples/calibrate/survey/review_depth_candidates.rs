//! Non-circular Review Depth calibration.
//!
//! This survey intentionally does not call the committed Review Depth profile
//! functions. It sweeps independent unusualness/support candidates, compares
//! adjacent cells, and repeats a deterministic maturity ladder on a bounded
//! script-diverse sample. The output is evidence for anchor selection, not an
//! approval of the current production tables.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use rayon::prelude::*;

use ssc_core::config::Config;
use ssc_core::diagnostics::{Finding, RuleId};
use ssc_core::key::parse_key;
use ssc_core::{Corpus, analyze_with_config};

use crate::oracle::{OracleScope, load_corpora};

const MATURITIES: [(&str, usize); 5] = [
    ("1", 1),
    ("5", 5),
    ("28", 28),
    ("120", 120),
    ("full", usize::MAX),
];
const MATURITY_SAMPLE: usize = 28;

#[derive(Clone, Copy)]
struct Support {
    label: &'static str,
    confidence_z: f32,
    recurrence_k: f32,
    rate_per_10k: Option<f32>,
    trust_gate: Option<f32>,
}

const SUPPORTS: [Support; 3] = [
    Support {
        label: "strict-support",
        confidence_z: 2.58,
        recurrence_k: 16.0,
        rate_per_10k: Some(20.0),
        trust_gate: Some(0.95),
    },
    Support {
        label: "mid-support",
        confidence_z: 1.96,
        recurrence_k: 32.0,
        rate_per_10k: Some(40.0),
        trust_gate: Some(0.90),
    },
    Support {
        label: "broad-support",
        confidence_z: 1.28,
        recurrence_k: 64.0,
        rate_per_10k: Some(65.0),
        trust_gate: Some(0.75),
    },
];

#[derive(Clone, Copy)]
struct Candidate {
    unusualness: f32,
    support: Support,
}

#[derive(Clone)]
struct MaturityView {
    maturity: &'static str,
    order: &'static str,
    corpus: Corpus,
}

#[derive(Clone)]
struct Snapshot {
    findings: usize,
    opportunities: usize,
    median: Option<f32>,
    p75: Option<f32>,
    p90: Option<f32>,
    p99: Option<f32>,
    ids: BTreeSet<String>,
    values: BTreeMap<String, String>,
    examples: Vec<String>,
}

#[derive(Clone)]
struct CellSummary {
    rule: RuleId,
    unusualness: u32,
    support: &'static str,
    maturity: &'static str,
    order: &'static str,
    opportunities: usize,
    findings: usize,
}

struct CorpusReport {
    text: String,
    cells: Vec<CellSummary>,
    max_row_bytes: usize,
}

fn candidates(rule: RuleId) -> Vec<Candidate> {
    let unusualness = match rule {
        RuleId::PunctuationSpacingAnomaly => [0.30, 0.50, 0.80],
        RuleId::SentenceInitialLowercase | RuleId::InconsistentWordCasing => [0.80, 0.95, 0.99],
        _ => unreachable!("candidate survey only covers mapped pilots"),
    };
    unusualness
        .into_iter()
        .flat_map(|floor| {
            SUPPORTS.map(|support| Candidate {
                unusualness: floor,
                support,
            })
        })
        .collect()
}

fn config_for(rule: RuleId, candidate: Candidate) -> Config {
    let mut config = Config::v1_defaults();
    config.rules.insert(rule, true);
    match rule {
        RuleId::PunctuationSpacingAnomaly => {
            config.punctuation_spacing.emit_score_min = candidate.unusualness;
            config.punctuation_spacing.confidence_z = candidate.support.confidence_z;
            config.punctuation_spacing.minority_recurrence_k = candidate.support.recurrence_k;
            config.punctuation_spacing.minority_rate_per_10k =
                candidate.support.rate_per_10k.unwrap();
        }
        RuleId::SentenceInitialLowercase => {
            config.casing.sentence_initial.evidence.emit_score_min = candidate.unusualness;
            config.casing.sentence_initial.evidence.confidence_z = candidate.support.confidence_z;
            config.casing.sentence_initial.evidence.recurrence_k = candidate.support.recurrence_k;
            config.casing.sentence_initial.trust_gate = candidate.support.trust_gate.unwrap();
        }
        RuleId::InconsistentWordCasing => {
            config.casing.inconsistent_word.evidence.emit_score_min = candidate.unusualness;
            config.casing.inconsistent_word.evidence.confidence_z = candidate.support.confidence_z;
            config.casing.inconsistent_word.evidence.recurrence_k = candidate.support.recurrence_k;
        }
        _ => unreachable!("candidate survey only covers mapped pilots"),
    }
    config
}

fn finding_id(f: &Finding) -> String {
    format!(
        "{}:{}:{}:{}",
        f.key_idx.get(),
        f.code.code(),
        f.range.start,
        f.range.end
    )
}

fn finding_value(f: &Finding) -> String {
    format!("{}|{:?}|{:?}", finding_id(f), f.score, f.args)
}

fn quantile(scores: &mut [f32], p: f32) -> Option<f32> {
    if scores.is_empty() {
        return None;
    }
    scores.sort_by(f32::total_cmp);
    let rank = (p * scores.len() as f32).ceil() as usize;
    Some(scores[rank.saturating_sub(1).min(scores.len() - 1)])
}

fn snapshot(corpus: &Corpus, rule: RuleId, config: Config) -> Snapshot {
    let mut opportunity_config = config.clone();
    match rule {
        RuleId::PunctuationSpacingAnomaly => {
            opportunity_config.punctuation_spacing.emit_score_min = 0.0
        }
        RuleId::SentenceInitialLowercase => {
            opportunity_config
                .casing
                .sentence_initial
                .evidence
                .emit_score_min = 0.0
        }
        RuleId::InconsistentWordCasing => {
            opportunity_config
                .casing
                .inconsistent_word
                .evidence
                .emit_score_min = 0.0
        }
        _ => unreachable!(),
    }
    let mut opportunity_config_rules = opportunity_config.rules.clone();
    opportunity_config_rules.extend(
        RuleId::ALL
            .iter()
            .map(|&candidate| (candidate, candidate == rule)),
    );
    opportunity_config.rules = opportunity_config_rules;
    let opportunities = analyze_with_config(corpus, None, &opportunity_config)
        .into_iter()
        .filter(|f| f.code == rule)
        .count();

    let mut rules = config.rules.clone();
    rules.extend(
        RuleId::ALL
            .iter()
            .map(|&candidate| (candidate, candidate == rule)),
    );
    let mut config = config;
    config.rules = rules;
    let findings = analyze_with_config(corpus, None, &config);
    let findings: Vec<Finding> = findings.into_iter().filter(|f| f.code == rule).collect();
    let mut scores: Vec<f32> = findings.iter().filter_map(|f| f.score).collect();
    let mut ids = BTreeSet::new();
    let mut values = BTreeMap::new();
    for finding in &findings {
        let id = finding_id(finding);
        ids.insert(id.clone());
        values.insert(id, finding_value(finding));
    }
    let examples = values.values().take(3).cloned().collect();
    let mut median_scores = scores.clone();
    let mut p75_scores = scores.clone();
    let mut p90_scores = scores.clone();
    let p99 = quantile(&mut scores, 0.99);
    Snapshot {
        findings: findings.len(),
        opportunities,
        median: quantile(&mut median_scores, 0.50),
        p75: quantile(&mut p75_scores, 0.75),
        p90: quantile(&mut p90_scores, 0.90),
        p99,
        ids,
        values,
        examples,
    }
}

fn chapter_blocks(corpus: &Corpus) -> Vec<Vec<usize>> {
    let mut blocks = Vec::new();
    let mut start = 0;
    while start < corpus.len() {
        let first = parse_key(&corpus.keys()[start]).unwrap();
        let mut end = start + 1;
        while end < corpus.len() {
            let next = parse_key(&corpus.keys()[end]).unwrap();
            if next.book != first.book || next.chapter != first.chapter {
                break;
            }
            end += 1;
        }
        blocks.push((start..end).collect());
        start = end;
    }
    blocks
}

fn book_blocks(corpus: &Corpus) -> Vec<Vec<Vec<usize>>> {
    let mut books: Vec<Vec<Vec<usize>>> = Vec::new();
    for block in chapter_blocks(corpus) {
        let book = parse_key(&corpus.keys()[block[0]]).unwrap().book;
        if books
            .last()
            .and_then(|blocks| blocks.first())
            .map(|blocks| parse_key(&corpus.keys()[blocks[0]]).unwrap().book)
            == Some(book)
        {
            books.last_mut().unwrap().push(block);
        } else {
            books.push(vec![block]);
        }
    }
    books
}

fn maturity_views(corpus: &Corpus, include_maturity: bool) -> Vec<MaturityView> {
    let books = book_blocks(corpus);
    let mut views = Vec::new();
    for &(maturity, limit) in &MATURITIES {
        if !include_maturity && maturity != "full" {
            continue;
        }
        for &order in &["canonical", "reverse"] {
            let ordered: Vec<&Vec<usize>> = if order == "canonical" {
                books.iter().flat_map(|blocks| blocks.iter()).collect()
            } else {
                books
                    .iter()
                    .flat_map(|blocks| blocks.iter().rev())
                    .collect()
            };
            let block_limit = if limit == usize::MAX {
                ordered.len()
            } else {
                limit
            };
            let selected: Vec<usize> = ordered
                .into_iter()
                .take(block_limit)
                .flat_map(|block| block.iter().copied())
                .collect();
            if selected.is_empty() {
                continue;
            }
            let keys = selected.iter().map(|&i| corpus.keys()[i].clone()).collect();
            let texts = selected
                .iter()
                .map(|&i| corpus.texts()[i].clone())
                .collect();
            views.push(MaturityView {
                maturity,
                order,
                corpus: Corpus::try_from_parts(keys, texts).unwrap(),
            });
        }
    }
    views
}

fn adjacent_counts(
    current: &Snapshot,
    right: Option<&Snapshot>,
    down: Option<&Snapshot>,
) -> (usize, usize, usize) {
    let mut additions = 0;
    let mut removals = 0;
    let mut flips = 0;
    for neighbor in [right, down].into_iter().flatten() {
        additions += neighbor.ids.difference(&current.ids).count();
        removals += current.ids.difference(&neighbor.ids).count();
        flips += current
            .ids
            .intersection(&neighbor.ids)
            .filter(|id| current.values.get(*id) != neighbor.values.get(*id))
            .count();
    }
    (additions, removals, flips)
}

fn corpus_report(corpus_id: &str, corpus: &Corpus, include_maturity: bool) -> CorpusReport {
    let views = maturity_views(corpus, include_maturity);
    let candidates_by_rule: Vec<(RuleId, Vec<Candidate>)> = [
        RuleId::PunctuationSpacingAnomaly,
        RuleId::SentenceInitialLowercase,
        RuleId::InconsistentWordCasing,
    ]
    .into_iter()
    .map(|rule| (rule, candidates(rule)))
    .collect();
    let mut rows = String::new();
    let mut cells = Vec::new();
    for view in &views {
        for &(rule, ref grid) in &candidates_by_rule {
            let snapshots: Vec<Snapshot> = grid
                .par_iter()
                .map(|&candidate| snapshot(&view.corpus, rule, config_for(rule, candidate)))
                .collect();
            for (index, candidate) in grid.iter().copied().enumerate() {
                let right = (index % 3 != 2).then(|| &snapshots[index + 1]);
                let down = (index / 3 != 2).then(|| &snapshots[index + 3]);
                let (additions, removals, flips) = adjacent_counts(&snapshots[index], right, down);
                let example = snapshots[index].examples.join(" || ");
                writeln!(
                    rows,
                    "{}\t{corpus_id}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    rule.code(),
                    view.maturity,
                    view.order,
                    candidate.unusualness,
                    candidate.support.label,
                    snapshots[index].opportunities,
                    snapshots[index].findings,
                    snapshots[index]
                        .median
                        .map_or_else(|| "-".into(), |v| format!("{v:.6}")),
                    snapshots[index]
                        .p75
                        .map_or_else(|| "-".into(), |v| format!("{v:.6}")),
                    snapshots[index]
                        .p90
                        .map_or_else(|| "-".into(), |v| format!("{v:.6}")),
                    snapshots[index]
                        .p99
                        .map_or_else(|| "-".into(), |v| format!("{v:.6}")),
                    additions,
                    removals,
                    flips,
                    example,
                )
                .unwrap();
                cells.push(CellSummary {
                    rule,
                    unusualness: (candidate.unusualness * 1000.0).round() as u32,
                    support: candidate.support.label,
                    maturity: view.maturity,
                    order: view.order,
                    opportunities: snapshots[index].opportunities,
                    findings: snapshots[index].findings,
                });
            }
        }
    }
    let max_row_bytes = rows.lines().map(str::len).max().unwrap_or(0);
    CorpusReport {
        text: rows,
        cells,
        max_row_bytes,
    }
}

fn percentile(values: &mut [usize], p: f32) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let rank = (p * values.len() as f32).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn correlation(rows: &[CellSummary]) -> f32 {
    if rows.len() < 2 {
        return 0.0;
    }
    let n = rows.len() as f32;
    let mean_x = rows.iter().map(|r| r.opportunities as f32).sum::<f32>() / n;
    let mean_y = rows.iter().map(|r| r.findings as f32).sum::<f32>() / n;
    let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
    for row in rows {
        let x = row.opportunities as f32 - mean_x;
        let y = row.findings as f32 - mean_y;
        xy += x * y;
        xx += x * x;
        yy += y * y;
    }
    if xx == 0.0 || yy == 0.0 {
        0.0
    } else {
        xy / (xx * yy).sqrt()
    }
}

/// Run the non-circular candidate sweep. Full-fleet rows use the full corpus
/// maturity only; the first 28 deterministic corpora additionally receive the
/// 1/5/28/120 ladder and alternate chapter order.
pub(crate) fn review_depth_survey(path: &Path, out_path: &Path, tier: &str) {
    let scope = match tier {
        "wa" => OracleScope::Wa,
        "full" | "small" => OracleScope::Full,
        other => panic!("unknown Review Depth survey tier {other:?} (want small|wa|full)"),
    };
    let corpora = load_corpora(path, scope);
    if tier == "small" && corpora.len() > 20 {
        panic!(
            "small Review Depth candidate input must be a small blob, got {} corpora",
            corpora.len()
        );
    }
    let maturity_sample = corpora.len().min(MATURITY_SAMPLE);
    let started = Instant::now();
    let reports: Vec<CorpusReport> = corpora
        .par_iter()
        .enumerate()
        .map(|(index, (id, corpus))| corpus_report(id, corpus, index < maturity_sample))
        .collect();
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(out_path)
            .unwrap_or_else(|e| panic!("create {}: {e}", out_path.display())),
    );
    writeln!(
        out,
        "# review-depth-candidate-survey-v2\ttier={tier}\tcorpora={}\tmaturity_sample={maturity_sample}\tgrid=3x3",
        corpora.len()
    )
    .unwrap();
    writeln!(
        out,
        "rule\tcorpus\tmaturity\torder\tunusualness\tsupport\topportunities\tfindings\tmedian_score\tp75_score\tp90_score\tp99_score\tadj_additions\tadj_removals\tadj_flips\texamples"
    )
    .unwrap();
    for report in &reports {
        out.write_all(report.text.as_bytes()).unwrap();
    }

    let mut grouped: BTreeMap<
        (RuleId, u32, &'static str, &'static str, &'static str),
        Vec<CellSummary>,
    > = BTreeMap::new();
    for report in &reports {
        for cell in &report.cells {
            grouped
                .entry((
                    cell.rule,
                    cell.unusualness,
                    cell.support,
                    cell.maturity,
                    cell.order,
                ))
                .or_default()
                .push(cell.clone());
        }
    }
    for ((rule, unusualness, support, maturity, order), rows) in grouped {
        let mut findings: Vec<usize> = rows.iter().map(|r| r.findings).collect();
        let mut opportunities: Vec<usize> = rows.iter().map(|r| r.opportunities).collect();
        let p50 = percentile(&mut findings.clone(), 0.50);
        let p75 = percentile(&mut findings.clone(), 0.75);
        let p90 = percentile(&mut findings.clone(), 0.90);
        let p99 = percentile(&mut findings, 0.99);
        let corr = correlation(&rows);
        writeln!(
            out,
            "# audit\t{}\t{maturity}\t{order}\t{support}\t{unusualness}\tcorpus_n={}\tfindings_p50={p50}\tfindings_p75={p75}\tfindings_p90={p90}\tfindings_p99={p99}\topportunities_p50={}\topportunities_p90={}\tcorr_opportunities_findings={corr:.6}",
            rule.code(),
            rows.len(),
            percentile(&mut opportunities, 0.50),
            percentile(&mut opportunities, 0.90),
        )
        .unwrap();
    }
    let max_summary_bytes = reports.iter().map(|r| r.max_row_bytes).max().unwrap_or(0);
    writeln!(
        out,
        "# runtime\twall_ms={}\tmax_row_bytes={max_summary_bytes}",
        started.elapsed().as_millis()
    )
    .unwrap();
    eprintln!(
        "review-depth candidate survey: wrote {} corpora, maturity sample {}, grid 3x3 × 3 rules to {}",
        corpora.len(),
        maturity_sample,
        out_path.display()
    );
}
