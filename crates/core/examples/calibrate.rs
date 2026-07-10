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
//!   # rare-glyph inventory and recurrence-knee spike (one corpus or fleet):
//!   cargo run --release -p ssc-core --example calibrate -- --glyphs corpora/vref
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
use ssc_core::charclass::class_of;
use ssc_core::rule::{ProjectRule, StatefulRule};
use ssc_core::signals::casing::{PosClass, SiteEval, evaluate};
use ssc_core::signals::bracket_balance::BracketBalance;
use ssc_core::signals::lexical::{PunctOnlyToken, RepeatedCharacterRun};
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::signals::punctuation::{PunctuationAdjacencyAnomaly, PunctuationSpacingAnomaly};
use ssc_core::token::tokenize;
use ssc_core::{
    BookId, BracketMeasure, Config, Finding, FindingArgs, LengthRatioScope, RuleId, VerseMap,
    analyze, analyze_with_config,
};

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::load_corpus;

// terminal_strength SPIKE (shortlist 2/3) — dev-only sweep harness. The trust
// model itself now ships in `signals::casing` (ADR 0052); this spike retains
// the multiplier-vs-gate sweep reporting the calibration doc was built from,
// reading the graduated `analysis::association`.
#[path = "../dev/terminal.rs"]
mod terminal;

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
        // Casing two-factor calibration (ADR 0051). `<path>` is a single vref
        // file (per-corpus report) or the `corpora/vref` directory (fleet
        // aggregate). Drives the real `signals::casing::evaluate` — the same
        // walk, model, and soft-censored classification the shipped rules use —
        // and sweeps floor/k over its per-site factors.
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
        // Rare-glyph calibration: tally every scalar for the future census,
        // but score only the visible L/N/P/S candidate lanes. A file prints
        // its glyph table; a vref directory aggregates the fleet sweep.
        [flag, path] if flag == "--glyphs" => {
            let p = Path::new(path);
            if p.is_dir() {
                glyph_fleet(p);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                glyph_single_report(&analyze_glyphs(id, &load_corpus(p)));
            }
            return;
        }
        // terminal_strength SPIKE (shortlist 2/3): per-mark boundary trust
        // (W1 case-follow ⊕ W2 word-reshuffle, noisy-OR) wired into ADR 0051
        // casing. `<path>` = a single vref file (per-corpus report) or the
        // `corpora/vref` directory (fleet deltas). Optional trailing `A` uses
        // the plain-differentness W2 variant (default is the guarded B).
        [flag, path, rest @ ..] if flag == "--terminal" && rest.len() <= 1 => {
            let variant_b = rest.first().map(|s| s.as_str()) != Some("A");
            let p = Path::new(path);
            if p.is_dir() {
                terminal_fleet(p, variant_b);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                terminal_single(&terminal::analyze_corpus(id, &load_corpus(p), variant_b));
            }
            return;
        }
        // Casing stats-size probe (ADR 0051): reduce each corpus with the real
        // rule and report the serialized `CasingStats` JSON byte size (the wire
        // size that round-trips) percentiles across the fleet.
        [flag, dir] if flag == "--casing-size" => {
            casing_size(Path::new(dir));
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
            if n.is_multiple_of(100) {
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
// Casing two-factor calibration (ADR 0051). Consumes the real
// `signals::casing::evaluate` (one classified `SiteEval` per lowercase site,
// with each channel's dominance/minority/opportunities), then sweeps the
// absolute recurrence knee `k` and the emission floor over those factors —
// `score = dominance × rarity(minority, k)` — exactly as the shipped rules do
// at the frozen knobs. The rules apply floor 0.95 / k 32; this reports the
// grid around that so the packet volumes stay reproducible.
// ═══════════════════════════════════════════════════════════════════════════

/// Packet floor/knee grid (rows = floor, cols = k); the shipped knobs are the
/// (0.95, 32) cell.
const PACKET_FLOORS: [f64; 4] = [0.80, 0.90, 0.95, 0.98];
const PACKET_KS: [f64; 3] = [8.0, 16.0, 32.0];
const REF_FLOOR: f64 = 0.95;
const REF_K: f64 = 32.0;

/// Named review anchors tracked across the fleet — `(corpus, sid, lowercased
/// word)`; the ADR 0051 adjudicated true/false positives.
const ANCHORS: &[(&str, &str, &str)] = &[
    ("swhulb", "LUK 8:44", "yesu"),        // TP intrinsic
    ("WA-fr-ulb", "JHN 13:2", "jésus"),    // TP intrinsic
    ("spaRV1909", "1SA 7:8", "filisteos"), // TP intrinsic
    ("vie1934", "MAT 24:24", "christ"),    // TP intrinsic (min 2)
    ("eng-web", "3MA 6:9", "gentiles"),    // TP-ish intrinsic
    ("eng-kjv", "SIR 7:5", "justify"),     // TP positional (cross-seam)
    ("WA-en-ulb", "LAM 1:22", "deal"),     // TP positional (min 2)
    ("fraLSG", "ACT 19:13", "juifs"),      // FP intrinsic (French adjective)
    ("porblt", "MAT 24:24", "messias"),    // FP intrinsic (generic plural)
    ("deu1912", "PHM 1:9", "alter"),       // FP intrinsic (adj/noun homograph)
    ("ind", "DEU 14:12", "rajawali"),      // FP positional (list colon)
    ("nld", "GEN 6:19", "mannetje"),       // FP positional (list colon)
];

/// The absolute linear recurrence knee (ADR 0050/0051 absolute form).
fn rarity_abs(minority: u64, k: f64) -> f64 {
    (1.0 - (minority.saturating_sub(1) as f64 / k)).clamp(0.0, 1.0)
}

/// A site's two channel scores at knee `k` (0 where the channel is absent).
fn site_scores(s: &SiteEval, k: f64) -> (f64, f64) {
    let intr = s.intrinsic.map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
    let pos = s.positional.map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
    (intr, pos)
}

/// The site's quadrant (`None` = not a clean anomaly candidate).
fn site_quad(s: &SiteEval) -> Option<&'static str> {
    match (s.intrinsic.is_some(), s.positional.is_some()) {
        (true, true) => Some("both"),
        (true, false) => Some("intrinsic"),
        (false, true) => Some("positional"),
        (false, false) => None,
    }
}

fn pos_glyph(pos: PosClass) -> Option<char> {
    match pos {
        PosClass::ForcedAfterTerminal(ck) => Some(ck.mark),
        _ => None,
    }
}

/// One tracked anchor's factors, so its score is recomputable at any k.
#[derive(Clone)]
struct AnchorHit {
    corpus: String,
    sid: String,
    word: String,
    quad: &'static str,
    intr: Option<(f64, u64, u64)>,
    pos: Option<(f64, u64, u64)>,
}

impl AnchorHit {
    fn score(&self, k: f64) -> f64 {
        let i = self.intr.map_or(0.0, |(d, m, _)| d * rarity_abs(m, k));
        let p = self.pos.map_or(0.0, |(d, m, _)| d * rarity_abs(m, k));
        i.max(p)
    }
}

/// One surfaced site sampled for review.
struct CasingSample {
    sid: String,
    quad: &'static str,
    word: String,
    glyph: Option<char>,
    dom: f64,
    minority: u64,
    opps: u64,
    score: f64,
    ctx: String,
}

/// Per-corpus casing result. Grids are `[knee][floor]`, fleet-summable.
struct CasingCorpus {
    id: String,
    verses: usize,
    sites: u64,
    grid_intr: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_pos: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_both: Vec<[u64; PACKET_FLOORS.len()]>,
    hist: [u64; 40],
    ref_intrinsic: u64,
    ref_positional: u64,
    ref_both: u64,
    anchors: Vec<AnchorHit>,
    samples: Vec<CasingSample>,
}

/// Run the real casing model over one corpus and roll up the sweep grids,
/// reference-setting counts, histogram, tracked anchors, and samples.
fn analyze_casing(id: String, map: &VerseMap) -> CasingCorpus {
    let books = ssc_core::verse::by_book(map);
    // Production knobs (ADR 0051 floor/k/z + ADR 0052 trust gate 0.90). The
    // sweep below varies floor/k around the reference cell; the trust gate and
    // discount are baked into the returned factors.
    let sites = evaluate(&books, &ssc_core::config::CasingConfig::default());

    let nk = PACKET_KS.len();
    let mut grid_intr = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_pos = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_both = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist = [0u64; 40];
    let (mut ref_i, mut ref_p, mut ref_b) = (0u64, 0u64, 0u64);
    let mut anchors: Vec<AnchorHit> = Vec::new();
    let mut samples: Vec<CasingSample> = Vec::new();
    let anchor_corpus = ANCHORS.iter().any(|a| a.0 == id);
    let mut n_sites = 0u64;

    for s in &sites {
        let Some(quad) = site_quad(s) else { continue };
        n_sites += 1;
        let text = &map[&s.sid];
        let word = text[s.start as usize..s.end as usize].to_lowercase();

        // Sweep grids.
        for (ki, &k) in PACKET_KS.iter().enumerate() {
            let (is, ps) = site_scores(s, k);
            let surf = is.max(ps);
            for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                if surf >= fl {
                    match quad {
                        "intrinsic" => grid_intr[ki][fi] += 1,
                        "positional" => grid_pos[ki][fi] += 1,
                        _ => grid_both[ki][fi] += 1,
                    }
                }
            }
        }

        // Reference setting (k=32, floor 0.95): counts, histogram, samples.
        let (is, ps) = site_scores(s, REF_K);
        let surf = is.max(ps);
        hist[(surf.clamp(0.0, 0.999_999) * 40.0) as usize] += 1;
        if surf >= REF_FLOOR {
            match quad {
                "intrinsic" => ref_i += 1,
                "positional" => ref_p += 1,
                _ => ref_b += 1,
            }
            if samples.len() < 400 {
                let (dom, min, opp) = if ps >= is {
                    let f = s.positional.unwrap();
                    (f.dominance, f.minority, f.opportunities)
                } else {
                    let f = s.intrinsic.unwrap();
                    (f.dominance, f.minority, f.opportunities)
                };
                samples.push(CasingSample {
                    sid: s.sid.to_string(),
                    quad,
                    word: text[s.start as usize..s.end as usize].to_string(),
                    glyph: pos_glyph(s.pos),
                    dom,
                    minority: min,
                    opps: opp,
                    score: surf,
                    ctx: casing_ctx(text, s.start as usize, s.end as usize),
                });
            }
        }

        // Anchor capture.
        if anchor_corpus
            && ANCHORS.iter().any(|&(ac, asid, aw)| ac == id && asid == s.sid.to_string() && aw == word)
        {
            anchors.push(AnchorHit {
                corpus: id.clone(),
                sid: s.sid.to_string(),
                word,
                quad,
                intr: s.intrinsic.map(|f| (f.dominance, f.minority, f.opportunities)),
                pos: s.positional.map(|f| (f.dominance, f.minority, f.opportunities)),
            });
        }
    }

    CasingCorpus {
        id,
        verses: map.len(),
        sites: n_sites,
        grid_intr,
        grid_pos,
        grid_both,
        hist,
        ref_intrinsic: ref_i,
        ref_positional: ref_p,
        ref_both: ref_b,
        anchors,
        samples,
    }
}

