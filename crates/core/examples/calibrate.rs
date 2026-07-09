//! Throwaway calibration harness — NOT the library path.
//!
//! ADR 0010 keeps file IO out of `core`'s contract; this example exists only
//! to run rules over the vref corpus files (ADR 0040) and report finding
//! volumes for calibration decisions (vision §10). It reads
//! `corpora/vref/<id>.txt` directly (`REF\ttext`); the text is already onion's
//! projection, so there is no segmentation here.
//!
//! Usage:
//!   # proportionality (target vs reference):
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       corpora/vref/WA-bem-reg.txt corpora/vref/WA-en-ulb.txt
//!   # per-verse batch (one corpus, default config):
//!   cargo run --release -p ssc-core --example calibrate -- corpora/vref/WA-en-ulb.txt
//!   # repeated-run score report / parameter sweep:
//!   cargo run --release -p ssc-core --example calibrate -- --repeat corpora/vref/WA-en-ulb.txt [rate K]
//!   # fleet survey → self-contained HTML report (all rules, floors zeroed,
//!   # every corpus in the directory; out defaults to target/fleet-report.html):
//!   cargo run --release -p ssc-core --example calibrate -- --fleet corpora/vref [out.html]

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::config::{
    BracketBalanceConfig, CasingConfig, MixedScriptConfig, ProportionalityConfig,
    PunctOnlyTokenConfig, PunctuationAdjacencyConfig, PunctuationSpacingConfig,
    RepeatedCharacterRunConfig,
};
use ssc_core::rule::StatefulRule;
use ssc_core::signals::lexical::{PunctOnlyToken, RepeatedCharacterRun};
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::signals::punctuation::{PunctuationAdjacencyAnomaly, PunctuationSpacingAnomaly};
use ssc_core::{
    BookId, Config, Finding, FindingArgs, LengthRatioScope, RuleId, VerseMap, analyze,
    analyze_with_config,
};

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::load_corpus;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (target_dir, source_dir, z_threshold) = match args.as_slice() {
        // Dump a corpus as `{ "GEN 1:1": text, … }` JSON on stdout (ad-hoc).
        [flag, t] if flag == "--json" => {
            let map: BTreeMap<String, String> = load_corpus(Path::new(t))
                .iter()
                .map(|(sid, text)| (sid.to_string(), text.clone()))
                .collect();
            println!("{}", serde_json::to_string(&map).unwrap());
            return;
        }
        // Redundant-ZWSP report (ADR 0027): count the deterministic duplicate-run
        // findings the default-on rule emits, and confirm hygiene flags no U+200B.
        [flag, t] if flag == "--zwsp" => {
            zwsp_calib(Path::new(t));
            return;
        }
        // Punctuation adjacency calibration (ADR 0024): the rule is default-on;
        // report its score distribution at floor 0.
        [flag, t] if flag == "--punct" => {
            punct_calib(Path::new(t));
            return;
        }
        // Punctuation spacing calibration (ADR 0029, 0050): score distribution
        // at floor 0, a per-mark two-factor decomposition (dominance × rarity),
        // and a recurrence-knee sweep. Trailing args are the `k` values to sweep
        // (default 8 12 16 24 32 48).
        [flag, t, ks @ ..] if flag == "--spacing" => {
            let sweep: Vec<f32> = if ks.is_empty() {
                vec![8.0, 12.0, 16.0, 24.0, 32.0, 48.0]
            } else {
                ks.iter().map(|s| s.parse().expect("minority_recurrence_k")).collect()
            };
            spacing_calib(Path::new(t), &sweep);
            return;
        }
        // Repeated-character-run signal exploration: per-finding TSV with the
        // candidate corpus-relative signals (word frequency, run recurrence,
        // corpus base rate) on stdout; per-corpus summary on stderr.
        [flag, t] if flag == "--repeat" => {
            repeat_calib(Path::new(t), RepeatedCharacterRunConfig::default());
            return;
        }
        // Parameter sweep: override the two evidence factors while always
        // reporting at floor zero. The third knob stays a surfacing policy, not
        // part of the score sweep.
        [flag, t, rate, word_k] if flag == "--repeat" => {
            repeat_calib(
                Path::new(t),
                RepeatedCharacterRunConfig {
                    convention_rate_per_10k: rate.parse().expect("repeat convention rate"),
                    word_recurrence_k: word_k.parse().expect("repeat word recurrence K"),
                    ..Default::default()
                },
            );
            return;
        }
        // Punct-only-token signal exploration: per-finding TSV (chunk, its
        // corpus-wide recurrence as a flagged pattern, context) on stdout;
        // per-corpus summary on stderr.
        [flag, t] if flag == "--punct-only" => {
            punct_only_calib(Path::new(t));
            return;
        }
        // Fleet survey: every rule over every corpus in a vref directory,
        // emission floors zeroed so score histograms show the sub-floor mass;
        // writes a self-contained HTML report (Observable Plot).
        [flag, dir, rest @ ..] if flag == "--fleet" && rest.len() <= 1 => {
            let out = rest
                .first()
                .map(|s| Path::new(s).to_path_buf())
                .unwrap_or_else(|| Path::new("target/fleet-report.html").to_path_buf());
            fleet(Path::new(dir), &out);
            return;
        }
        [t] => {
            batch(Path::new(t));
            return;
        }
        [t, s] => (t, s, ProportionalityConfig::default().z_threshold),
        [t, s, z] => (t, s, z.parse().expect("z threshold")),
        _ => {
            eprintln!("usage: calibrate <target-vref-file> [<source-vref-file> [z]]");
            std::process::exit(2);
        }
    };

    let target = load_corpus(Path::new(target_dir));
    let source = load_corpus(Path::new(source_dir));
    eprintln!(
        "target {} verses, source {} verses",
        target.len(),
        source.len()
    );

    let rule = ProjectLengthRatio {
        cfg: ProportionalityConfig {
            z_threshold,
            ..Default::default()
        },
    };
    let t0 = std::time::Instant::now();
    let findings = rule.judge(&rule.reduce(&ssc_core::verse::by_book(&target), Some(&source), None).0, &ssc_core::verse::by_book(&target), None, None);
    eprintln!("proportionality check: {:?}", t0.elapsed());

    let mut per_book: BTreeMap<BookId, usize> = BTreeMap::new();
    for f in &findings {
        *per_book.entry(f.sid.book).or_default() += 1;
    }

    println!("total findings: {}", findings.len());
    println!("\nper book:");
    for (book, n) in &per_book {
        println!("  {book} {n}");
    }

    let mut by_z: Vec<_> = findings.iter().collect();
    by_z.sort_by(|a, b| {
        let za = z_of(a).abs();
        let zb = z_of(b).abs();
        zb.partial_cmp(&za).unwrap()
    });
    println!("\ntop 15 by |z|:");
    print_findings(&target, by_z.iter().take(15).copied());
    println!("\nborderline 15 (lowest flagged |z|):");
    print_findings(&target, by_z.iter().rev().take(15).copied());
}

