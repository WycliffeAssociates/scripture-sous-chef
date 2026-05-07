//! Dogfood CLI for the engine. v0 surface: `sous check <dir>`.
//!
//! Loads a USFM corpus, runs `analyze()`, prints findings to stdout
//! and writes a JSON dump to `debug/<corpus-name>.json` for review.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use ssc_core::aggregate::{AggregationPolicy, aggregate_with_posteriors};
use ssc_core::analysis::char_ngrams::CharNgramStats;
use ssc_core::analysis::candidate_families::{
    CandidateFamiliesConfig, CandidateFamily, CandidateFamilies, GeneratorKind,
};
use ssc_core::analysis::compression::{CompressionTextureConfig, CompressionTextureModel};
use ssc_core::analysis::lemma_cluster::{LemmaClusterConfig, LemmaClusters};
use ssc_core::analysis::lemma_feedback::LabelledLemmaIndex;
use ssc_core::analysis::lexicon::{Lexicon, LexiconConfig};
use ssc_core::analysis::morphology::SegmentedCorpus;
use ssc_core::analysis::posterior::{BetaPosterior, PosteriorStore, PriorTable};
use ssc_core::analysis::rare_words::{
    RareWordsAnalysis, RareWordsConfig, TriageCandidate,
};
use ssc_core::analyze_with_stats;
use ssc_core::config::{Config, ExceptionSet};
use ssc_core::diagnostics::{Diagnostics, Severity};
use ssc_core::discourse::Discourse;
use ssc_core::project::Project;
use ssc_ingest::{build, usfm};

mod config_loader {
    include!("../config_loader.rs");
}

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  \
         sous check       [--nt-only] [--config <path>] [--source <dir>] [--all] <corpus-dir>\n  \
         sous triage      [--nt-only] [--config <path>] [--out markdown|html] [--limit N] <corpus-dir>\n  \
         sous dump-words  [--nt-only] <corpus-dir>   write debug/<name>.words.tsv (form\\tcount)"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.into_iter();
    let Some(cmd) = iter.next() else {
        return usage();
    };
    let rest: Vec<String> = iter.collect();
    match cmd.as_str() {
        "check" => run_check(rest),
        "triage" => run_triage(rest),
        "dump-words" => run_dump_words(rest),
        other => {
            eprintln!("unknown subcommand: {other}");
            usage()
        }
    }
}