/// ~24 chars of lead-in plus the flagged word, whitespace flattened.
fn casing_ctx(text: &str, start: usize, end: usize) -> String {
    let ctx_start = text[..start].char_indices().rev().nth(23).map(|(i, _)| i).unwrap_or(0);
    let ctx_end = text[end..].char_indices().nth(24).map(|(i, _)| end + i).unwrap_or(text.len());
    text[ctx_start..ctx_end].replace(['\t', '\n'], " ")
}

fn print_casing_grid(name: &str, grid: &[[u64; PACKET_FLOORS.len()]]) {
    println!("  [{name}] rows = floor, cols = k");
    print!("    {:>6}", "fl\\k");
    for k in PACKET_KS {
        print!("  {:>8}", format!("k={k:.0}"));
    }
    println!();
    for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
        print!("    {fl:>6.2}");
        for row in grid {
            print!("  {:>8}", row[fi]);
        }
        println!();
    }
}

fn print_casing_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!("\nscore histogram at ref knee (k=32) — {total} sites, 40 buckets:");
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>7} {bar}", lo + 0.025);
    }
}

fn print_casing_samples(samples: &[&CasingSample]) {
    for s in samples {
        println!(
            "    {:<11} {:<10} [{}] g={} dom={:.3} min={} opp={} score={:.3} | {}",
            s.sid,
            s.quad,
            s.word,
            s.glyph.map(|c| format!("{c:?}")).unwrap_or_else(|| "^".to_string()),
            s.dom,
            s.minority,
            s.opps,
            s.score,
            s.ctx,
        );
    }
}

/// Detailed single-corpus casing report.
fn casing_single_report(c: &CasingCorpus) {
    println!("=== casing (ADR 0051): {} ({} verses) ===", c.id, c.verses);
    println!("classifiable lowercase sites: {}", c.sites);
    println!(
        "\nreference setting (k=32, floor 0.95): surfaced {} (intrinsic {}, positional {}, both {})",
        c.ref_intrinsic + c.ref_positional + c.ref_both,
        c.ref_intrinsic,
        c.ref_positional,
        c.ref_both
    );
    println!("\nsurfaced-site volume sweep:");
    print_casing_grid("intrinsic", &c.grid_intr);
    print_casing_grid("positional", &c.grid_pos);
    print_casing_grid("both-quadrant", &c.grid_both);
    print_casing_hist(&c.hist);

    let mut s: Vec<&CasingSample> = c.samples.iter().collect();
    s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop surfaced samples (ref knee):");
    print_casing_samples(&s.iter().take(20).copied().collect::<Vec<_>>());
    println!("\nnear-floor surfaced samples:");
    print_casing_samples(&s.iter().rev().take(10).copied().collect::<Vec<_>>());
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
    let t0 = std::time::Instant::now();
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
            if n.is_multiple_of(100) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("casing fleet evaluate: {:?}", t0.elapsed());

    // Fleet aggregates.
    let nk = PACKET_KS.len();
    let (mut ref_i, mut ref_p, mut ref_b) = (0u64, 0u64, 0u64);
    let mut hist = [0u64; 40];
    let mut corpora_with_ref = 0u32;
    for c in &corpora {
        ref_i += c.ref_intrinsic;
        ref_p += c.ref_positional;
        ref_b += c.ref_both;
        for (h, ch) in hist.iter_mut().zip(&c.hist) {
            *h += ch;
        }
        if c.ref_intrinsic + c.ref_positional + c.ref_both > 0 {
            corpora_with_ref += 1;
        }
    }

    println!("=== CASING TWO-FACTOR (ADR 0051) — fleet aggregate ({} corpora) ===", corpora.len());
    println!(
        "\n-- reference setting (k=32, floor 0.95) --\n  surfaced: {}  (intrinsic {ref_i}, positional {ref_p}, both {ref_b})  across {corpora_with_ref} corpora",
        ref_i + ref_p + ref_b
    );

    // Packet 1 — per-channel volume, affected corpora, top-5 corpus share.
    // `chan`: 0 = intrinsic, 1 = positional, 2 = both.
    let channel_cell = |chan: u8, ki: usize, fi: usize| -> (u64, u32, f64) {
        let mut counts: Vec<u64> = corpora
            .iter()
            .map(|c| match chan {
                0 => c.grid_intr[ki][fi],
                1 => c.grid_pos[ki][fi],
                _ => c.grid_both[ki][fi],
            })
            .filter(|&n| n > 0)
            .collect();
        let total: u64 = counts.iter().sum();
        let affected = counts.len() as u32;
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let top5: u64 = counts.iter().take(5).sum();
        (total, affected, if total > 0 { top5 as f64 / total as f64 } else { 0.0 })
    };
    println!("\n-- packet 1: per-channel surfaced volume | total (affected corpora; top-5 share) --");
    for (chan, name) in [(0u8, "intrinsic"), (1, "positional"), (2, "both-quadrant")] {
        println!("  [{name}]  rows = floor, cols = k");
        print!("    {:>6}", "fl\\k");
        for k in PACKET_KS {
            print!("  {:>22}", format!("k={k:.0}"));
        }
        println!();
        for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
            print!("    {fl:>6.2}");
            for ki in 0..nk {
                let (t, a, sh) = channel_cell(chan, ki, fi);
                print!("  {:>22}", format!("{t} ({a}; {:.0}%)", sh * 100.0));
            }
            println!();
        }
    }

    // Packet 2 — anchor fates.
    let all_anchors: Vec<&AnchorHit> = corpora.iter().flat_map(|c| c.anchors.iter()).collect();
    println!("\n-- packet 2: anchor fates — factors, score@k, alive floors at k=32 --");
    for &(ac, asid, aw) in ANCHORS {
        match all_anchors.iter().find(|h| h.corpus == ac && h.sid == asid && h.word == aw) {
            Some(h) => {
                let (s8, s16, s32) = (h.score(8.0), h.score(16.0), h.score(32.0));
                let alive: Vec<String> = PACKET_FLOORS.iter().filter(|&&fl| s32 >= fl).map(|fl| format!("{fl:.2}")).collect();
                let ifac = h.intr.map(|(d, m, o)| format!("i(d{d:.3} m{m} o{o})")).unwrap_or_default();
                let pfac = h.pos.map(|(d, m, o)| format!("p(d{d:.3} m{m} o{o})")).unwrap_or_default();
                println!(
                    "  {ac:<11} {asid:<9} {aw:<11} {:<11} {ifac} {pfac}  s@8={s8:.3} @16={s16:.3} @32={s32:.3}  alive≥[{}]",
                    h.quad,
                    if alive.is_empty() { "dead@0.80+".to_string() } else { alive.join(",") },
                );
            }
            None => println!("  {ac:<11} {asid:<9} {aw:<11} — not captured (not a lowercase anomaly candidate)"),
        }
    }

    print_casing_hist(&hist);

    // Noisiest corpora at the reference setting.
    let mut ranked: Vec<&CasingCorpus> = corpora
        .iter()
        .filter(|c| c.ref_intrinsic + c.ref_positional + c.ref_both > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.ref_intrinsic + c.ref_positional + c.ref_both));
    println!("\n-- top-15 noisiest corpora (ref setting) --");
    for c in ranked.iter().take(15) {
        println!(
            "  {:<24} surfaced {:>6}  (i {}, p {}, b {})",
            c.id,
            c.ref_intrinsic + c.ref_positional + c.ref_both,
            c.ref_intrinsic,
            c.ref_positional,
            c.ref_both
        );
    }

    // Samples from major-language corpora.
    const MAJOR: &[&str] = &[
        "eng-web", "eng-kjv", "engwebster", "WA-en-ulb", "spaRV1909", "WA-es-419-ulb",
        "fraLSG", "WA-fr-ulb", "porblt", "ita1885", "ron1924", "deu1912", "swhulb",
        "WA-sw-ulb", "ind", "nld", "vie1934", "tglulb",
    ];
    println!("\n-- surfaced samples from major-language corpora (ref knee) --");
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples.is_empty() {
            continue;
        }
        let mut s: Vec<&CasingSample> = c.samples.iter().collect();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        println!("  [{}] surfaced {} (i {}, p {}, b {}):", c.id, c.ref_intrinsic + c.ref_positional + c.ref_both, c.ref_intrinsic, c.ref_positional, c.ref_both);
        print_casing_samples(&s.iter().take(3).copied().collect::<Vec<_>>());
    }
}

