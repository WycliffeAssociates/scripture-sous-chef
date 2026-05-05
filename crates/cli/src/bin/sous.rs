//! Dogfood CLI for the engine. v0 surface: `sous check <dir>`.
//!
//! Loads a USFM corpus, runs `analyze()`, prints findings to stdout
//! and writes a JSON dump to `debug/<corpus-name>.json` for review.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use ssc_core::aggregate::{aggregate_with_posteriors, AggregationPolicy};
use ssc_core::analysis::posterior::{BetaPosterior, PosteriorStore, PriorTable};
use ssc_core::analyze_with_stats;
use ssc_core::config::{Config, ExceptionSet};
use ssc_core::diagnostics::{Diagnostics, Severity};
use ssc_core::project::Project;
use ssc_ingest::{build, usfm};

mod config_loader {
    include!("../config_loader.rs");
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: sous check [--nt-only] [--config <path>] [--source <dir>] [--all] <corpus-dir>"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.into_iter();
    let Some(cmd) = iter.next() else {
        return usage();
    };
    if cmd != "check" {
        eprintln!("unknown subcommand: {cmd}");
        return usage();
    }

    let mut nt_only = false;
    let mut config_path: Option<PathBuf> = None;
    let mut source_path: Option<PathBuf> = None;
    let mut path: Option<PathBuf> = None;
    let mut show_all = false;
    let mut args_iter = iter.peekable();
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
    let posteriors = match PosteriorStore::from_event_log(&events_path, priors_from_policy(&policy))
    {
        Ok(store) => store,
        Err(e) => {
            eprintln!(
                "feedback warning: could not read {}: {}",
                events_path.display(),
                e
            );
            PosteriorStore::new(priors_from_policy(&policy))
        }
    };
    let mut exceptions = exceptions;
    for finding_id in posteriors.dismissed_finding_ids() {
        exceptions.insert_finding_id(finding_id);
    }

    let project = build::project_from_raw_map(name.clone(), raw, source, config, exceptions);

    let start = Instant::now();
    let (diags, stats) = analyze_with_stats(&project);
    let elapsed_us = start.elapsed().as_micros();

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
        "[{}] {} verses, {} findings, {} clusters ({} surfaced, {} multi-rule), {}.{:03} µs",
        name,
        project.target.verses.len(),
        diags.findings.len(),
        clusters.len(),
        n_surfaced,
        n_multi_rule,
        elapsed_us / 1000,
        elapsed_us % 1000
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

fn priors_from_policy(policy: &AggregationPolicy) -> PriorTable {
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

/// Serde-based JSON dump for stats.
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(path, json)
}