fn run_check(args: Vec<String>) -> ExitCode {
    let mut nt_only = false;
    let mut config_path: Option<PathBuf> = None;
    let mut source_path: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut show_all = false;
    let mut args_iter = args.into_iter().peekable();
    while let Some(a) = args_iter.next() {
        match a.as_str() {
            "--nt-only" => nt_only = true,
            "--all" => show_all = true,
            "--config" => {
                let Some(p) = args_iter.next() else {
                    eprintln!("--config requires a path argument");
                    return usage();
                };
                config_path = Some(PathBuf::from(p));
            }
            "--source" => {
                let Some(p) = args_iter.next() else {
                    eprintln!("--source requires a path argument");
                    return usage();
                };
                source_path = Some(PathBuf::from(p));
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                return usage();
            }
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else { return usage() };

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    // Load config: explicit path, discovered path, or defaults
    let (config, exceptions) = match config_path {
        Some(p) => match config_loader::load_config(&p) {
            Ok((cfg, exc, warnings)) => {
                for w in warnings {
                    eprintln!("config warning: {w}");
                }
                (cfg, exc)
            }
            Err(e) => {
                eprintln!("config error: {e}");
                return ExitCode::from(1);
            }
        },
        None => {
            if let Some(p) = config_loader::discover_config(&path) {
                match config_loader::load_config(&p) {
                    Ok((cfg, exc, warnings)) => {
                        for w in warnings {
                            eprintln!("config warning: {w}");
                        }
                        (cfg, exc)
                    }
                    Err(e) => {
                        eprintln!("config warning: {} (using defaults)", e);
                        (Config::default(), ExceptionSet::default())
                    }
                }
            } else {
                (Config::default(), ExceptionSet::default())
            }
        }
    };

    let raw = match usfm::read_usfm_dir(&path, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            return ExitCode::from(1);
        }
    };

    // Load source corpus if --source is provided
    let source = match source_path {
        Some(src_path) => {
            let src_name = src_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let src_raw = match usfm::read_usfm_dir(&src_path, nt_only) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("read failed for source: {e}");
                    return ExitCode::from(1);
                }
            };
            Some((src_name, src_raw))
        }
        None => None,
    };

    // γ aggregation. Build the policy from defaults, then merge any
    // overrides supplied via `sous.json` so users can tune surfacing
    // and rule trustworthiness without touching code.
    let policy = aggregation_policy_from_config(&config);

    // GUI/editor integrations can write explicit accept/dismiss events here.
    // The dogfood CLI only reads them today; it is not the intended UX for
    // collecting feedback.
    let events_path = path.join(".sous").join("events.jsonl");
    let posteriors = match PosteriorStore::from_event_log(
        &events_path,
        placeholder_priors_from_policy_weights(&policy),
    ) {
        Ok(store) => store,
        Err(e) => {
            eprintln!(
                "feedback warning: could not read {}: {}",
                events_path.display(),
                e
            );
            PosteriorStore::new(placeholder_priors_from_policy_weights(&policy))
        }
    };
    let mut exceptions = exceptions;
    for finding_id in posteriors.dismissed_finding_ids() {
        exceptions.insert_finding_id(finding_id);
    }

    // Replay lemma-family events from the same JSONL log; the reader
    // ignores finding-level kinds, so the two readers share one file.
    let lemma_labels = LabelledLemmaIndex::from_event_log(&events_path).unwrap_or_else(|e| {
        eprintln!(
            "feedback warning: could not read lemma labels from {}: {}",
            events_path.display(),
            e
        );
        LabelledLemmaIndex::default()
    });

    let project = build::project_from_raw_map_with_labels(
        name.clone(),
        raw,
        source,
        config,
        exceptions,
        lemma_labels,
    );

    let start = Instant::now();
    let (diags, stats) = analyze_with_stats(&project);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    let clusters = aggregate_with_posteriors(&diags, &policy, Some(&posteriors));
    let n_surfaced = clusters.iter().filter(|c| c.surfaced).count();
    let n_multi_rule = clusters.iter().filter(|c| c.rules_fired.len() >= 2).count();

    // Console output: surfaced clusters by default; `--all` shows
    // unsurfaced ones too. JSON outputs always contain everything.
    for cluster in &clusters {
        if !show_all && !cluster.surfaced {
            continue;
        }
        let tier = if cluster.surfaced { "✓" } else { "·" };
        let corr = if cluster.matched_correlations.is_empty() {
            String::new()
        } else {
            format!("  [{}]", cluster.matched_correlations.join(","))
        };
        println!(
            "{} {:5.2}  {}  {} rule(s){}",
            tier,
            cluster.score,
            cluster.sid,
            cluster.rules_fired.len(),
            corr,
        );
        for f in &cluster.findings {
            println!(
                "        {:>5?}  {:<26}  {}",
                f.severity, f.rule_id, f.message
            );
        }
    }

    let json_path = Path::new("debug").join(format!("{name}.json"));
    if let Err(e) = write_diagnostics_json(&json_path, &project, &diags, &clusters) {
        eprintln!("warning: could not write {}: {}", json_path.display(), e);
    } else {
        eprintln!("wrote {}", json_path.display());
    }

    let stats_path = Path::new("debug").join(format!("{name}.stats.json"));
    if let Err(e) = write_json(&stats_path, &stats) {
        eprintln!("warning: could not write {}: {}", stats_path.display(), e);
    } else {
        eprintln!("wrote {}", stats_path.display());
    }

    let clusters_path = Path::new("debug").join(format!("{name}.clusters.json"));
    if let Err(e) = write_clusters_json(&clusters_path, &project, &clusters) {
        eprintln!(
            "warning: could not write {}: {}",
            clusters_path.display(),
            e
        );
    } else {
        eprintln!("wrote {}", clusters_path.display());
    }
    eprintln!(
        "[{}] {} verses, {} findings, {} clusters ({} surfaced, {} multi-rule), {:.3} ms",
        name,
        project.target.verses.len(),
        diags.findings.len(),
        clusters.len(),
        n_surfaced,
        n_multi_rule,
        elapsed_ms,
    );

    ExitCode::SUCCESS
}