/// Casing stats-size probe: reduce every corpus with the real
/// `SentenceInitialLowercase` rule and report the serialized `CasingStats`
/// JSON byte size (the wire size the shell round-trips) — p50/p90/max plus a
/// few named corpora.
fn casing_size(dir: &Path) {
    use rayon::prelude::*;
    use ssc_core::config::CasingConfig;
    use ssc_core::signals::casing::SentenceInitialLowercase;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let rule = SentenceInitialLowercase { cfg: CasingConfig::default() };
    let mut rows: Vec<(String, usize)> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let books = ssc_core::verse::by_book(&map);
            let (stats, _) = rule.reduce(&books, None, None);
            let bytes = serde_json::to_string(&stats).map(|s| s.len()).unwrap_or(0);
            (id, bytes)
        })
        .collect();
    rows.sort_by_key(|r| r.1);
    let n = rows.len();
    let pct = |q: f64| rows[((n - 1) as f64 * q) as usize].1;
    println!("casing CasingStats JSON size over {n} corpora:");
    println!("  p50 {} B  p90 {} B  max {} B", pct(0.5), pct(0.9), pct(1.0));
    println!("  largest: {} ({} B)", rows[n - 1].0, rows[n - 1].1);
    for id in ["eng-kjv", "deu1912", "swhulb", "vie1934"] {
        if let Some((_, b)) = rows.iter().find(|r| r.0 == id) {
            println!("  {id}: {b} B");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Rare-glyph calibration. The inventory counts every scalar so a future census
// can reuse this walk. The spike's candidate rows are deliberately narrower:
// visible letters, numbers, punctuation, and symbols only.
// ═══════════════════════════════════════════════════════════════════════════

const GLYPH_ABS_KS: [f64; 6] = [2.0, 4.0, 8.0, 16.0, 32.0, 64.0];
const GLYPH_RATE_PER_10K: [f64; 6] = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0];
const GLYPH_SWEEP_FLOOR: f64 = 0.95;
const GLYPH_HIST_LABELS: [&str; 8] = ["1", "2", "3-4", "5-8", "9-16", "17-32", "33-64", "65+"];
// Round 3: alphabet closure is now a LETTER-SCALAR share (hapax L-scalar types /
// all L-scalar occurrences), which is far smaller than the round-2 word-hapax
// share, so the self-disable sweep uses finer low-end steps: 0.001% … 2%.
const CLOSURE_SCALAR_SHARES: [f64; 8] =
    [0.00001, 0.0001, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02];
// Round 3: knee ≤1–5 was conjecture; sweep ≤1 through ≤8 to see where the
// retained set stops being flat.
const LETTER_RARE_MAX_COUNTS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// Representative closure threshold and knee used only to pick retained review
/// samples for the human adjudication table (not a frozen knob).
const RETAINED_SAMPLE_THRESHOLD: f64 = 0.001;
const RETAINED_SAMPLE_KNEE: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlyphLane {
    Letter,
    Number,
    Punctuation,
    Symbol,
}

impl GlyphLane {
    const ALL: [Self; 4] = [Self::Letter, Self::Number, Self::Punctuation, Self::Symbol];

    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Punctuation => 2,
            Self::Symbol => 3,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "L",
            Self::Number => "N",
            Self::Punctuation => "P",
            Self::Symbol => "S",
        }
    }
}

/// The visible candidate lanes. Marks, separators, controls, and formats are
/// inventoried but never enter the spike's rarity sweeps.
fn glyph_lane(c: char) -> Option<GlyphLane> {
    let cl = class_of(c);
    if cl.is_mark()
        || cl.is_whitespace()
        || cl.is_control()
        || cl.is_zero_width_format()
        || cl.is_invalid_codepoint()
    {
        return None;
    }
    if cl.is_numeric() {
        Some(GlyphLane::Number)
    } else if cl.is_alphabetic() {
        Some(GlyphLane::Letter)
    } else if cl.is_punctuation() {
        Some(GlyphLane::Punctuation)
    } else if cl.is_symbol() {
        Some(GlyphLane::Symbol)
    } else {
        None
    }
}

/// UAX #29 tokens that consist only of letters and their combining marks.
/// Numeric references and mixed `q1`-style tokens do not establish alphabet
/// closure or lexical concentration.
fn is_letter_token(word: &str) -> bool {
    let mut has_letter = false;
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_alphabetic() && !cl.is_mark() {
            has_letter = true;
        } else if !cl.is_mark() {
            return false;
        }
    }
    has_letter
}

fn letter_round2(
    inventory: &BTreeMap<char, u64>,
    word_tokens: BTreeMap<String, u64>,
    glyph_words: BTreeMap<char, BTreeMap<String, u64>>,
) -> LetterRound2 {
    let tokens: u64 = word_tokens.values().sum();
    let hapax_types = word_tokens.values().filter(|&&count| count == 1).count() as u64;
    // Letter-scalar closure straight off the inventory the harness already built.
    let mut letter_scalars = 0u64;
    let mut hapax_letter_scalars = 0u64;
    for (&glyph, &count) in inventory {
        if glyph_lane(glyph) == Some(GlyphLane::Letter) {
            letter_scalars += count;
            if count == 1 {
                hapax_letter_scalars += 1;
            }
        }
    }
    let mut rare = Vec::new();
    for (&glyph, &count) in inventory {
        if glyph_lane(glyph) != Some(GlyphLane::Letter) || count > *LETTER_RARE_MAX_COUNTS.last().unwrap() {
            continue;
        }
        let Some(words) = glyph_words.get(&glyph) else {
            rare.push(LetterRare {
                glyph,
                count,
                lexical_word: None,
                lexical_word_tokens: 0,
            });
            continue;
        };
        let accounted: u64 = words.values().sum();
        let dominant = words.iter().max_by_key(|(_, occurrences)| **occurrences);
        let (lexical_word, lexical_word_tokens) = match dominant {
            Some((word, &occurrences))
                if accounted == count && occurrences == count && word_tokens.get(word).copied().unwrap_or(0) >= 2 =>
            {
                (Some(word.clone()), word_tokens[word])
            }
            _ => (None, 0),
        };
        rare.push(LetterRare {
            glyph,
            count,
            lexical_word,
            lexical_word_tokens,
        });
    }
    rare.sort_by_key(|candidate| (candidate.count, candidate.glyph));
    LetterRound2 {
        tokens,
        types: word_tokens.len() as u64,
        hapax_types,
        letter_scalars,
        hapax_letter_scalars,
        rare,
    }
}

fn glyph_count_bucket(count: u64) -> usize {
    match count {
        0 => unreachable!("inventory entries have nonzero counts"),
        1 => 0,
        2 => 1,
        3..=4 => 2,
        5..=8 => 3,
        9..=16 => 4,
        17..=32 => 5,
        33..=64 => 6,
        _ => 7,
    }
}

fn glyph_rarity_abs(count: u64, knee: f64) -> f64 {
    rarity_abs(count, knee)
}

/// A rate-shaped knee: one occurrence remains fully rare, then the knee grows
/// with opportunities in the glyph's own category lane.
fn glyph_rarity_rate(count: u64, lane_total: u64, rate_per_10k: f64) -> f64 {
    let knee = 1.0 + rate_per_10k * lane_total as f64 / 10_000.0;
    rarity_abs(count, knee)
}

#[derive(Clone, Copy)]
struct GlyphCandidate {
    glyph: char,
    lane: GlyphLane,
    count: u64,
    lane_total: u64,
}

#[derive(Clone, Copy, Default)]
struct GlyphSweep {
    types: u64,
    sites: u64,
}

#[derive(Clone)]
struct GlyphSample {
    corpus: String,
    sid: String,
    glyph: char,
    lane: GlyphLane,
    count: u64,
    lane_total: u64,
    context: String,
}

/// One very-rare letter's lexical evidence. A concentration discount is only
/// justified when every scalar occurrence is accounted for by one repeatedly
/// observed, case-folded word type.
struct LetterRare {
    glyph: char,
    count: u64,
    lexical_word: Option<String>,
    lexical_word_tokens: u64,
}

struct LetterRound2 {
    // Word-level machinery, retained unchanged for the lexical-concentration
    // discount and for the round-2/round-3 flip comparison.
    tokens: u64,
    types: u64,
    hapax_types: u64,
    // Round-3 alphabet-closure gate: letter-SCALAR closure. `letter_scalars` is
    // total GC-L scalar occurrences; `hapax_letter_scalars` is the number of L
    // scalar types seen exactly once. Their ratio is the hapax-letter-type
    // occurrence share (each hapax type contributes exactly one occurrence).
    letter_scalars: u64,
    hapax_letter_scalars: u64,
    rare: Vec<LetterRare>,
}

impl LetterRound2 {
    /// Letter-SCALAR closure (round 3): hapax L-scalar occurrence share. ~0 for
    /// closed alphabets (English/Bemba), materially nonzero for open inventories
    /// (CJK). This is the alphabet-closure gate, not vocabulary closure.
    fn closure(&self) -> f64 {
        self.hapax_letter_scalars as f64 / self.letter_scalars.max(1) as f64
    }

    /// Round-2 metric, kept only to report which corpora flip open under the
    /// round-3 scalar closure that were closed under word-hapax share.
    fn word_hapax_share(&self) -> f64 {
        self.hapax_types as f64 / self.tokens.max(1) as f64
    }
}

struct GlyphCorpus {
    id: String,
    verses: usize,
    scalar_count: u64,
    inventory: BTreeMap<char, u64>,
    lane_totals: [u64; 4],
    count_hist: [[u64; GLYPH_HIST_LABELS.len()]; 4],
    abs_sweeps: Vec<[GlyphSweep; 4]>,
    rate_sweeps: Vec<[GlyphSweep; 4]>,
    decomposed_pairs: BTreeMap<String, u64>,
    samples: Vec<GlyphSample>,
    letter_round2: LetterRound2,
    retained_samples: Vec<GlyphSample>,
}

/// The fleet keeps calibration rollups, not a corpus's full scalar inventory.
/// This permits corpus-level parallelism without retaining all 1,504 maps.
struct GlyphFleetSummary {
    id: String,
    scalar_count: u64,
    lane_totals: [u64; 4],
    count_hist: [[u64; GLYPH_HIST_LABELS.len()]; 4],
    abs_sweeps: Vec<[GlyphSweep; 4]>,
    rate_sweeps: Vec<[GlyphSweep; 4]>,
    decomposed_pairs: BTreeMap<String, u64>,
    samples: Vec<GlyphSample>,
    letter_round2: LetterRound2,
    retained_samples: Vec<GlyphSample>,
}

impl From<GlyphCorpus> for GlyphFleetSummary {
    fn from(corpus: GlyphCorpus) -> Self {
        Self {
            id: corpus.id,
            scalar_count: corpus.scalar_count,
            lane_totals: corpus.lane_totals,
            count_hist: corpus.count_hist,
            abs_sweeps: corpus.abs_sweeps,
            rate_sweeps: corpus.rate_sweeps,
            decomposed_pairs: corpus.decomposed_pairs,
            samples: corpus.samples,
            letter_round2: corpus.letter_round2,
            retained_samples: corpus.retained_samples,
        }
    }
}