fn print_findings<'a>(
    target: &VerseMap,
    findings: impl Iterator<Item = &'a ssc_core::Finding>,
) {
    for f in findings {
        let Some(FindingArgs::LengthRatio { ratio_pct, scope }) = f.args else {
            continue;
        };
        let robust_z = scope_z(&scope);
        let text = &target[&f.sid];
        let preview: String = text.chars().take(60).collect();
        println!(
            "  {:<10} z={:+7.1} ratio={:6.0}% | {}",
            f.sid.to_string(),
            robust_z,
            ratio_pct,
            preview
        );
    }
}

/// Per-verse batch over one corpus with the shipped defaults: counts per
/// rule, worst book per rule, and a few sample slices per rule.
fn batch(dir: &Path) {
    let t0 = std::time::Instant::now();
    let target = load_corpus(dir);
    let t_load = t0.elapsed();
    let t1 = std::time::Instant::now();
    let findings = analyze(&target, None);
    let t_analyze = t1.elapsed();
    eprintln!(
        "{} verses | load+parse {:?} | analyze {:?} ({:.1} µs/verse)",
        target.len(),
        t_load,
        t_analyze,
        t_analyze.as_secs_f64() * 1e6 / target.len() as f64
    );

    let mut by_rule: BTreeMap<RuleId, Vec<&ssc_core::Finding>> = BTreeMap::new();
    for f in &findings {
        by_rule.entry(f.code).or_default().push(f);
    }
    println!("total findings: {}\n", findings.len());
    for (rule, fs) in &by_rule {
        let mut per_book: BTreeMap<BookId, usize> = BTreeMap::new();
        for f in fs {
            *per_book.entry(f.sid.book).or_default() += 1;
        }
        let (worst_book, worst) = per_book
            .iter()
            .max_by_key(|&(_, n)| *n)
            .map(|(b, n)| (*b, *n))
            .unwrap();
        println!("{rule}: {} (worst book {worst_book}: {worst})", fs.len());
        for f in fs.iter().take(5) {
            let text = &target[&f.sid];
            let slice: String = f.range.slice(text).chars().take(40).collect();
            let ctx_start = text[..f.range.start]
                .char_indices()
                .rev()
                .nth(19)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let ctx: String = text[ctx_start..].chars().take(60).collect();
            println!("    {:<10} [{slice}] …{ctx}", f.sid.to_string());
        }
    }
}