fn aggregation_policy_from_config(config: &Config) -> AggregationPolicy {
    let mut policy = AggregationPolicy::default();
    if let Some(agg) = &config.aggregation {
        if let Some(v) = agg.min_surface_score {
            policy.min_surface_score = v;
        }
        if let Some(v) = agg.default_weight {
            policy.default_weight = v;
        }
    }
    for rc in &config.rules {
        if let Some(w) = rc.weight {
            policy.rule_weights.insert(rc.id, w);
        }
    }
    policy
}

/// TODO: replace with priors loaded from a checked-in `priors.json` once
/// an offline eBible sweep produces empirically-fitted noise floors. For
/// now we synthesise a Beta from the policy's per-rule weight just so
/// the posterior store has a non-flat starting point.
///
/// This conflates two different things:
/// - **policy weight** is a hand-tuned trust scalar in `[0, 1]` that
///   controls how much a rule's evidence contributes before any feedback
///   exists.
/// - **noise-floor prior** is an empirically fitted Beta describing how
///   often this rule fires on diverse, reasonably-clean corpora.
///
/// They aren't the same thing. When a real prior table exists, delete
/// this helper, load it once at startup, and pass that `PriorTable` in
/// instead. The posterior store and `Beta` arithmetic do not change.
fn placeholder_priors_from_policy_weights(policy: &AggregationPolicy) -> PriorTable {
    let mut priors = PriorTable::with_default(prior_with_mean(policy.default_weight));
    for (rule_id, weight) in &policy.rule_weights {
        priors.insert_rule(*rule_id, prior_with_mean(*weight));
    }
    priors
}

fn prior_with_mean(mean: f64) -> BetaPosterior {
    const PRIOR_STRENGTH: f64 = 2.0;
    let mean = if mean.is_nan() {
        0.5
    } else {
        mean.clamp(0.0, 1.0)
    };
    BetaPosterior::new(mean * PRIOR_STRENGTH, (1.0 - mean) * PRIOR_STRENGTH)
}

/// One finding within a grouped SID entry — no redundant sid/verse fields.
#[derive(serde::Serialize)]
struct DiagFinding {
    rule_id: String,
    severity: Severity,
    finding_id: u64,
    cluster_key: String,
    byte_start: usize,
    byte_end: usize,
    span: String,
    message: String,
    evidence: f64,
}

/// All findings for one SID, with cluster score for ranking.
#[derive(serde::Serialize)]
struct DiagVerse {
    sid: String,
    score: f64,
    surfaced: bool,
    verse: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    src_verse: String,
    findings: Vec<DiagFinding>,
}