fn glyph_candidates(inventory: &BTreeMap<char, u64>, lane_totals: &[u64; 4]) -> Vec<GlyphCandidate> {
    inventory
        .iter()
        .filter_map(|(&glyph, &count)| {
            glyph_lane(glyph).map(|lane| GlyphCandidate {
                glyph,
                lane,
                count,
                lane_total: lane_totals[lane.index()],
            })
        })
        .collect()
}

fn glyph_sweep(candidates: &[GlyphCandidate], score: impl Fn(GlyphCandidate) -> f64) -> [GlyphSweep; 4] {
    candidates.iter().copied().fold([GlyphSweep::default(); 4], |mut out, candidate| {
        if score(candidate) >= GLYPH_SWEEP_FLOOR {
            let lane = &mut out[candidate.lane.index()];
            lane.types += 1;
            lane.sites += candidate.count;
        }
        out
    })
}

fn glyph_sweep_total(sweep: &[GlyphSweep; 4]) -> GlyphSweep {
    sweep.iter().fold(GlyphSweep::default(), |mut total, lane| {
        total.types += lane.types;
        total.sites += lane.sites;
        total
    })
}

fn glyph_context(text: &str, start: usize, end: usize) -> String {
    let before = text[..start].char_indices().rev().nth(22).map(|(i, _)| i).unwrap_or(0);
    let after = text[end..].char_indices().nth(22).map(|(i, _)| end + i).unwrap_or(text.len());
    text[before..after].replace(['\t', '\n'], " ")
}

/// Pick one source occurrence for the strongest rare candidates. The samples
/// are review leads, not stored rule sites: a production rule will forward or
/// re-scan its own spans under the stateful protocol.
fn glyph_samples(id: &str, map: &VerseMap, candidates: &[GlyphCandidate]) -> Vec<GlyphSample> {
    let mut ranked: Vec<GlyphCandidate> = candidates
        .iter()
        .copied()
        .filter(|c| glyph_rarity_abs(c.count, 32.0) >= GLYPH_SWEEP_FLOOR)
        .collect();
    ranked.sort_by_key(|c| (std::cmp::Reverse(c.lane_total), c.count, c.glyph));

    let mut wanted = BTreeMap::new();
    for lane in GlyphLane::ALL {
        for candidate in ranked.iter().copied().filter(|candidate| candidate.lane == lane).take(6) {
            wanted.insert(candidate.glyph, candidate);
        }
    }
    let mut samples = Vec::new();
    for (sid, text) in map {
        for (start, glyph) in text.char_indices() {
            let Some(candidate) = wanted.remove(&glyph) else { continue };
            samples.push(GlyphSample {
                corpus: id.to_string(),
                sid: sid.to_string(),
                glyph,
                lane: candidate.lane,
                count: candidate.count,
                lane_total: candidate.lane_total,
                context: glyph_context(text, start, start + glyph.len_utf8()),
            });
            if wanted.is_empty() {
                return samples;
            }
        }
    }
    samples.sort_by_key(|sample| {
        (
            sample.lane.index(),
            std::cmp::Reverse(sample.lane_total),
            sample.count,
            sample.glyph,
        )
    });
    samples
}

/// Retained review leads: rare letter glyphs (count ≤ knee) that survive the
/// lexical-concentration discount, so a human can adjudicate signal quality on
/// the set the rule would actually keep in a closed-alphabet corpus. Whether the
/// corpus itself clears closure is decided at fleet time.
fn glyph_retained_samples(id: &str, map: &VerseMap, round2: &LetterRound2) -> Vec<GlyphSample> {
    let mut wanted: BTreeMap<char, u64> = BTreeMap::new();
    for candidate in round2
        .rare
        .iter()
        .filter(|c| c.count <= RETAINED_SAMPLE_KNEE && c.lexical_word.is_none())
    {
        wanted.insert(candidate.glyph, candidate.count);
    }
    let mut samples = Vec::new();
    for (sid, text) in map {
        if wanted.is_empty() {
            break;
        }
        for (start, glyph) in text.char_indices() {
            let Some(count) = wanted.remove(&glyph) else { continue };
            samples.push(GlyphSample {
                corpus: id.to_string(),
                sid: sid.to_string(),
                glyph,
                lane: GlyphLane::Letter,
                count,
                lane_total: round2.letter_scalars,
                context: glyph_context(text, start, start + glyph.len_utf8()),
            });
        }
    }
    samples.sort_by_key(|sample| (sample.count, sample.glyph));
    samples
}

fn analyze_glyphs(id: String, map: &VerseMap) -> GlyphCorpus {
    let mut inventory: BTreeMap<char, u64> = BTreeMap::new();
    let mut lane_totals = [0u64; 4];
    let mut decomposed_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_words: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_glyph_words: BTreeMap<char, BTreeMap<String, u64>> = BTreeMap::new();
    let mut scalar_count = 0u64;

    for text in map.values() {
        let mut previous: Option<char> = None;
        for glyph in text.chars() {
            scalar_count += 1;
            *inventory.entry(glyph).or_default() += 1;
            if let Some(lane) = glyph_lane(glyph) {
                lane_totals[lane.index()] += 1;
            }

            // This is a dependency-free preflight for the normalization seam:
            // record immediately attached base+mark pairs. Canonical equivalence
            // still needs a normalizer before composed and decomposed forms can
            // be joined as one abstract glyph.
            if class_of(glyph).is_mark()
                && let Some(base) = previous
                && !class_of(base).is_mark()
            {
                *decomposed_pairs.entry(format!("{base}{glyph}")).or_default() += 1;
            }
            previous = Some(glyph);
        }

        for token in tokenize(text) {
            let word = token.span.slice(text);
            if !is_letter_token(word) {
                continue;
            }
            let key = word.to_lowercase();
            *letter_words.entry(key.clone()).or_default() += 1;
            for glyph in word.chars().filter(|&glyph| glyph_lane(glyph) == Some(GlyphLane::Letter)) {
                *letter_glyph_words
                    .entry(glyph)
                    .or_default()
                    .entry(key.clone())
                    .or_default() += 1;
            }
        }
    }

    let candidates = glyph_candidates(&inventory, &lane_totals);
    let mut count_hist = [[0u64; GLYPH_HIST_LABELS.len()]; 4];
    for candidate in &candidates {
        count_hist[candidate.lane.index()][glyph_count_bucket(candidate.count)] += 1;
    }
    let abs_sweeps = GLYPH_ABS_KS
        .iter()
        .map(|&k| glyph_sweep(&candidates, |c| glyph_rarity_abs(c.count, k)))
        .collect();
    let rate_sweeps = GLYPH_RATE_PER_10K
        .iter()
        .map(|&rate| glyph_sweep(&candidates, |c| glyph_rarity_rate(c.count, c.lane_total, rate)))
        .collect();
    let samples = glyph_samples(&id, map, &candidates);
    let letter_round2 = letter_round2(&inventory, letter_words, letter_glyph_words);
    let retained_samples = glyph_retained_samples(&id, map, &letter_round2);

    GlyphCorpus {
        id,
        verses: map.len(),
        scalar_count,
        inventory,
        lane_totals,
        count_hist,
        abs_sweeps,
        rate_sweeps,
        decomposed_pairs,
        samples,
        letter_round2,
        retained_samples,
    }
}

fn glyph_label(glyph: char) -> String {
    format!("{} U+{:04X}", glyph.escape_default(), glyph as u32)
}

fn print_glyph_sweeps(abs: &[[GlyphSweep; 4]], rate: &[[GlyphSweep; 4]]) {
    println!("\nrecurrence sweeps (rows surface raw rarity >= {GLYPH_SWEEP_FLOOR:.2}; types / sites):");
    let describe = |sweep: &[GlyphSweep; 4]| {
        let total = glyph_sweep_total(sweep);
        let lanes = GlyphLane::ALL
            .iter()
            .map(|lane| {
                let s = sweep[lane.index()];
                format!("{} {}/{}", lane.label(), s.types, s.sites)
            })
            .collect::<Vec<_>>()
            .join("  ");
        format!("total {}/{}  {lanes}", total.types, total.sites)
    };
    println!("  absolute knee:");
    for (&k, row) in GLYPH_ABS_KS.iter().zip(abs) {
        println!("    K={k:>5.1}: {}", describe(row));
    }
    println!("  rate knee (K = 1 + rate × lane opportunities / 10k):");
    for (&rate, row) in GLYPH_RATE_PER_10K.iter().zip(rate) {
        println!("    r={rate:>5.2}: {}", describe(row));
    }
}

fn print_glyph_histogram(hist: &[[u64; GLYPH_HIST_LABELS.len()]; 4]) {
    println!("\ncandidate type-count histogram (number of glyph types):");
    print!("  {:<5}", "lane");
    for label in GLYPH_HIST_LABELS {
        print!(" {:>7}", label);
    }
    println!();
    for lane in GlyphLane::ALL {
        print!("  {:<5}", lane.label());
        for n in hist[lane.index()] {
            print!(" {n:>7}");
        }
        println!();
    }
}

fn print_glyph_samples(samples: &[GlyphSample]) {
    for sample in samples {
        let per_10k = sample.count as f64 * 10_000.0 / sample.lane_total.max(1) as f64;
        println!(
            "  {:<18} {:<10} {:<15} {} count={} lane_n={} rate={per_10k:.3}/10k | {}",
            sample.corpus,
            sample.sid,
            sample.lane.label(),
            glyph_label(sample.glyph),
            sample.count,
            sample.lane_total,
            sample.context,
        );
    }
}

#[derive(Clone, Copy, Default)]
struct LetterRound2Tally {
    base: GlyphSweep,
    closure_killed: GlyphSweep,
    lexical_killed: GlyphSweep,
    retained: GlyphSweep,
}

fn add_glyph_sweep(total: &mut GlyphSweep, add: GlyphSweep) {
    total.types += add.types;
    total.sites += add.sites;
}

fn add_letter_round2_tally(total: &mut LetterRound2Tally, add: LetterRound2Tally) {
    add_glyph_sweep(&mut total.base, add.base);
    add_glyph_sweep(&mut total.closure_killed, add.closure_killed);
    add_glyph_sweep(&mut total.lexical_killed, add.lexical_killed);
    add_glyph_sweep(&mut total.retained, add.retained);
}

