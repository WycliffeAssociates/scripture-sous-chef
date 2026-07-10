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
//!   # bracket-balance audit (one corpus): floor-0 scores, per-family tallies,
//!   # sample findings with delimiter inventories:
//!   cargo run --release -p ssc-core --example calibrate -- --bracket corpora/vref/cmncbt.txt
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
use ssc_core::rule::{ProjectRule, StatefulRule};
use ssc_core::signals::casing::{
    FirstCaseExperimental, PosClassExperimental, WordObsExperimental, walk_book_experimental,
};
use ssc_core::signals::bracket_balance::BracketBalance;
use ssc_core::signals::lexical::{PunctOnlyToken, RepeatedCharacterRun};
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::signals::punctuation::{PunctuationAdjacencyAnomaly, PunctuationSpacingAnomaly};
use ssc_core::{
    BookId, BracketMeasure, Config, Finding, FindingArgs, LengthRatioScope, RuleId, VerseMap,
    analyze, analyze_with_config,
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
        // Word-level casing two-factor SPIKE (next-checks-shortlist item 4).
        // `<path>` is a single vref file (detailed per-corpus report) or the
        // `corpora/vref` directory (fleet aggregate). Knobs NOT frozen — this
        // is a calibration spike over `walk_book_experimental`.
        [flag, path] if flag == "--casing" => {
            let p = Path::new(path);
            if p.is_dir() {
                casing_fleet(p);
            } else {
                let corpus = analyze_casing(
                    p.file_stem().unwrap().to_string_lossy().to_string(),
                    &load_corpus(p),
                );
                casing_single_report(&corpus);
            }
            return;
        }
        // Bracket-balance calibration (ADR 0037): floor-0 score distribution,
        // per-family tallies (glyph pair, events, pairing rate, orphan count,
        // long-span count), and ~20 sample orphan findings with their
        // DelimObservation inventories rendered readably.
        [flag, t] if flag == "--bracket" => {
            bracket_calib(Path::new(t));
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

/// Bracket-balance calibration (ADR 0037) at floor 0. Reports the production
/// score distribution, per-family tallies (which delimiter families the corpus
/// uses, how often each pairs, and how many orphans / long spans each yields),
/// and a sample of orphan findings with their full `DelimObservation`
/// inventories rendered readably — the audit view ADR 0037 findings carry.
fn bracket_calib(dir: &Path) {
    use ssc_core::charclass::{bracket_close_of, bracket_open_of, class_of};

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);
    eprintln!("{corpus}: {} verses", target.len());

    // Floor-0 run of the production rule: every orphan and every long-span pair
    // surfaces, so the score distribution shows the sub-floor mass too.
    let rule = BracketBalance {
        cfg: BracketBalanceConfig { emit_score_min: 0.0, ..Default::default() },
    };
    let books = ssc_core::verse::by_book(&target);
    let t0 = std::time::Instant::now();
    let findings = rule.check(&books, None);
    eprintln!("bracket check: {:?}", t0.elapsed());
    report_scored("punct.bracket-balance", &target, &findings);

    // Per-family event tally over the whole corpus, using the same family
    // classification the rule uses (family key = the pair's open glyph).
    #[derive(Default)]
    struct Fam {
        open: char,
        close: char,
        opens: u64,
        closes: u64,
    }
    let mut fams: BTreeMap<char, Fam> = BTreeMap::new();
    for text in target.values() {
        for c in text.chars() {
            if !class_of(c).is_punctuation() {
                continue;
            }
            let (family, is_open, open_glyph, close_glyph) = if let Some(close) = bracket_close_of(c)
            {
                (c, true, c, close)
            } else if let Some(open) = bracket_open_of(c) {
                (open, false, open, c)
            } else {
                continue;
            };
            let e = fams.entry(family).or_default();
            e.open = open_glyph;
            e.close = close_glyph;
            if is_open {
                e.opens += 1;
            } else {
                e.closes += 1;
            }
        }
    }

    // Orphan / long-span counts per family, read off the floor-0 findings. The
    // finding's own slice is the anchor glyph (the orphan for Pairing, the
    // opener for ShortSpan); its family is that glyph or its opener.
    let mut orphans: BTreeMap<char, u64> = BTreeMap::new();
    let mut long_spans: BTreeMap<char, u64> = BTreeMap::new();
    for f in &findings {
        let text = &target[&f.sid];
        let glyph = f.range.slice(text).chars().next().unwrap();
        let family = bracket_close_of(glyph)
            .map(|_| glyph)
            .or_else(|| bracket_open_of(glyph))
            .unwrap_or(glyph);
        match &f.args {
            Some(FindingArgs::BracketWindow { measure: BracketMeasure::Pairing, .. }) => {
                *orphans.entry(family).or_default() += 1;
            }
            Some(FindingArgs::BracketWindow { measure: BracketMeasure::ShortSpan, .. }) => {
                *long_spans.entry(family).or_default() += 1;
            }
            _ => {}
        }
    }

    println!("\nper-family tally (family = open glyph; events = opens + closes):");
    println!(
        "  {:^9} {:>8} {:>7} {:>7} {:>9} {:>7} {:>9}",
        "pair", "events", "opens", "closes", "orphans", "long", "pair_rate"
    );
    let mut rows: Vec<&Fam> = fams.values().collect();
    rows.sort_by_key(|f| std::cmp::Reverse(f.opens + f.closes));
    for f in rows {
        let events = f.opens + f.closes;
        let orph = orphans.get(&f.open).copied().unwrap_or(0);
        let long = long_spans.get(&f.open).copied().unwrap_or(0);
        // Descriptive pairing rate == matched_events / events == (events −
        // orphan_events) / events (each orphan is one unmatched event).
        let rate = (events.saturating_sub(orph)) as f64 / events.max(1) as f64 * 100.0;
        println!(
            "  {}…{}  U+{:04X}  {:>8} {:>7} {:>7} {:>9} {:>7} {:>8.1}%",
            f.open,
            f.close,
            f.open as u32,
            events,
            f.opens,
            f.closes,
            orph,
            long,
            rate
        );
    }

    // ~20 sample orphan findings with their DelimObservation inventories, so
    // the family collisions (quote-role glyphs vs real brackets) are eyeballable.
    println!("\nsample findings (up to 20) with delimiter inventories:");
    let mut samples: Vec<&Finding> = findings.iter().collect();
    samples.sort_by(|a, b| {
        b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0)).unwrap()
    });
    for f in samples.iter().take(20) {
        let text = &target[&f.sid];
        let glyph = f.range.slice(text);
        let (measure, window, majority, total) = match &f.args {
            Some(FindingArgs::BracketWindow { measure, window, majority, total }) => {
                (*measure, window, *majority, *total)
            }
            _ => continue,
        };
        let kind = match measure {
            BracketMeasure::Pairing => "orphan",
            BracketMeasure::ShortSpan => "long-span",
        };
        println!(
            "  {:<10} score={:.3} {kind} [{glyph}] {majority}/{total}",
            f.sid.to_string(),
            f.score.unwrap_or(0.0),
        );
        // Render the inventory compactly: glyph + role + matched flag, grouped
        // so the reviewer sees what surrounds the orphan.
        let inv: String = window
            .iter()
            .map(|o| {
                let role = match o.role {
                    ssc_core::DelimRole::Open => 'o',
                    ssc_core::DelimRole::Close => 'c',
                };
                let mark = if o.matched { '=' } else { '!' };
                format!("{}{role}{mark}", o.glyph)
            })
            .collect::<Vec<_>>()
            .join(" ");
        let inv: String = inv.chars().take(160).collect();
        println!("      inv: {inv}");
    }
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

    // rarity(minority, N, k) with the volume-scaled knee
    // K = k + rate·N/10k (ADR 0050 amendment); `rate` is the shipped default.
    let rate = f64::from(PunctuationSpacingConfig::default().minority_rate_per_10k);
    let rarity = |minority: u64, n: u64, k: f64| -> f64 {
        let knee = k + rate * n as f64 / 10_000.0;
        (1.0 - ((minority.saturating_sub(1) as f64) / knee).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    };

    println!(
        "\nknee sweep — per-mark score = dominance × rarity(minority, K = k + {rate}·N/10k):"
    );
    for r in rows.values() {
        let n = r.spaced + r.attached;
        print!("  {:?} (min={:<5} N={:<7} dom={:.3}):", r.mark, r.minority, n, r.dominance);
        for &k in sweep {
            let s = r.dominance * rarity(r.minority, n, k as f64);
            print!("  k{:.0}={:.3}", k, s);
        }
        println!();
    }

    // Surfaced volume each (k, floor) pair would emit: a mark contributes all
    // `minority` of its minority-form occurrences iff its score clears the floor.
    println!("\nsurfaced-occurrence volume by k and floor (rate {rate}/10k):");
    println!("  {:>6}  {:>10}  {:>10}", "k", "floor 0.50", "floor 0.75");
    for &k in sweep {
        let vol = |floor: f64| -> u64 {
            rows.values()
                .filter(|r| {
                    r.dominance * rarity(r.minority, r.spaced + r.attached, k as f64) >= floor
                })
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
        "\nshipped default (k {}, rate {}/10k, floor {}, enabled) surfaces: {shipped}",
        PunctuationSpacingConfig::default().minority_recurrence_k,
        PunctuationSpacingConfig::default().minority_rate_per_10k,
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

// ═══════════════════════════════════════════════════════════════════════════
// SPIKE — word-level casing two-factor calibration (next-checks-shortlist
// item 4). Consumes `walk_book_experimental`; all estimation/scoring/sweeps
// live here. Knobs NOT frozen. The generative model: an occurrence's case =
// OR(position-forces-upper, word-intrinsically-capitalized); forced uppercase
// is censored from the lexicon (one-directional). Mirrors the `--spacing`
// two-factor decomposition (dominance × recurrence rarity, ADR 0050).
// ═══════════════════════════════════════════════════════════════════════════

/// Confidence for every Wilson bound here (mirrors `CasingConfig::confidence_z`
/// and the frozen spacing/casing default).
const CASING_Z: f64 = 1.96;
/// Reference recurrence knee for the "surfaced" setting used by samples, the
/// hard-vs-soft diff, the noisiest-corpus ranking, and the current-rule fate —
/// the ADR 0050 frozen analog (absolute knee k = 32, floor 0.5).
const REF_ABS_K: f64 = 32.0;
const REF_FLOOR: f64 = 0.5;
/// The current rule's shipped floor (`CasingConfig::default().emit_score_min`).
const CURRENT_FLOOR: f64 = 0.98;

/// Absolute recurrence-knee sweep (occurrence count).
const ABS_KS: [f64; 5] = [8.0, 16.0, 32.0, 64.0, 128.0];
/// Rate-scaled knee sweep (minority per 1k opportunities).
const RATE_KS: [f64; 6] = [0.5, 1.0, 2.0, 4.0, 8.0, 16.0];
/// Emission-floor sweep.
const FLOORS: [f64; 8] = [0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.98];

/// Wilson score-interval lower bound of `k/n` at confidence `z` — the harness
/// copy of `evidence::dominance` (crate-private, so re-derived here for the
/// example, exactly as `--spacing` re-derives its rarity knee). Float `k` so
/// the soft-censoring reweight (a fractional upper count) flows through.
fn wilson_lb(k: f64, n: f64, z: f64) -> f64 {
    if n <= 0.0 {
        return 0.0;
    }
    let p = (k / n).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}

/// Absolute linear recurrence knee (ADR 0050): a hapax minority → 1, fading to
/// 0 past `k`.
fn rarity_abs(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k).max(0.0)).clamp(0.0, 1.0)
}

/// Rate-scaled knee: the same shape with the minority replaced by its rate per
/// 1k opportunities, so a hapax in a large denominator stays ≈1 and the flag
/// boundary is a per-1k minority rate. `k_rate` is that rate cutoff.
fn rarity_rate(minority: u64, opps: u64, k_rate: f64) -> f64 {
    if opps == 0 {
        return 1.0;
    }
    let rate = 1000.0 * minority as f64 / opps as f64;
    (1.0 - (rate / k_rate).max(0.0)).clamp(0.0, 1.0)
}

/// Per-word case profile. Midflow counts define the intrinsic profile (Step 1,
/// forced uppercase censored); forced counts feed the positional channel and
/// the soft-censoring re-estimate.
#[derive(Default, Clone)]
struct WProfile {
    mid_up: u32,
    mid_lo: u32,
    for_up: u32,
    for_lo: u32,
}

impl WProfile {
    fn mid_total(&self) -> u32 {
        self.mid_up + self.mid_lo
    }
    fn for_total(&self) -> u32 {
        self.for_up + self.for_lo
    }
    fn total(&self) -> u32 {
        self.mid_total() + self.for_total()
    }
    fn total_lower(&self) -> u32 {
        self.mid_lo + self.for_lo
    }
    fn total_upper(&self) -> u32 {
        self.mid_up + self.for_up
    }
    /// All-position capitalized: uppercase-majority over *every* occurrence,
    /// not just midflow. The censoring-shadow denominator — a proper noun seen
    /// mostly at forced positions is capitalized in truth even where hard
    /// censoring (midflow-only) can no longer classify it.
    fn is_cap_allpos(&self) -> bool {
        let up = self.total_upper();
        let n = up + self.total_lower();
        n > 0 && wilson_lb(up as f64, n as f64, CASING_Z) > 0.5
    }
    /// Hard-censoring intrinsic dominance of the capitalized form: Wilson lower
    /// bound of `mid_up / mid_total` (midflow only — Step 1's censoring).
    fn cap_dom_hard(&self) -> f64 {
        wilson_lb(self.mid_up as f64, self.mid_total() as f64, CASING_Z)
    }
    fn is_cap_hard(&self) -> bool {
        self.mid_total() > 0 && self.cap_dom_hard() > 0.5
    }
    fn is_lower_hard(&self) -> bool {
        self.mid_total() > 0
            && wilson_lb(self.mid_lo as f64, self.mid_total() as f64, CASING_Z) > 0.5
    }
    /// Soft-censoring capitalized share: forced uppercase re-enters the
    /// intrinsic profile with weight `1 − habit` (a single re-estimate after
    /// the positional habit is known — no full EM). `habit` is the pooled
    /// lexicon-restricted forced-uppercase dominance.
    fn cap_dom_soft(&self, habit: f64) -> f64 {
        let up = self.mid_up as f64 + (1.0 - habit) * self.for_up as f64;
        wilson_lb(up, up + self.mid_lo as f64, CASING_Z)
    }
    fn is_cap_soft(&self, habit: f64) -> bool {
        let up = self.mid_up as f64 + (1.0 - habit) * self.for_up as f64;
        let n = up + self.mid_lo as f64;
        n > 0.0 && self.cap_dom_soft(habit) > 0.5
    }
    fn is_lower_soft(&self, habit: f64) -> bool {
        let up = self.mid_up as f64 + (1.0 - habit) * self.for_up as f64;
        let n = up + self.mid_lo as f64;
        n > 0.0 && wilson_lb(self.mid_lo as f64, n, CASING_Z) > 0.5
    }
}

/// Per-glyph forced-position habit. `naive` pools all words (the current rule's
/// estimate); `lex` restricts to intrinsically-lowercase words (removes the
/// proper-noun confound). Book-initial is keyed `None`.
#[derive(Default, Clone)]
struct GlyphHabit {
    glyph: Option<char>,
    naive_up: u64,
    naive_lo: u64,
    naive_total: u64,
    lex_up: u64,
    lex_total: u64,
}

impl GlyphHabit {
    fn naive_dom(&self) -> f64 {
        wilson_lb(self.naive_up as f64, self.naive_total as f64, CASING_Z)
    }
    fn lex_dom(&self) -> f64 {
        wilson_lb(self.lex_up as f64, self.lex_total as f64, CASING_Z)
    }
}

/// One surfaced site for human review.
struct CasingSample {
    sid: String,
    quad: &'static str,
    word: String,
    glyph: Option<char>,
    dom: f64,
    minority: u64,
    opps: u64,
    rarity: f64,
    score: f64,
    other: f64,
    ctx: String,
}

/// The full per-corpus casing spike result. Aggregate grids/histograms are
/// fleet-summable; `samples` is bounded per corpus.
struct CasingCorpus {
    id: String,
    verses: usize,
    word_types: usize,
    word_tokens: u64,
    approx_bytes: usize,
    habit: Vec<GlyphHabit>,
    pooled_naive_dom: f64,
    pooled_lex_dom: f64,
    cap_types: u64,
    cap_tokens: u64,
    shadow_types: u64,
    shadow_tokens: u64,
    // sweep grids, surfaced distinct sites, [knee][floor]
    abs_grid: Vec<[u64; FLOORS.len()]>,
    rate_grid: Vec<[u64; FLOORS.len()]>,
    // score histogram at the reference knee (all lowercase sites), 40 buckets
    hist: [u64; 40],
    // reference-setting counts (abs k=32, floor 0.5, hard censoring)
    ref_surfaced: u64,
    ref_intrinsic: u64,
    ref_positional: u64,
    ref_both: u64,
    soft_ref_surfaced: u64,
    hard_soft_diff: u64,
    // current-rule (floor 0.98) surfaced set and its fate under the new score
    current_surfaced: u64,
    fate_survive: u64,
    fate_die_rarity: u64,
    fate_die_habit: u64,
    fate_both: u64,
    fate_die_ambiguous: u64,
    samples: Vec<CasingSample>,
}

/// A lowercase site's channel components, resolved from the profile + habit.
struct SiteScore {
    quad: &'static str,
    intr_dom: f64,
    intr_min: u64,
    intr_opp: u64,
    pos_dom: f64,
    pos_min: u64,
    pos_opp: u64,
}

impl SiteScore {
    /// Surfacing score at (knee, k): the max of the applicable channels.
    fn score(&self, rate_knee: bool, k: f64) -> (f64, f64, f64) {
        let r = |min: u64, opp: u64| {
            if rate_knee {
                rarity_rate(min, opp, k)
            } else {
                rarity_abs(min, k)
            }
        };
        let intr = if self.quad == "positional" {
            0.0
        } else {
            self.intr_dom * r(self.intr_min, self.intr_opp)
        };
        let pos = if self.quad == "intrinsic" {
            0.0
        } else {
            self.pos_dom * r(self.pos_min, self.pos_opp)
        };
        (intr.max(pos), intr, pos)
    }
}

/// Classify one lowercase obs into a quadrant and resolve its two-factor
/// components under a given cap/lower classification. Returns `None` when the
/// site is not a clean anomaly candidate (midflow-lowercase of a lower word, or
/// a forced-lowercase of a word the lexicon can't classify).
fn classify_site(
    prof: &WProfile,
    forced: bool,
    glyph_dom: f64,
    cap: bool,
    lower: bool,
) -> Option<SiteScore> {
    // Intrinsic components (valid whenever the word is capitalized).
    let intr_dom = prof.cap_dom_hard(); // dominance factor is always midflow-only
    let intr_min = prof.total_lower() as u64;
    let intr_opp = prof.total() as u64;
    // Positional components (valid whenever the word is forced-position here).
    let pos_min = prof.for_lo as u64;
    let pos_opp = prof.for_total() as u64;

    let quad = if cap && forced {
        "both"
    } else if cap {
        "intrinsic"
    } else if forced && lower {
        "positional"
    } else {
        return None;
    };
    Some(SiteScore {
        quad,
        intr_dom,
        intr_min,
        intr_opp,
        pos_dom: glyph_dom,
        pos_min,
        pos_opp,
    })
}

/// Run the full word-level casing spike over one corpus.
fn analyze_casing(id: String, map: &VerseMap) -> CasingCorpus {
    use std::collections::HashMap;

    // ── Pass 0: word observations, per book (cross-seam state per `walk_book`).
    let books = ssc_core::verse::by_book(map);
    let mut obs: Vec<WordObsExperimental> = Vec::new();
    for verses in books.values() {
        obs.extend(walk_book_experimental(verses));
    }

    // ── Pass 1: intern words, build per-word profiles and cardinality.
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut prof: Vec<WProfile> = Vec::new();
    let mut key_of: Vec<u32> = Vec::with_capacity(obs.len());
    let mut approx_bytes = 0usize;
    for o in &obs {
        let text = &map[&o.sid];
        let key = text[o.start as usize..o.end as usize].to_lowercase();
        let id = *ids.entry(key.clone()).or_insert_with(|| {
            approx_bytes += key.len() + std::mem::size_of::<WProfile>() + 24;
            prof.push(WProfile::default());
            (prof.len() - 1) as u32
        });
        key_of.push(id);
        let p = &mut prof[id as usize];
        let forced = !matches!(o.pos, PosClassExperimental::Midflow);
        match (forced, o.case) {
            (false, FirstCaseExperimental::Upper) => p.mid_up += 1,
            (false, FirstCaseExperimental::Lower) => p.mid_lo += 1,
            (true, FirstCaseExperimental::Upper) => p.for_up += 1,
            (true, FirstCaseExperimental::Lower) => p.for_lo += 1,
            (_, FirstCaseExperimental::Uncased) => {}
        }
    }

    // ── Pass 2: per-glyph habit (naive over all words, lexicon-restricted over
    // intrinsically-lowercase words — hard classification).
    let mut habit_map: HashMap<Option<char>, GlyphHabit> = HashMap::new();
    for (i, o) in obs.iter().enumerate() {
        let key = match o.pos {
            PosClassExperimental::Midflow => continue,
            PosClassExperimental::BookInitial => None,
            PosClassExperimental::ForcedAfterTerminal(g) => Some(g),
        };
        let p = &prof[key_of[i] as usize];
        let h = habit_map.entry(key).or_insert_with(|| GlyphHabit {
            glyph: key,
            ..Default::default()
        });
        h.naive_total += 1;
        match o.case {
            FirstCaseExperimental::Upper => h.naive_up += 1,
            FirstCaseExperimental::Lower => h.naive_lo += 1,
            FirstCaseExperimental::Uncased => {}
        }
        if p.is_lower_hard() {
            h.lex_total += 1;
            if o.case == FirstCaseExperimental::Upper {
                h.lex_up += 1;
            }
        }
    }
    let mut habit: Vec<GlyphHabit> = habit_map.into_values().collect();
    habit.sort_by_key(|h| std::cmp::Reverse(h.naive_total));
    let habit_dom: HashMap<Option<char>, f64> =
        habit.iter().map(|h| (h.glyph, h.lex_dom())).collect();

    // Pooled habits (naive vs lexicon over ALL forced positions) and the soft
    // reweight base.
    let (mut n_up, mut n_tot, mut l_up, mut l_tot) = (0u64, 0u64, 0u64, 0u64);
    for h in &habit {
        n_up += h.naive_up;
        n_tot += h.naive_total;
        l_up += h.lex_up;
        l_tot += h.lex_total;
    }
    let pooled_naive_dom = wilson_lb(n_up as f64, n_tot as f64, CASING_Z);
    let pooled_lex_dom = wilson_lb(l_up as f64, l_tot as f64, CASING_Z);
    let habit_pooled = pooled_lex_dom; // soft reweight base (1 − habit)

    // ── Cardinality + censoring shadow (over hard-cap word types/tokens).
    let mut cap_types = 0u64;
    let mut cap_tokens = 0u64;
    let mut shadow_types = 0u64;
    let mut shadow_tokens = 0u64;
    for p in &prof {
        // All-position capitalized words (not the hard-censored view) so the
        // shadow can count the words hard censoring loses.
        if p.is_cap_allpos() {
            let up = p.total_upper() as u64;
            cap_types += 1;
            cap_tokens += up;
            // ≥90% of the word's uppercase evidence is forced-position.
            if up > 0 && p.for_up as f64 / up as f64 >= 0.9 {
                shadow_types += 1;
                shadow_tokens += up;
            }
        }
    }

    // ── Judge pass: classify every lowercase site, sweep, histogram, samples,
    // hard-vs-soft, current-rule fate.
    let mut abs_grid = vec![[0u64; FLOORS.len()]; ABS_KS.len()];
    let mut rate_grid = vec![[0u64; FLOORS.len()]; RATE_KS.len()];
    let mut hist = [0u64; 40];
    let mut ref_surfaced = 0u64;
    let (mut ref_intrinsic, mut ref_positional, mut ref_both) = (0u64, 0u64, 0u64);
    let mut soft_ref_surfaced = 0u64;
    let mut hard_soft_diff = 0u64;
    let mut current_surfaced = 0u64;
    let (
        mut fate_survive,
        mut fate_die_rarity,
        mut fate_die_habit,
        mut fate_both,
        mut fate_die_ambiguous,
    ) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut samples: Vec<CasingSample> = Vec::new();

    for (i, o) in obs.iter().enumerate() {
        if o.case != FirstCaseExperimental::Lower {
            continue;
        }
        let p = &prof[key_of[i] as usize];
        let (forced, glyph_key) = match o.pos {
            PosClassExperimental::Midflow => (false, None),
            PosClassExperimental::BookInitial => (true, None),
            PosClassExperimental::ForcedAfterTerminal(g) => (true, Some(g)),
        };
        let glyph_dom = if forced {
            habit_dom.get(&glyph_key).copied().unwrap_or(0.0)
        } else {
            0.0
        };

        // Current-rule fate: the live rule surfaces bare-terminal (not
        // book-initial) lowercase sites whose per-glyph NAIVE dominance clears
        // 0.98. Track each such site's new destiny.
        if let PosClassExperimental::ForcedAfterTerminal(g) = o.pos {
            let naive = habit
                .iter()
                .find(|h| h.glyph == Some(g))
                .map(|h| h.naive_dom())
                .unwrap_or(0.0);
            if naive >= CURRENT_FLOOR {
                current_surfaced += 1;
                let cap = p.is_cap_hard();
                if cap {
                    fate_both += 1;
                } else if p.is_lower_hard() {
                    let pos = glyph_dom * rarity_abs(p.for_lo as u64, REF_ABS_K);
                    if pos >= REF_FLOOR {
                        fate_survive += 1;
                    } else if glyph_dom < REF_FLOOR {
                        fate_die_habit += 1;
                    } else {
                        fate_die_rarity += 1;
                    }
                } else {
                    fate_die_ambiguous += 1;
                }
            }
        }

        // Hard-censoring classification and site.
        let cap = p.is_cap_hard();
        let lower = p.is_lower_hard();
        let site = classify_site(p, forced, glyph_dom, cap, lower);

        // Soft-censoring classification (for the hard-vs-soft diff only).
        let cap_s = p.is_cap_soft(habit_pooled);
        let lower_s = p.is_lower_soft(habit_pooled);
        let site_s = classify_site(p, forced, glyph_dom, cap_s, lower_s);
        let soft_surf = site_s
            .as_ref()
            .map(|s| s.score(false, REF_ABS_K).0 >= REF_FLOOR)
            .unwrap_or(false);

        let Some(site) = site else {
            if soft_surf {
                hard_soft_diff += 1; // surfaced under soft only
            }
            continue;
        };

        let (ref_score, intr_s, pos_s) = site.score(false, REF_ABS_K);
        hist[((ref_score.clamp(0.0, 0.999_999)) * 40.0) as usize] += 1;
        let hard_surf = ref_score >= REF_FLOOR;
        if hard_surf != soft_surf {
            hard_soft_diff += 1;
        }
        if soft_surf {
            soft_ref_surfaced += 1;
        }

        // Sweep grids (skip sites that can never clear the lowest floor —
        // dominance caps the score, since rarity ≤ 1).
        let max_dom = site.intr_dom.max(site.pos_dom);
        if max_dom >= FLOORS[0] {
            for (ki, &k) in ABS_KS.iter().enumerate() {
                let s = site.score(false, k).0;
                for (fi, &fl) in FLOORS.iter().enumerate() {
                    if s >= fl {
                        abs_grid[ki][fi] += 1;
                    }
                }
            }
            for (ki, &k) in RATE_KS.iter().enumerate() {
                let s = site.score(true, k).0;
                for (fi, &fl) in FLOORS.iter().enumerate() {
                    if s >= fl {
                        rate_grid[ki][fi] += 1;
                    }
                }
            }
        }

        if hard_surf {
            ref_surfaced += 1;
            match site.quad {
                "intrinsic" => ref_intrinsic += 1,
                "positional" => ref_positional += 1,
                _ => ref_both += 1,
            }
            // Bounded sample capture: the two-factor components of the surfacing
            // channel (the louder of the two for a both-site).
            if samples.len() < 400 {
                let text = &map[&o.sid];
                let word = text[o.start as usize..o.end as usize].to_string();
                let (dom, min, opp, other) = if pos_s >= intr_s {
                    (site.pos_dom, site.pos_min, site.pos_opp, intr_s)
                } else {
                    (site.intr_dom, site.intr_min, site.intr_opp, pos_s)
                };
                let rar = if opp > 0 && ref_score > 0.0 { ref_score / dom.max(1e-9) } else { 1.0 };
                samples.push(CasingSample {
                    sid: o.sid.to_string(),
                    quad: site.quad,
                    word,
                    glyph: glyph_key,
                    dom,
                    minority: min,
                    opps: opp,
                    rarity: rar.min(1.0),
                    score: ref_score,
                    other,
                    ctx: casing_ctx(text, o.start as usize, o.end as usize),
                });
            }
        }
    }

    CasingCorpus {
        id,
        verses: map.len(),
        word_types: prof.len(),
        word_tokens: obs.len() as u64,
        approx_bytes,
        habit,
        pooled_naive_dom,
        pooled_lex_dom,
        cap_types,
        cap_tokens,
        shadow_types,
        shadow_tokens,
        abs_grid,
        rate_grid,
        hist,
        ref_surfaced,
        ref_intrinsic,
        ref_positional,
        ref_both,
        soft_ref_surfaced,
        hard_soft_diff,
        current_surfaced,
        fate_survive,
        fate_die_rarity,
        fate_die_habit,
        fate_both,
        fate_die_ambiguous,
        samples,
    }
}

/// ~24 chars of lead-in plus the flagged word neighbourhood, whitespace
/// flattened — the review context for a casing sample.
fn casing_ctx(text: &str, start: usize, end: usize) -> String {
    let ctx_start = text[..start]
        .char_indices()
        .rev()
        .nth(23)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let ctx_end = text[end..]
        .char_indices()
        .nth(24)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[ctx_start..ctx_end].replace(['\t', '\n'], " ")
}

fn glyph_str(g: Option<char>) -> String {
    match g {
        None => "^book/∅".to_string(),
        Some(c) => format!("{c:?}"),
    }
}

/// Detailed single-corpus casing report.
fn casing_single_report(c: &CasingCorpus) {
    println!("=== casing spike: {} ({} verses) ===", c.id, c.verses);
    println!(
        "word types {}  tokens {}  approx table bytes {} ({:.1} B/type)",
        c.word_types,
        c.word_tokens,
        c.approx_bytes,
        c.approx_bytes as f64 / c.word_types.max(1) as f64
    );

    println!("\nnaive vs lexicon-restricted positional habit (proper-noun confound):");
    println!(
        "  pooled: naive_dom={:.4}  lex_dom={:.4}  delta={:+.4}",
        c.pooled_naive_dom,
        c.pooled_lex_dom,
        c.pooled_naive_dom - c.pooled_lex_dom
    );
    println!(
        "  {:<10} {:>8} {:>8} {:>8} {:>9} {:>9} {:>8}",
        "glyph", "events", "naive%", "n_dom", "lex_events", "lex_dom", "delta"
    );
    for h in c.habit.iter().take(12) {
        println!(
            "  {:<10} {:>8} {:>7.1}% {:>8.4} {:>9} {:>9.4} {:>+8.4}",
            glyph_str(h.glyph),
            h.naive_total,
            100.0 * h.naive_up as f64 / h.naive_total.max(1) as f64,
            h.naive_dom(),
            h.lex_total,
            h.lex_dom(),
            h.naive_dom() - h.lex_dom(),
        );
    }

    println!("\ncensoring shadow (cap words whose upper evidence is ≥90% forced):");
    println!(
        "  cap types {}  shadow types {} ({:.1}%)  |  cap upper-tokens {}  shadow tokens {} ({:.1}%)",
        c.cap_types,
        c.shadow_types,
        100.0 * c.shadow_types as f64 / c.cap_types.max(1) as f64,
        c.cap_tokens,
        c.shadow_tokens,
        100.0 * c.shadow_tokens as f64 / c.cap_tokens.max(1) as f64,
    );

    println!(
        "\nreference setting (abs k=32, floor 0.5, hard): surfaced {} (intrinsic {}, positional {}, both {})",
        c.ref_surfaced, c.ref_intrinsic, c.ref_positional, c.ref_both
    );
    println!(
        "  soft-censoring surfaced {}  |  hard-vs-soft differing verdicts {}",
        c.soft_ref_surfaced, c.hard_soft_diff
    );

    println!("\ncurrent rule (floor 0.98) surfaced {} sites — fate under new positional score:", c.current_surfaced);
    println!(
        "  survive {}  die(recurrence) {}  die(habit=proper-noun confound) {}  both-quadrant {}  die(word unclassifiable) {}",
        c.fate_survive, c.fate_die_rarity, c.fate_die_habit, c.fate_both, c.fate_die_ambiguous
    );

    print_casing_sweep(c);
    print_casing_hist(&c.hist);

    println!("\ntop surfaced samples (ref knee):");
    let mut s: Vec<&CasingSample> = c.samples.iter().collect();
    s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    print_casing_samples(s.iter().take(20).copied());
    println!("\nnear-floor surfaced samples:");
    print_casing_samples(s.iter().rev().take(10).copied());
}

fn print_casing_samples<'a>(it: impl Iterator<Item = &'a CasingSample>) {
    for s in it {
        println!(
            "  {:<10} {:<10} [{}] g={} dom={:.3} min={} opp={} rar={:.3} score={:.3}{} | {}",
            s.sid,
            s.quad,
            s.word,
            glyph_str(s.glyph),
            s.dom,
            s.minority,
            s.opps,
            s.rarity,
            s.score,
            if s.quad == "both" {
                format!(" other={:.3}", s.other)
            } else {
                String::new()
            },
            s.ctx,
        );
    }
}