/// Write diagnostics JSON grouped by SID, sorted score-descending.
fn write_diagnostics_json(
    path: &Path,
    project: &Project,
    diags: &Diagnostics,
    clusters: &[ssc_core::aggregate::Cluster<'_>],
) -> std::io::Result<()> {
    use ssc_core::sid::Sid;
    use std::collections::HashMap;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Build SID → (score, surfaced) from clusters for O(1) lookup.
    let cluster_meta: HashMap<Sid, (f64, bool)> = clusters
        .iter()
        .map(|c| (c.sid, (c.score, c.surfaced)))
        .collect();

    // Group findings by SID, preserving cluster score order.
    let mut by_sid: std::collections::BTreeMap<Sid, Vec<DiagFinding>> =
        std::collections::BTreeMap::new();
    for f in &diags.findings {
        by_sid.entry(f.sid).or_default().push(DiagFinding {
            rule_id: f.rule_id.0.to_string(),
            severity: f.severity,
            finding_id: f.finding_id.0,
            cluster_key: f.cluster_key.to_string(),
            byte_start: f.byte_range.start,
            byte_end: f.byte_range.end,
            span: f.span.to_string(),
            message: f.message.clone(),
            evidence: f.evidence,
        });
    }

    let mut verses: Vec<DiagVerse> = by_sid
        .into_iter()
        .map(|(sid, findings)| {
            let verse_text = project
                .target
                .verses
                .get(&sid)
                .map(|v| v.nfc.as_str())
                .unwrap_or("");
            let src_verse_text = project
                .source
                .as_ref()
                .and_then(|s| s.verses.get(&sid))
                .map(|v| v.nfc.as_str())
                .unwrap_or("");
            let (score, surfaced) = cluster_meta.get(&sid).copied().unwrap_or((0.0, false));
            DiagVerse {
                sid: sid.to_string(),
                score,
                surfaced,
                verse: verse_text.to_string(),
                src_verse: src_verse_text.to_string(),
                findings,
            }
        })
        .collect();

    verses.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let output = serde_json::json!({
        "count": verses.len(),
        "verses": verses,
    });

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}

/// Cluster output with embedded verse text and finding bodies, so a
/// reviewer (human or AI) can scan the file without joining against
/// findings/stats. The `score_breakdown` field is the audit trail —
/// it shows every weight, evidence, and multiplier that contributed
/// to `score`, in the same order the formula composes them.
#[derive(serde::Serialize)]
struct ClusterOut {
    sid: String,
    score: f64,
    surfaced: bool,
    byte_start: usize,
    byte_end: usize,
    rules_fired: Vec<String>,
    matched_correlations: Vec<String>,
    verse: String,
    findings: Vec<ClusterFinding>,
    score_breakdown: ssc_core::aggregate::ScoreBreakdown,
}

#[derive(serde::Serialize)]
struct ClusterFinding {
    rule_id: String,
    severity: Severity,
    finding_id: u64,
    cluster_key: String,
    byte_start: usize,
    byte_end: usize,
    span: String,
    message: String,
    evidence: f64,
}

fn write_clusters_json(
    path: &Path,
    project: &Project,
    clusters: &[ssc_core::aggregate::Cluster<'_>],
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let out: Vec<ClusterOut> = clusters
        .iter()
        .map(|c| {
            let verse_text = project
                .target
                .verses
                .get(&c.sid)
                .map(|v| v.nfc.as_str())
                .unwrap_or("");
            ClusterOut {
                sid: c.sid.to_string(),
                score: c.score,
                surfaced: c.surfaced,
                byte_start: c.byte_range.start,
                byte_end: c.byte_range.end,
                rules_fired: c.rules_fired.iter().map(|r| r.0.to_string()).collect(),
                matched_correlations: c.matched_correlations.clone(),
                verse: verse_text.to_string(),
                findings: c
                    .findings
                    .iter()
                    .map(|f| ClusterFinding {
                        rule_id: f.rule_id.0.to_string(),
                        severity: f.severity,
                        finding_id: f.finding_id.0,
                        cluster_key: f.cluster_key.to_string(),
                        byte_start: f.byte_range.start,
                        byte_end: f.byte_range.end,
                        span: f.span.to_string(),
                        message: f.message.clone(),
                        evidence: f.evidence,
                    })
                    .collect(),
                score_breakdown: c.score_breakdown.clone(),
            }
        })
        .collect();
    let body = serde_json::json!({
        "count": out.len(),
        "clusters": out,
    });
    let json = serde_json::to_string_pretty(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}

// ─── dump-words subcommand ──────────────────────────────────────────────
//
// Word-list dump for external segmenter experiments (Morfessor,
// MorphAGram, etc.). Output is `debug/<name>.words.tsv` with one row
// per surface form, lowercased and stripped to alphabetic characters,
// followed by a tab and the corpus count. Sorted by count descending.

fn run_dump_words(args: Vec<String>) -> ExitCode {
    let mut nt_only = false;
    let mut path: Option<PathBuf> = None;
    let mut args_iter = args.into_iter().peekable();
    while let Some(a) = args_iter.next() {
        match a.as_str() {
            "--nt-only" => nt_only = true,
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                return usage();
            }
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else { return usage() };
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    let raw = match usfm::read_usfm_dir(&path, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            return ExitCode::from(1);
        }
    };

    let project = build::project_from_raw_map(
        name.clone(),
        raw,
        None,
        Config::default(),
        ExceptionSet::default(),
    );
    let discourse = Discourse::build(&project.target);
    let lexicon = Lexicon::build(&discourse, LexiconConfig::default());

    let out_path = Path::new("debug").join(format!("{name}.words.tsv"));
    if let Some(parent) = out_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("warning: could not create {}: {}", parent.display(), e);
            return ExitCode::from(1);
        }
    }

    let mut entries: Vec<(&String, u32)> = lexicon
        .words
        .iter()
        .map(|(form, profile)| (form, profile.n_total()))
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let mut body = String::new();
    use std::fmt::Write;
    for (form, count) in &entries {
        let _ = writeln!(body, "{form}\t{count}");
    }
    if let Err(e) = fs::write(&out_path, &body) {
        eprintln!("warning: could not write {}: {}", out_path.display(), e);
        return ExitCode::from(1);
    }
    eprintln!("wrote {} ({} forms)", out_path.display(), entries.len());
    ExitCode::SUCCESS
}

// ─── triage subcommand ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriageOutput {
    Markdown,
    Html,
}