fn letter_round2_tally(round2: &LetterRound2, max_count: u64, closed_alphabet: bool) -> LetterRound2Tally {
    let mut out = LetterRound2Tally::default();
    for candidate in round2.rare.iter().filter(|candidate| candidate.count <= max_count) {
        let candidate_sweep = GlyphSweep {
            types: 1,
            sites: candidate.count,
        };
        add_glyph_sweep(&mut out.base, candidate_sweep);
        if !closed_alphabet {
            add_glyph_sweep(&mut out.closure_killed, candidate_sweep);
        } else if candidate.lexical_word.is_some() {
            add_glyph_sweep(&mut out.lexical_killed, candidate_sweep);
        } else {
            add_glyph_sweep(&mut out.retained, candidate_sweep);
        }
    }
    out
}

fn kill_rate(killed: u64, base: u64) -> f64 {
    killed as f64 * 100.0 / base.max(1) as f64
}

fn print_letter_round2_single(round2: &LetterRound2) {
    println!("\nround 3 letter evidence:");
    println!(
        "  L scalars={}  hapax L scalars={}  scalar closure={:.4}%  (word types={}, round-2 word-hapax share={:.3}%)",
        round2.letter_scalars,
        round2.hapax_letter_scalars,
        round2.closure() * 100.0,
        round2.types,
        round2.word_hapax_share() * 100.0,
    );
    println!("  small-knee candidates assuming this corpus clears closure:");
    for max_count in LETTER_RARE_MAX_COUNTS {
        let tally = letter_round2_tally(round2, max_count, true);
        println!(
            "    <= {max_count}: base {}/{}  lexical-discount {}/{} ({:.1}%)  retained {}/{}",
            tally.base.types,
            tally.base.sites,
            tally.lexical_killed.types,
            tally.lexical_killed.sites,
            kill_rate(tally.lexical_killed.sites, tally.base.sites),
            tally.retained.types,
            tally.retained.sites,
        );
    }
    let lexical: Vec<_> = round2.rare.iter().filter(|candidate| candidate.lexical_word.is_some()).collect();
    println!("  lexical-concentration discounts (first {} of {}):", lexical.len().min(20), lexical.len());
    for candidate in lexical.iter().take(20) {
        println!(
            "    {:<15} count={} word={} ({} tokens)",
            glyph_label(candidate.glyph),
            candidate.count,
            candidate.lexical_word.as_deref().unwrap_or_default(),
            candidate.lexical_word_tokens,
        );
    }
}

fn glyph_single_report(corpus: &GlyphCorpus) {
    println!("=== RARE-GLYPH SPIKE: {} ({} verses) ===", corpus.id, corpus.verses);
    println!(
        "raw scalar inventory: {} occurrences / {} distinct scalars",
        corpus.scalar_count,
        corpus.inventory.len()
    );
    println!("candidate lane opportunities:");
    for lane in GlyphLane::ALL {
        let types = corpus.inventory.keys().filter(|&&c| glyph_lane(c) == Some(lane)).count();
        println!("  {}  {:>10} occurrences / {:>5} glyph types", lane.label(), corpus.lane_totals[lane.index()], types);
    }
    print_glyph_histogram(&corpus.count_hist);
    print_glyph_sweeps(&corpus.abs_sweeps, &corpus.rate_sweeps);
    print_letter_round2_single(&corpus.letter_round2);

    let mut candidates = glyph_candidates(&corpus.inventory, &corpus.lane_totals);
    candidates.sort_by_key(|c| (c.count, std::cmp::Reverse(c.lane_total), c.glyph));
    println!("\nrarest candidate glyphs (first {} of {}):", candidates.len().min(120), candidates.len());
    println!("  {:<15} {:<5} {:>8} {:>12} {:>14}", "glyph", "lane", "count", "lane total", "rate /10k");
    for candidate in candidates.iter().take(120) {
        let rate = candidate.count as f64 * 10_000.0 / candidate.lane_total.max(1) as f64;
        println!(
            "  {:<15} {:<5} {:>8} {:>12} {:>14.3}",
            glyph_label(candidate.glyph),
            candidate.lane.label(),
            candidate.count,
            candidate.lane_total,
            rate,
        );
    }

    let mut decomposed: Vec<_> = corpus.decomposed_pairs.iter().collect();
    decomposed.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    println!("\ndecomposed base+mark preflight (top 20; canonical pairing not yet joined):");
    if decomposed.is_empty() {
        println!("  none");
    } else {
        for (pair, count) in decomposed.iter().take(20) {
            println!("  {pair:?}  {count}");
        }
    }
    println!("\nsample high-rarity candidates (absolute K=32):");
    print_glyph_samples(&corpus.samples);
}