/// One finding sampled for the fleet report: enough to render a "what this
/// looks like in real text" row without shipping the corpus.
struct FleetSample {
    corpus: String,
    sid: String,
    score: Option<f32>,
    slice: String,
    ctx: String,
}

/// Per-corpus tally from one fleet pass.
struct FleetRow {
    id: String,
    verses: usize,
    chars: usize,
    /// Findings the shipped floor would show the user, per `RuleId::ALL` slot.
    surfaced: Vec<u32>,
    /// All scored sites at floor zero (== `surfaced` for unscored rules).
    sites: Vec<u32>,
    /// Score histogram per rule, aligned with that rule's bucket edges.
    hists: Vec<Vec<u64>>,
    /// ≤ 2 best surfaced samples per rule (corpus diversity cap).
    samples: Vec<Vec<FleetSample>>,
}

/// Fleet survey: every rule over every vref corpus in `dir`, with all
/// emission floors zeroed so the score histograms include the sub-floor mass
/// the shipped floors suppress. Writes a self-contained HTML report to `out`
/// (per-corpus rates, per-rule score distributions with the shipped floor
/// marked, and sample findings).
fn fleet(dir: &Path, out: &Path) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;

    let n_rules = RuleId::ALL.len();

    // Measurement config: everything on, every floor at zero. Surfaced-vs-not
    // is then recomputed against the shipped floors below, so one pass yields
    // both the full distribution and the user-facing volume.
    let mut cfg = Config::all();
    cfg.bracket_balance.emit_score_min = 0.0;
    cfg.casing.emit_score_min = 0.0;
    cfg.punctuation_adjacency.emit_score_min = 0.0;
    cfg.punctuation_spacing.emit_score_min = 0.0;
    cfg.repeated_character_run.emit_score_min = 0.0;
    cfg.punct_only_token.emit_score_min = 0.0;
    cfg.mixed_script.emit_score_min = 0.0;

    let floors: Vec<Option<f32>> = RuleId::ALL
        .iter()
        .map(|id| match id {
            RuleId::BracketBalance => Some(BracketBalanceConfig::default().emit_score_min),
            RuleId::SentenceInitialLowercase => Some(CasingConfig::default().emit_score_min),
            RuleId::PunctuationAdjacencyAnomaly => {
                Some(PunctuationAdjacencyConfig::default().emit_score_min)
            }
            RuleId::PunctuationSpacingAnomaly => {
                Some(PunctuationSpacingConfig::default().emit_score_min)
            }
            RuleId::RepeatedCharacterRun => {
                Some(RepeatedCharacterRunConfig::default().emit_score_min)
            }
            RuleId::PunctOnlyToken => Some(PunctOnlyTokenConfig::default().emit_score_min),
            RuleId::MixedScriptInToken => Some(MixedScriptConfig::default().emit_score_min),
            _ => None,
        })
        .collect();

    // Histogram bucket edges per rule: 40 uniform buckets plus the shipped
    // floor as an extra edge, so below-floor vs surfaced is exact per bucket.
    let edges: Vec<Vec<f32>> = floors
        .iter()
        .map(|floor| {
            let mut e: Vec<f32> = (0..=40).map(|i| i as f32 / 40.0).collect();
            if let Some(f) = floor
                && e.iter().all(|x| (x - f).abs() > 1e-6)
            {
                e.push(*f);
                e.sort_by(|a, b| a.partial_cmp(b).unwrap());
            }
            e
        })
        .collect();

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let rows: Vec<FleetRow> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let verses = map.len();
            let chars = map.values().map(|t| t.chars().count()).sum();
            let findings = if verses == 0 {
                Vec::new()
            } else {
                analyze_with_config(&map, None, &cfg)
            };

            let mut surfaced = vec![0u32; n_rules];
            let mut sites = vec![0u32; n_rules];
            let mut hists: Vec<Vec<u64>> =
                edges.iter().map(|e| vec![0u64; e.len() - 1]).collect();
            let mut samples: Vec<Vec<FleetSample>> = (0..n_rules).map(|_| Vec::new()).collect();

            for f in &findings {
                let ri = RuleId::ALL.iter().position(|r| *r == f.code).unwrap();
                sites[ri] += 1;
                if let Some(s) = f.score {
                    let e = &edges[ri];
                    let b = e.partition_point(|x| *x <= s.clamp(0.0, 0.999_999)) - 1;
                    hists[ri][b.min(e.len() - 2)] += 1;
                }
                let shown = f.score.is_none_or(|s| s >= floors[ri].unwrap_or(0.0));
                if !shown {
                    continue;
                }
                surfaced[ri] += 1;
                // Keep the 2 best surfaced samples per rule per corpus.
                let sv = &mut samples[ri];
                let better_than = |x: &FleetSample| {
                    f.score.unwrap_or(0.0) > x.score.unwrap_or(f32::INFINITY)
                };
                if sv.len() < 2 || sv.iter().any(better_than) {
                    let text = &map[&f.sid];
                    let sample = FleetSample {
                        corpus: id.clone(),
                        sid: f.sid.to_string(),
                        score: f.score,
                        slice: display_slice(f.range.slice(text), 24),
                        ctx: fleet_context(text, f.range.start),
                    };
                    if sv.len() < 2 {
                        sv.push(sample);
                    } else if let Some((i, _)) = sv.iter().enumerate().min_by(|a, b| {
                        a.1.score
                            .unwrap_or(0.0)
                            .partial_cmp(&b.1.score.unwrap_or(0.0))
                            .unwrap()
                    }) {
                        sv[i] = sample;
                    }
                }
            }

            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 100 == 0 {
                eprintln!("  …{n}/{total}");
            }
            FleetRow { id, verses, chars, surfaced, sites, hists, samples }
        })
        .collect();

    // Fleet-wide aggregates.
    let mut sites_total = vec![0u64; n_rules];
    let mut surfaced_total = vec![0u64; n_rules];
    let mut corpora_hit = vec![0u32; n_rules];
    for row in &rows {
        for ri in 0..n_rules {
            sites_total[ri] += row.sites[ri] as u64;
            surfaced_total[ri] += row.surfaced[ri] as u64;
            corpora_hit[ri] += (row.surfaced[ri] > 0) as u32;
        }
    }

    let corpora_json: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id, "v": r.verses, "ch": r.chars, "c": r.surfaced,
            })
        })
        .collect();

    let mut hist_total: Vec<Vec<u64>> = edges.iter().map(|e| vec![0u64; e.len() - 1]).collect();
    let mut samples_all: Vec<Vec<FleetSample>> = (0..n_rules).map(|_| Vec::new()).collect();
    for row in rows {
        for (ri, h) in row.hists.into_iter().enumerate() {
            for (b, n) in h.into_iter().enumerate() {
                hist_total[ri][b] += n;
            }
        }
        for (ri, s) in row.samples.into_iter().enumerate() {
            samples_all[ri].extend(s);
        }
    }
    for sv in &mut samples_all {
        sv.sort_by(|a, b| {
            b.score
                .unwrap_or(0.0)
                .partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap()
                .then_with(|| a.corpus.cmp(&b.corpus))
                .then_with(|| a.sid.cmp(&b.sid))
        });
        sv.truncate(8);
    }

    let rules_json: Vec<serde_json::Value> = RuleId::ALL
        .iter()
        .enumerate()
        .map(|(ri, id)| {
            let scored = hist_total[ri].iter().any(|&n| n > 0);
            let hist: Option<Vec<serde_json::Value>> = scored.then(|| {
                edges[ri]
                    .windows(2)
                    .zip(&hist_total[ri])
                    .map(|(w, &n)| serde_json::json!({"lo": w[0], "hi": w[1], "n": n}))
                    .collect()
            });
            serde_json::json!({
                "code": id.code(),
                "sites": sites_total[ri],
                "surfaced": surfaced_total[ri],
                "corpora_hit": corpora_hit[ri],
                "floor": floors[ri],
                "scored": scored,
                "hist": hist,
                "samples": samples_all[ri].iter().map(|s| serde_json::json!({
                    "corpus": s.corpus, "sid": s.sid, "score": s.score,
                    "slice": s.slice, "ctx": s.ctx,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    let data = serde_json::json!({
        "corpus_count": corpora_json.len(),
        "rules": rules_json,
        "corpora": corpora_json,
    });
    // `</` must not appear inside the inline <script> payload; `<\/` is the
    // same string after JSON unescaping.
    let payload = data.to_string().replace("</", "<\\/");
    let html = include_str!("fleet_report_template.html").replace("__FLEET_DATA__", &payload);
    std::fs::write(out, html).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
}

/// Printable preview of a finding slice: invisibles made visible, whitespace
/// flattened, capped at `max` chars.
fn display_slice(s: &str, max: usize) -> String {
    s.chars()
        .take(max)
        .map(|c| match c {
            '\u{200B}' => '·',
            '\t' | '\n' => ' ',
            c if c.is_control() => '⌧',
            c => c,
        })
        .collect()
}

/// ~20 chars of lead-in plus the finding neighbourhood, for the samples table.
fn fleet_context(text: &str, start: usize) -> String {
    let ctx_start = text[..start]
        .char_indices()
        .rev()
        .nth(19)
        .map(|(i, _)| i)
        .unwrap_or(0);
    display_slice(&text[ctx_start..], 64)
}

/// Redundant-ZWSP report (ADR 0027). The rule is deterministic and default-on, so
/// there is nothing to calibrate — this just reports how much U+200B a corpus
/// carries, how many runs are redundant (doubled U+200B), and confirms
/// deterministic hygiene still flags no U+200B.
fn zwsp_calib(dir: &Path) {
    let target = load_corpus(dir);
    eprintln!("{} verses", target.len());

    let raw: usize = target.values().map(|t| t.matches('\u{200B}').count()).sum();

    let f = analyze(&target, None);
    // Deterministic hygiene must still flag zero U+200B (checked by slicing the
    // char, not just the rule id — hyg.zero-width-misuse still owns BOM/bidi/WJ).
    let hyg_zwsp = f
        .iter()
        .filter(|f| f.code == RuleId::ZeroWidthMisuse)
        .filter(|f| target.get(&f.sid).and_then(|t| t.get(f.range.start..f.range.end)) == Some("\u{200B}"))
        .count();
    let redundant: Vec<_> = f.iter().filter(|f| f.code == RuleId::RedundantZeroWidthSpace).collect();
    println!(
        "U+200B raw={raw}  redundant runs flagged={}  (hyg U+200B flags: {hyg_zwsp}, must be 0)",
        redundant.len()
    );
    for fd in redundant.iter().take(10) {
        if let Some(t) = target.get(&fd.sid) {
            let n = t.get(fd.range.start..fd.range.end).unwrap_or("").matches('\u{200B}').count();
            println!("  {}  run of {n} U+200B", fd.sid);
        }
    }
}

/// Repeated-character-run calibration at floor zero. The scored distribution
/// comes from the production rule; the TSV joins each site to the human-readable
/// recurrence signals needed for typo/convention spot checks.
fn repeat_calib(dir: &Path, cfg: RepeatedCharacterRunConfig) {
    use std::collections::{HashMap, HashSet};

    use ssc_core::grapheme::segment;
    use ssc_core::signals::lexical::scan_repeated_character_run;
    use ssc_core::token::tokenize;

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);

    // Corpus pass for explanatory TSV columns. Production scoring performs its
    // own reduction below; keeping this throwaway join separate prevents the
    // calibration harness from becoming rule infrastructure.
    let mut word_freq: HashMap<String, usize> = HashMap::new();
    let mut cluster_runs: HashMap<String, usize> = HashMap::new();
    let mut cluster_types: HashMap<String, HashSet<String>> = HashMap::new();
    let mut total_tokens = 0usize;
    let mut lexical_units = 0usize;
    let mut tokens_with_run = 0usize;
    let mut graphemes = Vec::new();
    let mut word_graphemes = Vec::new();

    for text in target.values() {
        lexical_units += text.split_whitespace().count();
        let tokens = tokenize(text);
        total_tokens += tokens.len();
        graphemes.clear();
        segment(text, &mut graphemes);
        let raw_runs = scan_repeated_character_run(text, &graphemes);
        for run in &raw_runs {
            *cluster_runs
                .entry(run.slice(text).graphemes_first().to_lowercase())
                .or_default() += 1;
        }
        for tok in tokens {
            let word = tok.span.slice(text);
            if word.chars().take(3).count() < 3 {
                continue;
            }
            let folded = word.to_lowercase();
            word_graphemes.clear();
            segment(&folded, &mut word_graphemes);
            if scan_repeated_character_run(&folded, &word_graphemes).is_empty() {
                continue;
            }
            *word_freq.entry(folded.clone()).or_default() += 1;
            let runs: Vec<_> = raw_runs
                .iter()
                .filter(|run| tok.span.start <= run.start && run.end <= tok.span.end)
                .collect();
            if runs.is_empty() {
                continue;
            }
            tokens_with_run += 1;
            let mut seen = HashSet::new();
            for r in runs {
                // Cluster = first grapheme of the run, folded.
                let cluster = r.slice(text).graphemes_first().to_lowercase();
                if seen.insert(cluster.clone()) {
                    cluster_types
                        .entry(cluster)
                        .or_default()
                        .insert(folded.clone());
                }
            }
        }
    }

    let rule = RepeatedCharacterRun {
        cfg: RepeatedCharacterRunConfig {
            emit_score_min: 0.0,
            ..cfg
        },
    };
    let t0 = std::time::Instant::now();
    let repeat = rule.judge(&rule.reduce(&ssc_core::verse::by_book(&target), None, None).0, &ssc_core::verse::by_book(&target), None, None);
    eprintln!(
        "{corpus}: repeat reduce+judge {:?}; rate={} K={}",
        t0.elapsed(),
        cfg.convention_rate_per_10k,
        cfg.word_recurrence_k
    );
    report_scored("lex.repeated-character-run", &target, &repeat);

    println!(
        "corpus\tsid\tword\tcluster\trun_len\tword_freq\tcluster_runs\tcluster_rate_per_10k\tsame_run_types\ttokens_with_run\tlexical_units\tscore"
    );
    for f in &repeat {
        let text = &target[&f.sid];
        let word = tokenize(text)
            .iter()
            .find(|t| t.span.start <= f.range.start && f.range.end <= t.span.end)
            .map(|t| t.span.slice(text).to_string())
            .unwrap_or_default();
        let run_str = f.range.slice(text);
        graphemes.clear();
        segment(run_str, &mut graphemes);
        let run_len = graphemes.len();
        let cluster = run_str.graphemes_first().to_lowercase();
        let folded = word.to_lowercase();
        println!(
            "{corpus}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.4}\t{}\t{}\t{}\t{:.6}",
            f.sid,
            word,
            cluster,
            run_len,
            word_freq.get(&folded).copied().unwrap_or(0),
            cluster_runs.get(&cluster).copied().unwrap_or(0),
            cluster_runs.get(&cluster).copied().unwrap_or(0) as f64 * 10_000.0
                / lexical_units.max(1) as f64,
            cluster_types.get(&cluster).map(|s| s.len()).unwrap_or(0),
            tokens_with_run,
            lexical_units,
            f.score.unwrap_or(0.0),
        );
    }
    eprintln!(
        "{corpus}: {} verses, {} UAX tokens, {} lexical units, {} tokens-with-run ({:.2}/10k UAX tokens), {} findings",
        target.len(),
        total_tokens,
        lexical_units,
        tokens_with_run,
        tokens_with_run as f64 * 10_000.0 / total_tokens.max(1) as f64,
        repeat.len()
    );
}

trait GraphemesFirst {
    fn graphemes_first(&self) -> &str;
}
impl GraphemesFirst for str {
    fn graphemes_first(&self) -> &str {
        use unicode_segmentation::UnicodeSegmentation;
        self.graphemes(true).next().unwrap_or("")
    }
}

/// Punct-only-token signal exploration: every finding the shipped rule
/// produces, with the exact flagged chunk, how many times that same chunk is
/// flagged corpus-wide (pattern recurrence — the candidate convention signal),
/// and a little context for eyeballing.
fn punct_only_calib(dir: &Path) {
    use std::collections::HashMap;

    use ssc_core::signals::lexical::scan_punct_only_token;

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);

    // Pass 1: count every flagged chunk pattern corpus-wide.
    let mut pattern_count: HashMap<String, usize> = HashMap::new();
    let mut per_verse: Vec<(ssc_core::Sid, Vec<ssc_core::Span>)> = Vec::new();
    for (sid, text) in &target {
        let spans = scan_punct_only_token(text);
        if spans.is_empty() {
            continue;
        }
        for s in &spans {
            *pattern_count.entry(s.slice(text).to_string()).or_default() += 1;
        }
        per_verse.push((*sid, spans));
    }
    let total: usize = pattern_count.values().sum();

    // Pass 2: emit per-finding rows.
    println!("corpus\tsid\tchunk\tchunk_count\ttotal_findings\tverses\tcontext");
    for (sid, spans) in &per_verse {
        let text = &target[sid];
        for s in spans {
            let chunk = s.slice(text);
            let ctx_start = text[..s.start]
                .char_indices()
                .rev()
                .nth(19)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let ctx: String = text[ctx_start..]
                .chars()
                .take(20 + chunk.chars().count() + 20)
                .collect::<String>()
                .replace(['\t', '\n'], " ");
            println!(
                "{corpus}\t{sid}\t{chunk}\t{}\t{total}\t{}\t{ctx}",
                pattern_count[chunk],
                target.len(),
            );
        }
    }
    let mut top: Vec<_> = pattern_count.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    let head: Vec<String> = top.iter().take(6).map(|(p, n)| format!("[{p}]x{n}")).collect();
    eprintln!(
        "{corpus}: {} verses, {total} candidates, {} distinct patterns | {}",
        target.len(),
        pattern_count.len(),
        head.join(" ")
    );

    // Production score distribution at floor 0, and the shipped-floor count.
    let rule = PunctOnlyToken {
        cfg: PunctOnlyTokenConfig { emit_score_min: 0.0, ..Default::default() },
    };
    let findings = rule.judge(&rule.reduce(&ssc_core::verse::by_book(&target), None, None).0, &ssc_core::verse::by_book(&target), None, None);
    report_scored("lex.punct-only-token", &target, &findings);
    let shipped = PunctOnlyTokenConfig::default().emit_score_min;
    let surfaced = findings
        .iter()
        .filter(|f| f.score.unwrap_or(0.0) >= shipped)
        .count();
    eprintln!("{corpus}: surfaced at shipped floor {shipped}: {surfaced}");
}

/// Punctuation adjacency calibration (ADR 0024) at floor 0.
fn punct_calib(dir: &Path) {
    let target = load_corpus(dir);
    eprintln!("{} verses", target.len());
    let rule = PunctuationAdjacencyAnomaly {
        cfg: PunctuationAdjacencyConfig { emit_score_min: 0.0, ..Default::default() },
    };
    let t0 = std::time::Instant::now();
    let findings = rule.judge(&rule.reduce(&ssc_core::verse::by_book(&target), None, None).0, &ssc_core::verse::by_book(&target), None, None);
    eprintln!("punct reduce+judge: {:?}", t0.elapsed());
    report_scored("punct.adjacency-anomaly", &target, &findings);

    // How many the shipped default config surfaces (default-on rule).
    let shipped = analyze_with_config(&target, None, &Config::v1_defaults());
    let shipped_n = shipped
        .iter()
        .filter(|f| f.code == RuleId::PunctuationAdjacencyAnomaly)
        .count();
    println!("\nshipped default surfaces: {shipped_n}");
}

/// One mark's corpus-wide spacing verdict, as recovered from the rule's own
/// floor-0 findings: the raw split, the (flaggable) minority count, and the
/// dominance factor (ADR 0029) — the k-independent half of the two-factor score.
struct MarkRow {
    mark: char,
    spaced: u64,
    attached: u64,
    minority: u64,
    dominance: f64,
}

/// Punctuation spacing calibration (ADR 0029 dominance × ADR 0050 recurrence
/// rarity). Prints the production score distribution, the per-mark two-factor
/// decomposition, and a recurrence-knee (`minority_recurrence_k`) sweep with the
/// surfaced volume each `k`/floor pair would emit. The dominance factor is
/// k-independent, so it is recovered once from a wide-knee floor-0 run (where
/// `rarity ≈ 1 ⇒ score ≈ dominance`) and the sweep is then analytic.
fn spacing_calib(dir: &Path, sweep: &[f32]) {
    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);
    eprintln!("{corpus}: {} verses", target.len());
    let books = ssc_core::verse::by_book(&target);

    // Production distribution at the *shipped* score (default k, floor 0).
    let rule = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig { emit_score_min: 0.0, ..Default::default() },
    };
    let t0 = std::time::Instant::now();
    let findings = rule.judge(&rule.reduce(&books, None, None).0, &books, None, None);
    eprintln!("spacing reduce+judge: {:?}", t0.elapsed());
    report_scored("punct.spacing-anomaly", &target, &findings);

    // Recover per-mark (spaced, attached, dominance) from a wide-knee floor-0
    // run: with k huge the rarity factor is ≈1, so each finding's score is the
    // dominance, and its args carry the exact corpus-wide split. One finding per
    // minority-form occurrence ⇒ dedup by mark.
    let wide = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig {
            emit_score_min: 0.0,
            minority_recurrence_k: 1.0e9,
            ..Default::default()
        },
    };
    let wf = wide.judge(&wide.reduce(&books, None, None).0, &books, None, None);
    let mut rows: BTreeMap<char, MarkRow> = BTreeMap::new();
    for f in &wf {
        if let Some(FindingArgs::SpacingConvention { mark, spaced, attached }) = f.args {
            rows.entry(mark).or_insert_with(|| MarkRow {
                mark,
                spaced: spaced as u64,
                attached: attached as u64,
                minority: (spaced as u64).min(attached as u64),
                dominance: f.score.unwrap_or(0.0) as f64,
            });
        }
    }

    println!("\nper-mark two-factor decomposition (dominance × rarity, ADR 0050):");
    println!("  mark  spaced : attached   minority   dominance");
    for r in rows.values() {
        let which = if r.spaced < r.attached { "spaced-min" } else { "attached-min" };
        println!(
            "  {:?}  {:>7} : {:<7}  min={:<6} dom={:.4}  ({which})",
            r.mark, r.spaced, r.attached, r.minority, r.dominance
        );
    }

    // rarity(minority, k) = 1 − min(minority−1, k)/k.
    let rarity = |minority: u64, k: f64| -> f64 {
        (1.0 - ((minority.saturating_sub(1) as f64) / k).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    };

    println!("\nknee sweep — per-mark score = dominance × rarity(minority, k):");
    for r in rows.values() {
        print!("  {:?} (min={:<5} dom={:.3}):", r.mark, r.minority, r.dominance);
        for &k in sweep {
            let s = r.dominance * rarity(r.minority, k as f64);
            print!("  k{:.0}={:.3}", k, s);
        }
        println!();
    }

    // Surfaced volume each (k, floor) pair would emit: a mark contributes all
    // `minority` of its minority-form occurrences iff its score clears the floor.
    println!("\nsurfaced-occurrence volume by k and floor:");
    println!("  {:>6}  {:>10}  {:>10}", "k", "floor 0.50", "floor 0.75");
    for &k in sweep {
        let vol = |floor: f64| -> u64 {
            rows.values()
                .filter(|r| r.dominance * rarity(r.minority, k as f64) >= floor)
                .map(|r| r.minority)
                .sum()
        };
        println!("  {:>6.0}  {:>10}  {:>10}", k, vol(0.5), vol(0.75));
    }

    let mut cfg = Config::v1_defaults();
    cfg.rules.insert(RuleId::PunctuationSpacingAnomaly, true);
    let shipped = analyze_with_config(&target, None, &cfg)
        .iter()
        .filter(|f| f.code == RuleId::PunctuationSpacingAnomaly)
        .count();
    println!(
        "\nshipped default (k {}, floor {}, enabled) surfaces: {shipped}",
        PunctuationSpacingConfig::default().minority_recurrence_k,
        PunctuationSpacingConfig::default().emit_score_min,
    );
}

/// Shared score-distribution report for the corpus-relative rules: total
/// scored sites, how many clear a ladder of floors, and the top/bottom samples
/// with their exact slice and a little context.
fn report_scored(name: &str, target: &VerseMap, findings: &[Finding]) {
    println!("\n{name}: {} scored sites (floor 0)", findings.len());
    for floor in [0.5_f32, 0.7, 0.9, 0.99] {
        let n = findings.iter().filter(|f| f.score.unwrap_or(0.0) >= floor).count();
        println!("  ≥ {floor:>4}: {n}");
    }
    // 10-bucket histogram of evidence scores — shows the sub-floor mass.
    let mut buckets = [0usize; 10];
    for f in findings {
        let s = f.score.unwrap_or(0.0).clamp(0.0, 0.999_999);
        buckets[(s * 10.0) as usize] += 1;
    }
    println!("  score histogram (each row = 0.1 wide):");
    for (i, &n) in buckets.iter().enumerate() {
        let lo = i as f32 / 10.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("    [{lo:.1},{:.1}) {n:>6} {bar}", lo + 0.1);
    }
    let mut by_score: Vec<&Finding> = findings.iter().collect();
    by_score.sort_by(|a, b| b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0)).unwrap());
    println!("  top 10 by score:");
    print_scored(target, by_score.iter().take(10).copied());
    println!("  bottom 5 by score:");
    print_scored(target, by_score.iter().rev().take(5).copied());
}

fn print_scored<'a>(target: &VerseMap, findings: impl Iterator<Item = &'a Finding>) {
    for f in findings {
        let text = &target[&f.sid];
        let slice: String = f.range.slice(text).chars().take(16).collect();
        let ctx_start = text[..f.range.start]
            .char_indices()
            .rev()
            .nth(14)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let ctx: String = text[ctx_start..].chars().take(44).collect();
        println!(
            "    {:<10} score={:.3} [{}] …{}",
            f.sid.to_string(),
            f.score.unwrap_or(0.0),
            slice.replace('\u{200B}', "·"),
            ctx.replace('\u{200B}', "·")
        );
    }
}

fn z_of(f: &ssc_core::Finding) -> f32 {
    match &f.args {
        Some(FindingArgs::LengthRatio { scope, .. }) => scope_z(scope),
        _ => 0.0,
    }
}

/// A single representative z for display: the book z, or the project z for a
/// project-only outlier.
fn scope_z(scope: &LengthRatioScope) -> f32 {
    match scope {
        LengthRatioScope::Book { z } | LengthRatioScope::Project { z } => *z,
        LengthRatioScope::Both { book_z, .. } => *book_z,
    }
}