fn run_triage(args: Vec<String>) -> ExitCode {
    let mut nt_only = false;
    let mut config_path: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut output = TriageOutput::Markdown;
    let mut limit: usize = 50;
    let mut args_iter = args.into_iter().peekable();
    while let Some(a) = args_iter.next() {
        match a.as_str() {
            "--nt-only" => nt_only = true,
            "--config" => {
                let Some(p) = args_iter.next() else {
                    eprintln!("--config requires a path argument");
                    return usage();
                };
                config_path = Some(PathBuf::from(p));
            }
            "--out" => {
                let Some(o) = args_iter.next() else {
                    eprintln!("--out requires markdown|html");
                    return usage();
                };
                output = match o.as_str() {
                    "markdown" | "md" => TriageOutput::Markdown,
                    "html" => TriageOutput::Html,
                    other => {
                        eprintln!("unknown --out: {other}");
                        return usage();
                    }
                };
            }
            "--limit" => {
                let Some(n) = args_iter.next() else {
                    eprintln!("--limit requires a number");
                    return usage();
                };
                limit = match n.parse::<usize>() {
                    Ok(v) => v,
                    Err(_) => {
                        eprintln!("--limit must be a positive integer");
                        return usage();
                    }
                };
            }
            other if other.starts_with("--") => {
                eprintln!("unknown flag: {other}");
                return usage();
            }
            _ => path = Some(PathBuf::from(a)),
        }
    }
    let Some(path) = path else { return usage() };

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();

    // Re-use the same config/discovery dance as `check`.
    let (config, _exceptions) = match config_path {
        Some(p) => match config_loader::load_config(&p) {
            Ok((cfg, exc, warnings)) => {
                for w in warnings {
                    eprintln!("config warning: {w}");
                }
                (cfg, exc)
            }
            Err(e) => {
                eprintln!("config error: {e}");
                return ExitCode::from(1);
            }
        },
        None => {
            if let Some(p) = config_loader::discover_config(&path) {
                match config_loader::load_config(&p) {
                    Ok((cfg, exc, warnings)) => {
                        for w in warnings {
                            eprintln!("config warning: {w}");
                        }
                        (cfg, exc)
                    }
                    Err(e) => {
                        eprintln!("config warning: {} (using defaults)", e);
                        (Config::default(), ExceptionSet::default())
                    }
                }
            } else {
                (Config::default(), ExceptionSet::default())
            }
        }
    };
    let _ = config;

    let raw = match usfm::read_usfm_dir(&path, nt_only) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("read failed: {e}");
            return ExitCode::from(1);
        }
    };

    let events_path = path.join(".sous").join("events.jsonl");
    let labels = LabelledLemmaIndex::from_event_log(&events_path).unwrap_or_else(|e| {
        eprintln!(
            "feedback warning: could not read {}: {}",
            events_path.display(),
            e
        );
        LabelledLemmaIndex::default()
    });

    // Build a minimal project so we can construct a discourse and the
    // analysis primitives. We don't need the full check pipeline here.
    let project = build::project_from_raw_map_with_labels(
        name.clone(),
        raw,
        None,
        Config::default(),
        ExceptionSet::default(),
        labels.clone(),
    );

    let discourse = Discourse::build(&project.target);
    let lexicon = Lexicon::build(&discourse, LexiconConfig::default());
    let texture = CompressionTextureModel::build(&project.target, CompressionTextureConfig::default());
    let ngrams = CharNgramStats::build(lexicon.words.keys().map(String::as_str));
    let clusters = LemmaClusters::build(&project.target, LemmaClusterConfig::default());

    let analysis = RareWordsAnalysis::build_with_labels(
        &lexicon,
        &texture,
        &ngrams,
        Some(&labels),
        RareWordsConfig::default(),
    );

    if analysis.stats.disabled {
        eprintln!(
            "[{name}] triage disabled — corpus has only {} word types (need ≥200)",
            analysis.stats.n_word_types
        );
        return ExitCode::SUCCESS;
    }

    let top: Vec<&TriageCandidate> = analysis.candidates.iter().take(limit).collect();
    let seed_forms: Vec<String> = top.iter().map(|c| c.form.clone()).collect();

    // Optional morphology: if the user has run the Python segmenter
    // and dropped a segmentation.json next to the corpus, load it and
    // add the SegmenterStem proposer to the family pool. Missing file
    // is a no-op (the segmenter is `Disabled` and `build` skips the
    // stem branch).
    let segmentation_path = path.join(".sous").join("segmentation.json");
    if segmentation_path.exists() {
        if let Some(stale_count) = corpus_files_newer_than(&path, &segmentation_path) {
            if stale_count > 0 {
                eprintln!(
                    "warning: {} corpus file(s) modified after {} — \
                     consider regenerating with \
                     `experiments/segmenter_benchmark/dump_segmentation.py {}`",
                    stale_count,
                    segmentation_path.display(),
                    path.display(),
                );
            }
        }
    }
    let morphology = SegmentedCorpus::from_segmentation_file(&segmentation_path);
    let families = CandidateFamilies::build_with_morphology(
        &lexicon,
        &clusters,
        Some(&morphology),
        &seed_forms,
        CandidateFamiliesConfig::default(),
    );

    // Always write a debug JSON of the full analysis for downstream
    // tooling, regardless of whether the user picked html or markdown.
    let json_path = Path::new("debug").join(format!("{name}.triage.json"));
    if let Err(e) = write_triage_json(&json_path, &analysis, &families) {
        eprintln!("warning: could not write {}: {}", json_path.display(), e);
    } else {
        eprintln!("wrote {}", json_path.display());
    }

    let body = match output {
        TriageOutput::Markdown => render_triage_markdown(&name, &analysis, &families, &top),
        TriageOutput::Html => render_triage_html(&name, &analysis, &families, &top),
    };
    let extension = match output {
        TriageOutput::Markdown => "md",
        TriageOutput::Html => "html",
    };
    let out_path = Path::new("debug").join(format!("{name}.triage.{extension}"));
    if let Some(parent) = out_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("warning: could not create {}: {}", parent.display(), e);
            return ExitCode::from(1);
        }
    }
    if let Err(e) = fs::write(&out_path, &body) {
        eprintln!("warning: could not write {}: {}", out_path.display(), e);
        return ExitCode::from(1);
    }
    eprintln!("wrote {}", out_path.display());

    let n_known_good = labels.known_good.len();
    let n_known_bad = labels.known_bad.len();
    let n_confirmed_families = labels.confirmed_families.len();
    let morph_note = match morphology.stats.segmenter {
        ssc_core::analysis::morphology::SegmenterKind::Disabled => " no segmenter".to_string(),
        _ => format!(
            " segmenter={:?} morph-types={}",
            morphology.stats.segmenter, morphology.stats.n_morpheme_types
        ),
    };
    eprintln!(
        "[{}] {} types, {} rare ({} after filter), top {} suspect{} · labels: {} good / {} bad / {} families ·{}",
        name,
        analysis.stats.n_word_types,
        analysis.stats.n_rare_types,
        analysis.stats.n_rare_after_filter,
        top.len(),
        if top.len() == 1 { "" } else { "s" },
        n_known_good,
        n_known_bad,
        n_confirmed_families,
        morph_note,
    );
    ExitCode::SUCCESS
}