/// Fleet report: workers drop each raw inventory after deriving a compact
/// summary. The aggregate keeps only reproducible rollups and bounded samples.
fn glyph_fleet(dir: &Path) {
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
    eprintln!("rare-glyph fleet: {total} corpora in {}", dir.display());

    let mut lane_totals = [0u64; 4];
    let mut count_hist = [[0u64; GLYPH_HIST_LABELS.len()]; 4];
    let mut abs_sweeps = vec![[GlyphSweep::default(); 4]; GLYPH_ABS_KS.len()];
    let mut rate_sweeps = vec![[GlyphSweep::default(); 4]; GLYPH_RATE_PER_10K.len()];
    let mut noisiest: Vec<(String, [u64; 4], [u64; 4], u64)> = Vec::new();
    let mut samples = Vec::new();
    let mut decomposed: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut round2 = vec![vec![LetterRound2Tally::default(); LETTER_RARE_MAX_COUNTS.len()]; CLOSURE_SCALAR_SHARES.len()];
    let mut open_corpora = vec![0u64; CLOSURE_SCALAR_SHARES.len()];
    // (id, L scalars, hapax L scalars, scalar closure ppm, word-hapax share ppm)
    let mut closure_rows: Vec<(String, u64, u64, u64, u64)> = Vec::new();
    // Round-3 sanity checks: corpora that flip open (closed word-hapax → open
    // scalar closure), retained review leads, and lexical-kill mechanism leads.
    let mut flips: Vec<(String, u64, u64)> = Vec::new();
    let mut retained_samples: Vec<GlyphSample> = Vec::new();
    let mut lexical_kill_leads: Vec<(String, char, String, u64)> = Vec::new();
    let t0 = std::time::Instant::now();
    let done = AtomicUsize::new(0);
    let corpora: Vec<GlyphFleetSummary> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let summary = GlyphFleetSummary::from(analyze_glyphs(id, &load_corpus(path)));
            let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
            if completed.is_multiple_of(100) {
                eprintln!("  …{completed}/{total}");
            }
            summary
        })
        .collect();
    eprintln!("rare-glyph fleet analyze: {:?}", t0.elapsed());

    for corpus in corpora {
        for lane in GlyphLane::ALL {
            lane_totals[lane.index()] += corpus.lane_totals[lane.index()];
            for (sum, value) in count_hist[lane.index()].iter_mut().zip(corpus.count_hist[lane.index()]) {
                *sum += value;
            }
        }
        for (sum, value) in abs_sweeps.iter_mut().zip(&corpus.abs_sweeps) {
            for (sum, value) in sum.iter_mut().zip(value) {
                sum.types += value.types;
                sum.sites += value.sites;
            }
        }
        for (sum, value) in rate_sweeps.iter_mut().zip(&corpus.rate_sweeps) {
            for (sum, value) in sum.iter_mut().zip(value) {
                sum.types += value.types;
                sum.sites += value.sites;
            }
        }
        let abs_ref = corpus.abs_sweeps[4].map(|sweep| sweep.sites); // K=32
        let rate_ref = corpus.rate_sweeps[3].map(|sweep| sweep.sites); // 2/10k
        noisiest.push((corpus.id.clone(), abs_ref, rate_ref, corpus.scalar_count));
        let closure = corpus.letter_round2.closure();
        let word_hapax = corpus.letter_round2.word_hapax_share();
        closure_rows.push((
            corpus.id.clone(),
            corpus.letter_round2.letter_scalars,
            corpus.letter_round2.hapax_letter_scalars,
            (closure * 1_000_000.0).round() as u64,
            (word_hapax * 1_000_000.0).round() as u64,
        ));
        // Flip = closed under the round-2 word-hapax gate (>0.5%, the round-2
        // representative), open under the round-3 scalar gate (≤0.1%).
        if word_hapax > 0.005 && closure <= RETAINED_SAMPLE_THRESHOLD {
            flips.push((
                corpus.id.clone(),
                (word_hapax * 1_000_000.0).round() as u64,
                (closure * 1_000_000.0).round() as u64,
            ));
        }
        for (threshold_index, &threshold) in CLOSURE_SCALAR_SHARES.iter().enumerate() {
            let open = closure <= threshold;
            if open {
                open_corpora[threshold_index] += 1;
            }
            for (knee_index, &max_count) in LETTER_RARE_MAX_COUNTS.iter().enumerate() {
                add_letter_round2_tally(
                    &mut round2[threshold_index][knee_index],
                    letter_round2_tally(&corpus.letter_round2, max_count, open),
                );
            }
        }
        // Lexical-kill mechanism leads at knee ≤1: count==1 letter scalars whose
        // occurrence folds into a repeated word type. Uppercase glyph here proves
        // the suspected uppercase-folds-into-repeated-lowercase-word mechanism.
        if closure <= RETAINED_SAMPLE_THRESHOLD {
            for cand in corpus
                .letter_round2
                .rare
                .iter()
                .filter(|c| c.count == 1 && c.lexical_word.is_some())
            {
                if lexical_kill_leads.len() < 20 {
                    lexical_kill_leads.push((
                        corpus.id.clone(),
                        cand.glyph,
                        cand.lexical_word.clone().unwrap_or_default(),
                        cand.lexical_word_tokens,
                    ));
                }
            }
            retained_samples.extend(corpus.retained_samples.iter().cloned());
        }
        samples.extend(corpus.samples);
        for (pair, &count) in &corpus.decomposed_pairs {
            let row = decomposed.entry(pair.clone()).or_default();
            row.0 += count;
            row.1 += 1;
        }
    }
    eprintln!("rare-glyph fleet tally: {:?}", t0.elapsed());

    println!("=== RARE-GLYPH SPIKE — fleet aggregate ({total} corpora) ===");
    println!("candidate lane opportunities:");
    for lane in GlyphLane::ALL {
        println!("  {}  {}", lane.label(), lane_totals[lane.index()]);
    }
    print_glyph_histogram(&count_hist);
    print_glyph_sweeps(&abs_sweeps, &rate_sweeps);

    println!("\nround 3 L-only stack (base is the small absolute knee; all counts are sites):");
    println!("  closure threshold is hapax L-scalar types / all L-scalar occurrences (letter-SCALAR closure).");
    for (threshold_index, &threshold) in CLOSURE_SCALAR_SHARES.iter().enumerate() {
        println!(
            "  scalar closure <= {:.4}%: {}/{} corpora open the L lane",
            threshold * 100.0,
            open_corpora[threshold_index],
            total
        );
        for (knee_index, &max_count) in LETTER_RARE_MAX_COUNTS.iter().enumerate() {
            let tally = round2[threshold_index][knee_index];
            println!(
                "    <= {max_count}: base {:>6}; closure -{:>6} ({:>5.1}%); lexical -{:>6} ({:>5.1}%); keep {:>6}",
                tally.base.sites,
                tally.closure_killed.sites,
                kill_rate(tally.closure_killed.sites, tally.base.sites),
                tally.lexical_killed.sites,
                kill_rate(tally.lexical_killed.sites, tally.base.sites),
                tally.retained.sites,
            );
        }
    }

    // Highest scalar closure = open-inventory corpora that self-silence.
    closure_rows.sort_by_key(|(_, _, _, closure_ppm, _)| std::cmp::Reverse(*closure_ppm));
    println!("\nhighest letter-SCALAR closure (open-inventory self-disable, stay closed):");
    for (id, scalars, hapaxes, closure_ppm, word_ppm) in closure_rows.iter().take(20) {
        println!(
            "  {id:<24} {}/{} = {:.4}%  (word-hapax {:.3}%)",
            hapaxes,
            scalars,
            *closure_ppm as f64 / 10_000.0,
            *word_ppm as f64 / 10_000.0,
        );
    }

    // Sanity: corpora that flip open under scalar closure but were closed under
    // the round-2 word-hapax gate — the agglutinative Latin-script class.
    flips.sort_by_key(|(_, word_ppm, _)| std::cmp::Reverse(*word_ppm));
    println!(
        "\nflip-open corpora (word-hapax >0.5% [closed in round 2] but scalar closure <=0.1% [open now]): {} total",
        flips.len()
    );
    for (id, word_ppm, closure_ppm) in flips.iter().take(25) {
        println!(
            "  {id:<24} word-hapax {:.3}%  scalar closure {:.4}%",
            *word_ppm as f64 / 10_000.0,
            *closure_ppm as f64 / 10_000.0,
        );
    }

    // Sanity: confirm the mechanism of the knee≤1 lexical kills.
    println!(
        "\nlexical kills at knee<=1 (count==1 L scalar folding into a repeated word type): {} leads",
        lexical_kill_leads.len()
    );
    for (id, glyph, word, word_tokens) in lexical_kill_leads.iter().take(20) {
        let upper = glyph.is_uppercase();
        println!(
            "  {id:<20} {} -> word {word:?} ({word_tokens} tokens){}",
            glyph_label(*glyph),
            if upper { "  [uppercase → folds to repeated lowercase]" } else { "" },
        );
    }

    // Retained review table: ~30 diverse retained sites (letter, count<=3, not
    // lexical) in corpora open at the representative closure threshold.
    retained_samples.sort_by_key(|s| (s.corpus.clone(), s.count, s.glyph));
    retained_samples.dedup_by(|a, b| a.corpus == b.corpus && a.glyph == b.glyph);
    let mut diverse: Vec<GlyphSample> = Vec::new();
    let mut per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for sample in &retained_samples {
        let seen = per_corpus.entry(sample.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            diverse.push(sample.clone());
        }
    }
    println!(
        "\nretained review table ({} of {} retained leads; closure<={:.3}%, knee<={}, non-lexical):",
        diverse.len().min(30),
        retained_samples.len(),
        RETAINED_SAMPLE_THRESHOLD * 100.0,
        RETAINED_SAMPLE_KNEE,
    );
    print_glyph_samples(&diverse.into_iter().take(30).collect::<Vec<_>>());

    noisiest.sort_by_key(|(_, abs, rate, _)| {
        (
            std::cmp::Reverse(abs.iter().sum::<u64>()),
            std::cmp::Reverse(rate.iter().sum::<u64>()),
        )
    });
    println!("\nnoisiest corpora (raw-rarity reference: absolute K=32, rate=2/10k):");
    for (id, abs, rate, scalars) in noisiest.iter().take(20) {
        println!(
            "  {id:<24} abs L/N/P/S={}/{}/{}/{}  rate={}/{}/{}/{}  raw {scalars:>9} scalars",
            abs[0], abs[1], abs[2], abs[3], rate[0], rate[1], rate[2], rate[3],
        );
    }

    let mut decomposed: Vec<_> = decomposed.into_iter().collect();
    decomposed.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    println!("\ndecomposed base+mark preflight across the fleet (top 20):");
    for (pair, (count, corpora)) in decomposed.iter().take(20) {
        println!("  {pair:?}  {count:>8} occurrences in {corpora} corpora");
    }

    println!("\nreview samples by lane (absolute K=32):");
    for lane in GlyphLane::ALL {
        let mut lane_samples: Vec<_> = samples
            .iter()
            .filter(|sample| sample.lane == lane)
            .cloned()
            .collect();
        lane_samples.sort_by_key(|sample| {
            (std::cmp::Reverse(sample.lane_total), sample.count, sample.glyph)
        });
        println!("  [{}]", lane.label());
        print_glyph_samples(&lane_samples.into_iter().take(12).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    fn one_verse(text: &str) -> VerseMap {
        let mut map = VerseMap::new();
        map.insert(
            ssc_core::Sid::new(BookId::from_str("GEN").unwrap(), 1, 1),
            text.to_string(),
        );
        map
    }

    #[test]
    fn visible_candidate_lanes_cover_stated_examples_only() {
        assert_eq!(glyph_lane('q'), Some(GlyphLane::Letter));
        assert_eq!(glyph_lane('¹'), Some(GlyphLane::Number));
        assert_eq!(glyph_lane('“'), Some(GlyphLane::Punctuation));
        assert_eq!(glyph_lane('='), Some(GlyphLane::Symbol));
        assert_eq!(glyph_lane('\u{301}'), None);
        assert_eq!(glyph_lane(' '), None);
        assert_eq!(glyph_lane('\u{FFFD}'), None);
    }

    #[test]
    fn rate_knee_expands_with_lane_volume() {
        assert_eq!(glyph_rarity_abs(1, 32.0), 1.0);
        assert!(glyph_rarity_rate(32, 500_000, 2.0) > glyph_rarity_abs(32, 32.0));
    }

    #[test]
    fn closure_uses_hapax_letter_scalar_share() {
        // "alpha alpha alpha": a×6, l×3, p×3, h×3 — no scalar seen once.
        let closed = analyze_glyphs("closed".to_string(), &one_verse("alpha alpha alpha"));
        assert_eq!(closed.letter_round2.hapax_letter_scalars, 0);
        assert_eq!(closed.letter_round2.letter_scalars, 15);
        assert_eq!(closed.letter_round2.closure(), 0.0);

        // "alpha beta gamma": a×5, m×2 repeat; l,p,h,b,e,t,g each once (7 hapax
        // scalars) of 14 L occurrences → 0.5. Scalar closure, not word closure:
        // even with three distinct (word-hapax=1.0) word types the alphabet is
        // half-closed.
        let open = analyze_glyphs("open".to_string(), &one_verse("alpha beta gamma"));
        assert_eq!(open.letter_round2.hapax_letter_scalars, 7);
        assert_eq!(open.letter_round2.letter_scalars, 14);
        assert_eq!(open.letter_round2.closure(), 0.5);
        assert_eq!(open.letter_round2.word_hapax_share(), 1.0);
    }

    #[test]
    fn lexical_discount_requires_one_repeated_word_type() {
        let concentrated = analyze_glyphs("concentrated".to_string(), &one_verse("Xerxes Xerxes"));
        let x = concentrated
            .letter_round2
            .rare
            .iter()
            .find(|candidate| candidate.glyph == 'X')
            .unwrap();
        assert_eq!(x.lexical_word.as_deref(), Some("xerxes"));
        assert_eq!(x.lexical_word_tokens, 2);

        let scattered = analyze_glyphs("scattered".to_string(), &one_verse("Xenon Xylophone"));
        let x = scattered
            .letter_round2
            .rare
            .iter()
            .find(|candidate| candidate.glyph == 'X')
            .unwrap();
        assert!(x.lexical_word.is_none());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// terminal_strength SPIKE (shortlist 2/3). Per-mark boundary trust wired into
// ADR 0051 casing; reports witness measurements, per-mark fleet trust, the W2
// variant comparison (genealogy guard), the sigmoid refit evidence, and the
// wiring deltas vs the shipped baseline. Knobs NOT frozen — measurement only.
// ═══════════════════════════════════════════════════════════════════════════

use terminal::{ClassKey, ClassTrust, TermCorpus};

/// median, p25, p75, max of a sample (sorts in place).
fn quartiles(v: &mut [f64]) -> (f64, f64, f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    (at(0.25), at(0.5), at(0.75), at(1.0))
}

fn deviate(c: &ClassTrust) -> f64 {
    if c.df == 0 {
        0.0
    } else {
        (c.g2_after - c.df as f64) / (2.0 * c.df as f64).sqrt()
    }
}

/// Detailed single-corpus terminal-strength report.
fn terminal_single(c: &TermCorpus) {
    println!("=== terminal_strength SPIKE: {} ({} verses, {}) ===",
        c.id, c.verses, if c.bicameral { "bicameral" } else { "caseless" });
    println!("jurors (word-starts ≥10): {}  dropped classes (<30 events): {}",
        c.trust.n_jurors, c.trust.dropped_classes);
    if let Some(r) = c.trust.reference {
        println!("agreement reference class: {}", r.label());
    }
    println!("\nper-class witnesses (sorted by trust_B):");
    println!("  {:<8} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>7} {:>7} {:>7}",
        "class", "events", "s_case", "dev", "diff", "agree", "asym", "sR_A", "trustA", "trustB");
    let mut cls: Vec<&ClassTrust> = c.trust.classes.values().collect();
    cls.sort_by(|a, b| b.trust_b.partial_cmp(&a.trust_b).unwrap());
    for t in cls {
        println!("  {:<8} {:>7} {:>7.3} {:>6.1} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>7.3} {:>7.3}",
            t.class.label(), t.events,
            if t.s_case_seen { t.s_case } else { f64::NAN },
            deviate(t), t.diff, t.agree, t.asym, t.s_reshuffle_a, t.trust_a, t.trust_b);
    }
    println!("\nwiring deltas (floor 0.95, k=32) — baseline vs trust-wired (variant B):");
    println!("  intrinsic  {:>6} → {:<6}", c.base_i, c.tr_i);
    println!("  positional {:>6} → {:<6}", c.base_p, c.tr_p);
    println!("  both       {:>6} → {:<6}", c.base_b, c.tr_b);
    println!("  pool: gained-cap {}  lost-cap {}  intrinsic-flip {:+}",
        c.pool_gained, c.pool_lost, c.intrinsic_flips);
    println!("  quote-context sites promoted & surfaced: {}", c.promoted_surfaced);
    if !c.anchors.is_empty() {
        println!("\nanchor fates:");
        for a in &c.anchors {
            println!("  {:<9} {:<11} base={:.3}({}) tr={:.3}({}) quad={} class={} trust={:.3} habit={:.3}",
                a.sid, a.word, a.base_score, if a.base_alive {"alive"} else {"dead"},
                a.tr_score, if a.tr_alive {"alive"} else {"dead"}, a.quad, a.class, a.trust, a.habit);
        }
    }
    let mut ch: Vec<&terminal::Change> = c.changes.iter().collect();
    ch.sort_by(|a, b| b.tr_score.max(b.base_score).partial_cmp(&a.tr_score.max(a.base_score)).unwrap());
    println!("\nverdict changes ({} total; up to 25):", c.changes.len());
    for x in ch.iter().take(25) {
        println!("  [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
            x.direction, x.sid, x.word, x.base_score, x.tr_score, x.quad,
            x.trust, x.habit, x.dominance, x.minority, x.rarity, x.ctx);
    }
    if !c.samples_promoted.is_empty() {
        println!("\npromoted quote-context sites (up to 15):");
        for s in c.samples_promoted.iter().take(15) {
            println!("  {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                s.sid, s.word, s.class, s.trust, s.score, s.ctx);
        }
    }
}

const MAJOR: &[&str] = &[
    "eng-web", "eng-kjv", "engwebster", "WA-en-ulb", "spaRV1909", "WA-es-419-ulb",
    "fraLSG", "WA-fr-ulb", "porblt", "ita1885", "ron1924", "deu1912", "swhulb",
    "WA-sw-ulb", "ind", "nld", "vie1934", "tglulb",
];

/// Fleet aggregate: per-mark trust distributions, W2 variant comparison,
/// sigmoid-refit evidence, and casing wiring deltas vs the shipped baseline.
fn terminal_fleet(dir: &Path, variant_b: bool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use rayon::prelude::*;

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    files.sort();
    let total = files.len();
    eprintln!("terminal fleet: {total} corpora (W2 variant {})", if variant_b {"B (guarded)"} else {"A (plain)"});
    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<TermCorpus> = files.par_iter().map(|path| {
        let id = path.file_stem().unwrap().to_string_lossy().to_string();
        let map = load_corpus(path);
        let c = terminal::analyze_corpus(id, &map, variant_b);
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(200) { eprintln!("  …{n}/{total}"); }
        c
    }).collect();
    eprintln!("terminal fleet evaluate: {:?}", t0.elapsed());

    // ── Per-mark trust distributions (bare classes) across the fleet. ──
    // Collect per (mark, quoted) the trust/witness samples over corpora.
    let mut by_class: BTreeMap<ClassKey, Vec<&ClassTrust>> = BTreeMap::new();
    for c in &corpora {
        for t in c.trust.classes.values() {
            by_class.entry(t.class).or_default().push(t);
        }
    }
    let focus_bare = ['.', ',', '?', '!', ':', ';', '\u{2014}', '"', '\u{201D}', '-', '\u{2026}'];
    println!("\n=== TERMINAL_STRENGTH SPIKE — fleet ({} corpora) ===", corpora.len());
    println!("\n-- per-mark trust distribution (bare classes; median [p25,p75] max over corpora) --");
    println!("  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
        "mark", "corpora", "s_case", "s_reshuffle_A(diff)", "trust_A", "trust_B");
    let fmtq = |v: &mut Vec<f64>| { let (p25,med,p75,mx)=quartiles(v); format!("{med:.2}[{p25:.2},{p75:.2}]mx{mx:.2}") };
    for &m in &focus_bare {
        let key = ClassKey { mark: m, quoted: false };
        if let Some(ts) = by_class.get(&key) {
            let mut sc: Vec<f64> = ts.iter().filter(|t| t.s_case_seen).map(|t| t.s_case).collect();
            let mut di: Vec<f64> = ts.iter().map(|t| t.s_reshuffle_a).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            println!("  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
                key.label(), ts.len(), fmtq(&mut sc), fmtq(&mut di), fmtq(&mut ta), fmtq(&mut tb));
        }
    }
    println!("\n-- quote-context classes (mark+\") — the shortlist item-7 sweep --");
    println!("  {:<7} {:>7} {:>24} {:>24}", "class", "corpora", "trust_B", "trust_A");
    for &m in &['.', '?', '!', ':', ',', ';'] {
        let key = ClassKey { mark: m, quoted: true };
        if let Some(ts) = by_class.get(&key) {
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            println!("  {:<7} {:>7} {:>24} {:>24}", key.label(), ts.len(), fmtq(&mut tb), fmtq(&mut ta));
        }
    }

    // ── Sigmoid-refit evidence: standardized deviate for '.' vs ','. ──
    println!("\n-- W2 sigmoid refit evidence: standardized multinomial-G² deviate --");
    for &m in &['.', ',', '?', '!', ':'] {
        let key = ClassKey { mark: m, quoted: false };
        if let Some(ts) = by_class.get(&key) {
            let mut d: Vec<f64> = ts.iter().map(|t| deviate(t)).collect();
            let (p25, med, p75, mx) = quartiles(&mut d);
            println!("  {:<5} dev median {med:.1} [{p25:.1},{p75:.1}] max {mx:.1}", key.label());
        }
    }

    // ── W2 variant comparison: genealogy guard — worst comma offenders. ──
    println!("\n-- genealogy guard: corpora where ',' is most over-trusted by variant A --");
    let mut comma_rows: Vec<(&str, f64, f64, f64, f64)> = corpora.iter().filter_map(|c| {
        c.trust.classes.get(&ClassKey{mark:',',quoted:false}).map(|t|
            (c.id.as_str(), t.trust_a, t.trust_b, t.diff, t.agree))
    }).collect();
    comma_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    println!("  {:<20} {:>8} {:>8} {:>8} {:>8}", "corpus", "trustA", "trustB", "diff", "agree");
    for (id, ta, tb, d, ag) in comma_rows.iter().take(12) {
        println!("  {:<20} {:>8.3} {:>8.3} {:>8.3} {:>8.3}", id, ta, tb, d, ag);
    }

    // ── Wiring deltas vs baseline. ──
    let (mut bi, mut bp, mut bb, mut ti, mut tp, mut tb) = (0u64,0u64,0u64,0u64,0u64,0u64);
    let (mut pg, mut pl) = (0u64, 0u64);
    let mut promoted = 0u64;
    let mut corpora_changed = 0u32;
    for c in &corpora {
        bi += c.base_i; bp += c.base_p; bb += c.base_b;
        ti += c.tr_i; tp += c.tr_p; tb += c.tr_b;
        pg += c.pool_gained; pl += c.pool_lost;
        promoted += c.promoted_surfaced;
        if !c.changes.is_empty() { corpora_changed += 1; }
    }
    println!("\n-- wiring deltas (floor 0.95, k=32; variant {}) --", if variant_b {"B"} else {"A"});
    println!("  channel     baseline   trust-wired      Δ");
    println!("  intrinsic  {:>9} {:>13} {:>+7}", bi, ti, ti as i64 - bi as i64);
    println!("  positional {:>9} {:>13} {:>+7}", bp, tp, tp as i64 - bp as i64);
    println!("  both       {:>9} {:>13} {:>+7}", bb, tb, tb as i64 - bb as i64);
    println!("  TOTAL      {:>9} {:>13} {:>+7}", bi+bp+bb, ti+tp+tb,
        (ti+tp+tb) as i64 - (bi+bp+bb) as i64);
    println!("  corpora with ≥1 verdict change: {}", corpora_changed);
    println!("  pool recovery: word profiles gained-cap {pg}, lost-cap {pl}");
    println!("  quote-context sites promoted & surfaced (item-7 payoff): {promoted}");

    // ── Anchor fates. ──
    println!("\n-- anchor fates (12 ADR 0051 anchors) --");
    println!("  {:<9} {:<11} {:<10} {:>7} {:>7} {:<7} {:<10} {:>6} {:>6}",
        "corpus", "sid", "word", "base", "tr", "verdict", "class", "trust", "habit");
    for &(ac, asid, aw) in terminal::ANCHORS {
        let fate = corpora.iter().flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                let verdict = match (a.base_alive, a.tr_alive) {
                    (true, true) => "kept", (true, false) => "DIED",
                    (false, true) => "born", (false, false) => "silent",
                };
                println!("  {:<9} {:<11} {:<10} {:>7.3} {:>7.3} {:<7} {:<10} {:>6.3} {:>6.3}",
                    ac, asid, aw, a.base_score, a.tr_score, verdict, a.class, a.trust, a.habit);
            }
            None => println!("  {:<9} {:<11} {:<10}  (not a candidate site)", ac, asid, aw),
        }
    }

    // ── Top-10 corpora by positional-channel change. ──
    let mut ranked: Vec<&TermCorpus> = corpora.iter().filter(|c| c.pos_delta > 0).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.pos_delta));
    println!("\n-- top-10 corpora by positional-channel change --");
    for c in ranked.iter().take(10) {
        println!("  {:<20} pos {}→{} (Δ{:+})  examples:", c.id, c.base_p, c.tr_p, c.tr_p as i64 - c.base_p as i64);
        let mut ch: Vec<&terminal::Change> = c.changes.iter().filter(|x| x.quad != "intrinsic").collect();
        ch.sort_by(|a,b| b.tr_score.max(b.base_score).partial_cmp(&a.tr_score.max(a.base_score)).unwrap());
        for x in ch.iter().take(3) {
            println!("      [{}] {:<9} {:<12} base={:.3} tr={:.3} trust={:.3} | {}",
                x.direction, x.sid, x.word, x.base_score, x.tr_score, x.trust, x.ctx);
        }
    }

    // ── Changed-verdict samples from major-language corpora. ──
    println!("\n-- changed-verdict samples from major-language corpora (parametric review) --");
    let mut shown = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.changes.is_empty() { continue; }
        println!("  [{}]:", c.id);
        let mut ch: Vec<&terminal::Change> = c.changes.iter().collect();
        ch.sort_by(|a,b| b.tr_score.max(b.base_score).partial_cmp(&a.tr_score.max(a.base_score)).unwrap());
        for x in ch.iter().take(3) {
            println!("    [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
                x.direction, x.sid, x.word, x.base_score, x.tr_score, x.quad,
                x.trust, x.habit, x.dominance, x.minority, x.rarity, x.ctx);
            shown += 1;
        }
        if shown >= 25 { break; }
    }

    // ── Context-class payoff samples. ──
    println!("\n-- promoted quote-context sites from major-language corpora (item-7) --");
    let mut cnt = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples_promoted.is_empty() { continue; }
        for s in c.samples_promoted.iter().take(2) {
            println!("  [{}] {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.ctx);
            cnt += 1;
        }
        if cnt >= 10 { break; }
    }
    if cnt == 0 { println!("  (none surfaced in major-language corpora)"); }

    terminal_gate_sweep(&corpora, bi, bp, bb, ti, tp, tb, promoted, variant_b);
}