fn print_casing_sweep(c: &CasingCorpus) {
    println!("\nsurfaced-site volume — absolute knee (rows = k), floors across:");
    print!("  {:>5}", "k\\fl");
    for fl in FLOORS {
        print!("  {fl:>6.2}");
    }
    println!();
    for (ki, &k) in ABS_KS.iter().enumerate() {
        print!("  {k:>5.0}");
        for fi in 0..FLOORS.len() {
            print!("  {:>6}", c.abs_grid[ki][fi]);
        }
        println!();
    }
    println!("surfaced-site volume — rate knee (rows = per-1k minority cutoff):");
    print!("  {:>5}", "r\\fl");
    for fl in FLOORS {
        print!("  {fl:>6.2}");
    }
    println!();
    for (ki, &k) in RATE_KS.iter().enumerate() {
        print!("  {k:>5.1}");
        for fi in 0..FLOORS.len() {
            print!("  {:>6}", c.rate_grid[ki][fi]);
        }
        println!();
    }
}

fn print_casing_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!("\nscore histogram at ref knee ({} sites, 40 buckets):", total);
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>7} {bar}", lo + 0.025);
    }
}

/// Fleet aggregate over every vref corpus in `dir`.
fn casing_fleet(dir: &Path) {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rayon::prelude::*;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!("casing fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let corpora: Vec<CasingCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = if map.is_empty() {
                analyze_casing(id, &VerseMap::new())
            } else {
                analyze_casing(id, &map)
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 100 == 0 {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();

    // Fleet aggregates.
    let mut abs_grid = vec![[0u64; FLOORS.len()]; ABS_KS.len()];
    let mut rate_grid = vec![[0u64; FLOORS.len()]; RATE_KS.len()];
    let mut hist = [0u64; 40];
    let (mut ref_surf, mut ref_i, mut ref_p, mut ref_b) = (0u64, 0u64, 0u64, 0u64);
    let (mut soft_surf, mut hs_diff) = (0u64, 0u64);
    let (mut cur_surf, mut f_surv, mut f_rar, mut f_hab, mut f_both, mut f_amb) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut corpora_with_ref = 0u32;
    let mut deltas: Vec<f64> = Vec::new();
    let mut types_v: Vec<usize> = Vec::new();
    let mut bytes_v: Vec<usize> = Vec::new();
    let mut shadow_type_frac: Vec<f64> = Vec::new();
    let mut shadow_tok_frac: Vec<f64> = Vec::new();
    for c in &corpora {
        for ki in 0..ABS_KS.len() {
            for fi in 0..FLOORS.len() {
                abs_grid[ki][fi] += c.abs_grid[ki][fi];
            }
        }
        for ki in 0..RATE_KS.len() {
            for fi in 0..FLOORS.len() {
                rate_grid[ki][fi] += c.rate_grid[ki][fi];
            }
        }
        for b in 0..40 {
            hist[b] += c.hist[b];
        }
        ref_surf += c.ref_surfaced;
        ref_i += c.ref_intrinsic;
        ref_p += c.ref_positional;
        ref_b += c.ref_both;
        soft_surf += c.soft_ref_surfaced;
        hs_diff += c.hard_soft_diff;
        cur_surf += c.current_surfaced;
        f_surv += c.fate_survive;
        f_rar += c.fate_die_rarity;
        f_hab += c.fate_die_habit;
        f_both += c.fate_both;
        f_amb += c.fate_die_ambiguous;
        if c.ref_surfaced > 0 {
            corpora_with_ref += 1;
        }
        if c.cap_types > 0 && c.pooled_naive_dom > 0.0 {
            deltas.push(c.pooled_naive_dom - c.pooled_lex_dom);
        }
        if c.word_types > 0 {
            types_v.push(c.word_types);
            bytes_v.push(c.approx_bytes);
        }
        if c.cap_types > 0 {
            shadow_type_frac.push(c.shadow_types as f64 / c.cap_types as f64);
            if c.cap_tokens > 0 {
                shadow_tok_frac.push(c.shadow_tokens as f64 / c.cap_tokens as f64);
            }
        }
    }

    let pct = |v: &mut Vec<f64>, q: f64| -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f64 * q) as usize]
    };
    let pctu = |v: &mut Vec<usize>, q: f64| -> usize {
        if v.is_empty() {
            return 0;
        }
        v.sort_unstable();
        v[((v.len() - 1) as f64 * q) as usize]
    };

    println!("=== CASING TWO-FACTOR SPIKE — fleet aggregate ({} corpora) ===", corpora.len());

    println!("\n-- reference setting (absolute knee k=32, floor 0.5, hard censoring) --");
    println!(
        "  surfaced sites: {ref_surf}  (intrinsic {ref_i}, positional {ref_p}, both {ref_b})  across {corpora_with_ref} corpora"
    );
    println!("  soft-censoring surfaced: {soft_surf}  |  hard-vs-soft differing verdicts: {hs_diff}");

    println!("\n-- current rule (floor 0.98) surfaced set and its fate --");
    println!("  current-rule surfaced sites (fleet): {cur_surf}");
    println!(
        "  fate: survive {f_surv}  die(recurrence) {f_rar}  die(habit / proper-noun confound) {f_hab}  both-quadrant {f_both}  die(word unclassifiable) {f_amb}"
    );

    println!("\n-- naive vs lexicon-restricted habit delta (proper-noun confound), per-corpus pooled --");
    println!(
        "  corpora with a habit: {}  |  delta p10 {:.4}  p50 {:.4}  p90 {:.4}  max {:.4}",
        deltas.len(),
        pct(&mut deltas, 0.10),
        pct(&mut deltas, 0.50),
        pct(&mut deltas, 0.90),
        pct(&mut deltas, 1.0),
    );

    println!("\n-- censoring shadow (fraction of cap words whose upper evidence is ≥90% forced) --");
    println!(
        "  TYPES frac  p50 {:.3}  p90 {:.3}  max {:.3}",
        pct(&mut shadow_type_frac, 0.5),
        pct(&mut shadow_type_frac, 0.9),
        pct(&mut shadow_type_frac, 1.0),
    );
    println!(
        "  TOKENS frac p50 {:.3}  p90 {:.3}  max {:.3}",
        pct(&mut shadow_tok_frac, 0.5),
        pct(&mut shadow_tok_frac, 0.9),
        pct(&mut shadow_tok_frac, 1.0),
    );

    println!("\n-- per-corpus word-table cardinality (future word-level RuleStats sizing) --");
    println!(
        "  word types   p50 {}  p90 {}  max {}",
        pctu(&mut types_v, 0.5),
        pctu(&mut types_v, 0.9),
        pctu(&mut types_v, 1.0),
    );
    println!(
        "  approx bytes p50 {}  p90 {}  max {}",
        pctu(&mut bytes_v, 0.5),
        pctu(&mut bytes_v, 0.9),
        pctu(&mut bytes_v, 1.0),
    );

    println!("\n-- surfaced-site volume by knee shape / k / floor (fleet) --");
    println!("absolute knee (rows = k):");
    print!("  {:>6}", "k\\fl");
    for fl in FLOORS {
        print!("  {fl:>8.2}");
    }
    println!();
    for (ki, &k) in ABS_KS.iter().enumerate() {
        print!("  {k:>6.0}");
        for fi in 0..FLOORS.len() {
            print!("  {:>8}", abs_grid[ki][fi]);
        }
        println!();
    }
    println!("rate knee (rows = per-1k minority cutoff):");
    print!("  {:>6}", "r\\fl");
    for fl in FLOORS {
        print!("  {fl:>8.2}");
    }
    println!();
    for (ki, &k) in RATE_KS.iter().enumerate() {
        print!("  {k:>6.1}");
        for fi in 0..FLOORS.len() {
            print!("  {:>8}", rate_grid[ki][fi]);
        }
        println!();
    }

    print_casing_hist(&hist);

    // Noisiest corpora at the reference setting.
    let mut ranked: Vec<&CasingCorpus> = corpora.iter().filter(|c| c.ref_surfaced > 0).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.ref_surfaced));
    println!("\n-- top-15 noisiest corpora (ref setting surfaced) --");
    for c in ranked.iter().take(15) {
        println!(
            "  {:<24} surfaced {:>6}  (i {}, p {}, b {})  delta {:+.3}",
            c.id,
            c.ref_surfaced,
            c.ref_intrinsic,
            c.ref_positional,
            c.ref_both,
            c.pooled_naive_dom - c.pooled_lex_dom
        );
    }

    // A spread of surfaced samples from major-language corpora, for review.
    const MAJOR: &[&str] = &[
        "eng-web", "eng-kjv", "engwebster", "WA-en-ulb", "spaRV1909", "WA-es-419-ulb",
        "fraLSG", "WA-fr-ulb", "porblt", "WA-pt-br-ulb", "ita1885", "ron1924", "deu1912",
        "swhulb", "WA-sw-ulb", "ind", "nld", "vie1934", "tglulb",
    ];
    println!("\n-- surfaced samples from major-language corpora (ref knee) --");
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples.is_empty() {
            continue;
        }
        let mut s: Vec<&CasingSample> = c.samples.iter().collect();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        println!("  [{}] surfaced {} (i {}, p {}, b {}):", c.id, c.ref_surfaced, c.ref_intrinsic, c.ref_positional, c.ref_both);
        for sm in s.iter().take(3) {
            println!(
                "    {:<10} {:<10} [{}] g={} dom={:.3} min={} opp={} rar={:.3} score={:.3} | {}",
                sm.sid, sm.quad, sm.word, glyph_str(sm.glyph),
                sm.dom, sm.minority, sm.opps, sm.rarity, sm.score, sm.ctx,
            );
        }
    }
}