fn write_triage_json(
    path: &Path,
    analysis: &RareWordsAnalysis,
    families: &CandidateFamilies,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::json!({
        "stats": &analysis.stats,
        "candidates": &analysis.candidates,
        "families": &families.families,
        "by_form": &families.by_form,
    });
    let json = serde_json::to_string_pretty(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}

fn render_triage_markdown(
    name: &str,
    analysis: &RareWordsAnalysis,
    families: &CandidateFamilies,
    top: &[&TriageCandidate],
) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(out, "# Triage queue — `{name}`");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} word types · {} rare ({} after filter) · median compression ratio {:.3}, MAD {:.3}",
        analysis.stats.n_word_types,
        analysis.stats.n_rare_types,
        analysis.stats.n_rare_after_filter,
        analysis.stats.median_compression_ratio,
        analysis.stats.mad_compression_ratio,
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## How to label");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Append one JSON line per decision to `<corpus>/.sous/events.jsonl`. Templates ready to copy beneath each entry."
    );
    let _ = writeln!(out);
    for (i, candidate) in top.iter().enumerate() {
        let _ = writeln!(
            out,
            "### {}. `{}` (count {}, suspicion {:.2})",
            i + 1,
            candidate.form,
            candidate.count,
            candidate.suspicion,
        );
        let neighbour_count = families
            .families_for(&candidate.form)
            .iter()
            .flat_map(|fam| fam.forms.iter())
            .filter(|f| f.form != candidate.form)
            .count();
        let _ = writeln!(
            out,
            "Character anomaly: {:.2} · Compression ratio: {:.3} · Neighbours surfaced: {}",
            candidate.evidence.character_anomaly,
            candidate.evidence.raw_compression_ratio,
            neighbour_count,
        );
        let _ = writeln!(out);

        let proposed = families.families_for(&candidate.form);
        if proposed.is_empty() {
            let _ = writeln!(out, "_No candidate family proposed beyond surface-identity._");
        } else {
            for family in &proposed {
                let tags: Vec<String> =
                    family.proposed_by.iter().map(generator_label).collect();
                let _ = writeln!(
                    out,
                    "- **Family `{}`** [{}]",
                    family.representative,
                    tags.join(", ")
                );
                let members: Vec<String> = family
                    .forms
                    .iter()
                    .map(|f| format!("`{}` ({})", f.form, f.count))
                    .collect();
                let _ = writeln!(out, "  members: {}", members.join(", "));
                let confirm = family_event_template("lemma_family_confirm", family);
                let reject = family_event_template("lemma_family_reject", family);
                let _ = writeln!(out, "  ```jsonl");
                let _ = writeln!(out, "  {confirm}");
                let _ = writeln!(out, "  {reject}");
                let _ = writeln!(out, "  ```");
            }
        }
        let _ = writeln!(out);
    }
    out
}