/// Gate-threshold sweep report (2026-07-10). `b*` are the shipped-baseline
/// channel totals, `t*` the multiplier wiring, `mult_promoted` the multiplier's
/// promoted-and-surfaced count (the 237). Each item mirrors the ADR packet.
#[allow(clippy::too_many_arguments)]
fn terminal_gate_sweep(
    corpora: &[TermCorpus],
    bi: u64, bp: u64, bb: u64,
    ti: u64, tp: u64, tb: u64,
    mult_promoted: u64,
    variant_b: bool,
) {
    let sweep = terminal::GATE_SWEEP;
    let n_t = sweep.len();
    let base_total = bi + bp + bb;
    let mult_total = ti + tp + tb;

    println!("\n═══ GATE-THRESHOLD SWEEP (2026-07-10; variant {}) ═══",
        if variant_b { "B" } else { "A" });

    // 1. Surfaced volume per channel + deltas vs baseline and multiplier.
    println!("\n-- 1. surfaced volume per channel (fleet) --");
    println!("  baseline (shipped): i {bi}  p {bp}  b {bb}  TOTAL {base_total}");
    println!("  multiplier wiring:  i {ti}  p {tp}  b {tb}  TOTAL {mult_total}");
    println!("  {:<5} {:>8} {:>9} {:>6} {:>8} {:>10} {:>10}",
        "T", "intrins", "positnl", "both", "TOTAL", "Δ vs base", "Δ vs mult");
    for (i, &t) in sweep.iter().enumerate() {
        let (mut gi, mut gp, mut gb) = (0u64, 0u64, 0u64);
        for c in corpora {
            let (a, b2, c2) = c.gate.counts[i];
            gi += a; gp += b2; gb += c2;
        }
        let total = gi + gp + gb;
        println!("  {:<5.2} {:>8} {:>9} {:>6} {:>8} {:>+10} {:>+10}",
            t, gi, gp, gb, total,
            total as i64 - base_total as i64, total as i64 - mult_total as i64);
    }

    // 2. Middle population: sites lost between adjacent thresholds.
    println!("\n-- 2. middle population: sites gated off between adjacent T --");
    println!("  {:<14} {:>7}   classes (mark: count)", "step", "sites");
    for i in 0..n_t - 1 {
        let (lo, hi) = (sweep[i], sweep[i + 1]);
        let mut total = 0u64;
        let mut classes: BTreeMap<ClassKey, u64> = BTreeMap::new();
        for c in corpora {
            total += c.gate.step_lost[i];
            for (k, v) in &c.gate.step_classes[i] {
                *classes.entry(*k).or_default() += v;
            }
        }
        let mut cv: Vec<(&ClassKey, &u64)> = classes.iter().collect();
        cv.sort_by(|a, b| b.1.cmp(a.1));
        let cs = cv.iter().take(6)
            .map(|(k, v)| format!("{}:{}", k.label(), v))
            .collect::<Vec<_>>().join("  ");
        println!("  {:<14} {:>7}   {}", format!("{lo:.2}→{hi:.2}"), total, cs);
    }

    // 3. The 12 ADR 0051 anchors, alive at each threshold (first 7 = TP).
    println!("\n-- 3. the 12 ADR 0051 anchors: alive at each threshold --");
    print!("  {:<9} {:<11} {:<10} {:<4} {:<4}", "corpus", "sid", "word", "base", "mult");
    for &t in sweep { print!(" {:>5.2}", t); }
    println!("   kind");
    let mut tp_deaths: Vec<(String, f64)> = Vec::new();
    for (idx, &(ac, asid, aw)) in terminal::ANCHORS.iter().enumerate() {
        let is_tp = idx < 7;
        let fate = corpora.iter().flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                print!("  {:<9} {:<11} {:<10} {:<4} {:<4}",
                    ac, asid, aw,
                    if a.base_alive { "✓" } else { "·" },
                    if a.tr_alive { "✓" } else { "·" });
                for (i, &t) in sweep.iter().enumerate() {
                    print!(" {:>5}", if a.gate_alive[i] { "✓" } else { "·" });
                    if is_tp && !a.gate_alive[i] {
                        tp_deaths.push((format!("{ac} {asid} {aw} @T={t:.2}"), a.gate_score[i]));
                    }
                }
                println!("   {}", if is_tp { "TP" } else { "FP" });
            }
            None => println!("  {:<9} {:<11} {:<10}  (not a candidate site)", ac, asid, aw),
        }
    }
    if tp_deaths.is_empty() {
        println!("  ALL 7 TPs stay alive at every swept threshold.");
    } else {
        println!("  ⚠ TP deaths: {}",
            tp_deaths.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>().join(", "));
    }

    // 4. Readmissions vs the multiplier wiring.
    println!("\n-- 4. readmissions vs the multiplier wiring (fleet) --");
    for (i, &t) in sweep.iter().enumerate() {
        let r: u64 = corpora.iter().map(|c| c.gate.readmitted[i]).sum();
        println!("  T={t:.2}: {r} findings the multiplier eroded, readmitted by the gate");
    }
    // The documented-known fraLSG MIC 2:6 disent-ils FP (expected readmitted).
    let fralsg = corpora.iter().find(|c| c.id == "fraLSG")
        .and_then(|c| c.gate.readmit_samples.iter().find(|s| s.sid == "MIC 2:6"));
    match fralsg {
        Some(s) => println!(
            "  fraLSG MIC 2:6 [{}]: trust={:.3} gate-score={:.3} base={:.3} (readmitted; known FP) | {}",
            s.word, s.trust, s.score, s.base_score, s.ctx),
        None => println!("  fraLSG MIC 2:6 disent-ils: NOT in the readmit set (unexpected — investigate)"),
    }
    // Per-major-corpus readmit tally (T=0.50, the maximal readmit set) — shows
    // how much of the fleet-wide readmission lands in major vs minority langs.
    println!("\n  readmit count per major-language corpus (T=0.50):");
    let major_readmit: u64 = corpora.iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()))
        .map(|c| c.gate.readmitted[0]).sum();
    let mut mr: Vec<(&str, u64)> = corpora.iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()) && c.gate.readmitted[0] > 0)
        .map(|c| (c.id.as_str(), c.gate.readmitted[0])).collect();
    mr.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("    {} of {} fleet readmissions land in major-language corpora: {}",
        major_readmit, corpora.iter().map(|c| c.gate.readmitted[0]).sum::<u64>(),
        mr.iter().map(|(id, n)| format!("{id}:{n}")).collect::<Vec<_>>().join("  "));
    println!("\n  readmitted-site sample from major-language corpora (verse text):");
    let mut shown = 0;
    for c in corpora {
        if !MAJOR.contains(&c.id.as_str()) { continue; }
        for s in &c.gate.readmit_samples {
            println!("    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx);
            shown += 1;
            if shown >= 10 { break; }
        }
        if shown >= 10 { break; }
    }
    if shown == 0 { println!("    (no readmissions in major-language corpora)"); }
    // Erosion lands overwhelmingly in minority-language corpora, so also show a
    // fleet-wide sample from the highest-readmit corpora for adjudication.
    println!("\n  fleet-wide readmitted-site sample (highest-readmit corpora):");
    let mut ranked: Vec<&TermCorpus> = corpora.iter()
        .filter(|c| c.gate.readmitted[0] > 0).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.gate.readmitted[0]));
    let mut fshown = 0;
    for c in ranked {
        for s in c.gate.readmit_samples.iter().take(2) {
            println!("    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx);
            fshown += 1;
            if fshown >= 10 { break; }
        }
        if fshown >= 10 { break; }
    }

    // 5. The 237 promoted quote-context sites: survival at each threshold.
    println!("\n-- 5. promoted quote-context sites surviving at each threshold --");
    println!("  multiplier wiring promoted & surfaced: {mult_promoted}");
    for (i, &t) in sweep.iter().enumerate() {
        let s: u64 = corpora.iter().map(|c| c.gate.promoted_survived[i]).sum();
        println!("  T={t:.2}: {s} promoted quote-context sites survive");
    }

    // 6. Corpora that lose ALL positional coverage at each threshold.
    println!("\n-- 6. corpora that lose ALL positional coverage at each threshold --");
    for (i, &t) in sweep.iter().enumerate() {
        let mut losers: Vec<&TermCorpus> = corpora.iter()
            .filter(|c| c.gate.base_pos > 0 && !c.gate.pos_alive[i]).collect();
        losers.sort_by_key(|c| std::cmp::Reverse(c.gate.base_pos));
        let names = losers.iter().take(5)
            .map(|c| format!("{}(base_pos {})", c.id, c.gate.base_pos))
            .collect::<Vec<_>>().join(", ");
        println!("  T={t:.2}: {} corpora  [largest: {names}]", losers.len());
    }
}