fn render_triage_html(
    name: &str,
    analysis: &RareWordsAnalysis,
    families: &CandidateFamilies,
    top: &[&TriageCandidate],
) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(out, "<!doctype html>");
    let _ = writeln!(out, "<html><head><meta charset=\"utf-8\"><title>Triage — {}</title>", html_escape(name));
    let _ = writeln!(
        out,
        "<style>body{{font-family:system-ui,-apple-system,sans-serif;max-width:780px;margin:2em auto;padding:0 1em;line-height:1.5}}h1,h2,h3{{font-weight:600}}.meta{{color:#666;font-size:0.9em}}.candidate{{margin:1.5em 0;padding:1em;border:1px solid #ddd;border-radius:8px}}.family{{margin:0.6em 0;padding:0.6em;background:#f6f6f6;border-radius:6px}}.tag{{display:inline-block;padding:0.1em 0.5em;margin-right:0.3em;background:#dde;border-radius:4px;font-size:0.8em}}pre{{background:#fff;border:1px solid #ddd;padding:0.5em;border-radius:4px;font-size:0.85em;overflow-x:auto}}</style>"
    );
    let _ = writeln!(out, "</head><body>");
    let _ = writeln!(out, "<h1>Triage queue — <code>{}</code></h1>", html_escape(name));
    let _ = writeln!(
        out,
        "<p class=\"meta\">{} word types · {} rare ({} after filter) · median compression {:.3}, MAD {:.3}</p>",
        analysis.stats.n_word_types,
        analysis.stats.n_rare_types,
        analysis.stats.n_rare_after_filter,
        analysis.stats.median_compression_ratio,
        analysis.stats.mad_compression_ratio,
    );
    let _ = writeln!(
        out,
        "<p>Copy a JSON line into <code>&lt;corpus&gt;/.sous/events.jsonl</code> to label.</p>"
    );
    for (i, candidate) in top.iter().enumerate() {
        let _ = writeln!(out, "<section class=\"candidate\">");
        let _ = writeln!(
            out,
            "<h3>{}. <code>{}</code> <span class=\"meta\">count {} · suspicion {:.2}</span></h3>",
            i + 1,
            html_escape(&candidate.form),
            candidate.count,
            candidate.suspicion,
        );
        let neighbour_count = families
            .families_for(&candidate.form)
            .iter()
            .flat_map(|fam| fam.forms.iter())
            .filter(|f| f.form != candidate.form)
            .count();
        let _ = writeln!(
            out,
            "<p class=\"meta\">char anomaly {:.2} · ratio {:.3} · neighbours {}</p>",
            candidate.evidence.character_anomaly,
            candidate.evidence.raw_compression_ratio,
            neighbour_count,
        );
        let proposed = families.families_for(&candidate.form);
        for family in &proposed {
            let _ = writeln!(out, "<div class=\"family\">");
            let _ = writeln!(
                out,
                "<strong>{}</strong>",
                html_escape(&family.representative)
            );
            for tag in &family.proposed_by {
                let _ = writeln!(
                    out,
                    "<span class=\"tag\">{}</span>",
                    html_escape(generator_label(tag).as_str())
                );
            }
            let members: Vec<String> = family
                .forms
                .iter()
                .map(|f| format!("<code>{}</code> ({})", html_escape(&f.form), f.count))
                .collect();
            let _ = writeln!(out, "<p>members: {}</p>", members.join(", "));
            let confirm = family_event_template("lemma_family_confirm", family);
            let reject = family_event_template("lemma_family_reject", family);
            let _ = writeln!(
                out,
                "<pre>{}\n{}</pre>",
                html_escape(&confirm),
                html_escape(&reject)
            );
            let _ = writeln!(out, "</div>");
        }
        let _ = writeln!(out, "</section>");
    }
    let _ = writeln!(out, "</body></html>");
    out
}

fn family_event_template(kind: &str, family: &CandidateFamily) -> String {
    let forms: Vec<String> = family.forms.iter().map(|f| format!("\"{}\"", f.form)).collect();
    format!(
        "{{\"v\":1,\"ts\":\"<TS>\",\"kind\":\"{}\",\"family_id\":{},\"forms\":[{}]}}",
        kind,
        family.family_id,
        forms.join(",")
    )
}

fn generator_label(g: &GeneratorKind) -> String {
    match g {
        GeneratorKind::SurfaceIdentity => "surface".to_string(),
        GeneratorKind::BkDistance { radius } => format!("bk≤{radius}"),
        GeneratorKind::PrefixOverlap => "prefix".to_string(),
        GeneratorKind::SegmenterStem => "stem".to_string(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Count corpus files (USFM) modified after `reference`'s mtime.
/// Returns `None` if either side has no usable mtime metadata. Used as
/// a cheap "your segmentation may be stale" check before reading
/// `<corpus>/.sous/segmentation.json`.
///
/// ASSUMPTION: any USFM file mtime newer than the reference is a sign
/// of corpus drift. False positives (e.g. `touch`-ing without real
/// content change) are acceptable — the warning is advisory.
fn corpus_files_newer_than(corpus_dir: &Path, reference: &Path) -> Option<usize> {
    let ref_meta = std::fs::metadata(reference).ok()?;
    let ref_mtime = ref_meta.modified().ok()?;
    let entries = std::fs::read_dir(corpus_dir).ok()?;
    let mut count = 0;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let is_usfm = entry_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("usfm"))
            .unwrap_or(false);
        if !is_usfm {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(mtime) = meta.modified() {
                if mtime > ref_mtime {
                    count += 1;
                }
            }
        }
    }
    Some(count)
}

/// Serde-based JSON dump for stats.
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}
