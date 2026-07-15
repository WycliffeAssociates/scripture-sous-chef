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
//!   # mark attachment-signatures spike (one corpus or fleet):
//!   cargo run --release -p ssc-core --example calibrate -- --signatures corpora/vref
//!   # pooled class-conditioned spacing spike (Design A vs B; one corpus or fleet):
//!   cargo run --release -p ssc-core --example calibrate -- --pooled-spacing corpora/vref
//!   # fleet survey → self-contained HTML report (all rules, floors zeroed,
//!   # every corpus in the directory; out defaults to target/fleet-report.html):
//!   cargo run --release -p ssc-core --example calibrate -- --fleet corpora/vref [out.html]
//!   # incremental oracle with the cross-call analysis cache enabled:
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       --dump-incremental-cached corpora/vref /tmp/incremental.tsv default
//!   # fast inner-loop oracle: WA subset only (~251 corpora, ~6x quicker) —
//!   # trailing `wa` scopes any dump command; omit (or `full`) for the whole
//!   # fleet. A `wa` dump only diffs against another `wa` dump.
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       --dump-findings corpora/vref /tmp/findings.wa.tsv default wa

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use ssc_core::charclass::class_of;
use ssc_core::config::{
    BracketBalanceConfig, CasingConfig, MixedScriptConfig, ProportionalityConfig,
    PunctOnlyTokenConfig, PunctuationAdjacencyConfig, PunctuationSpacingConfig,
    RepeatedCharacterRunConfig,
};
use ssc_core::rule::{ProjectRule, StatefulRule};
use ssc_core::signals::bracket_balance::BracketBalance;
use ssc_core::signals::casing::{PosClass, SiteEval, evaluate};
use ssc_core::signals::lexical::{PunctOnlyToken, RepeatedCharacterRun};
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::signals::punctuation::{PunctuationAdjacencyAnomaly, PunctuationSpacingAnomaly};
use ssc_core::token::tokenize;
use ssc_core::{
    AnalysisCache, BracketMeasure, Config, Corpus, Finding, FindingArgs, LengthRatioScope, RuleId,
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
            let corpus = load_corpus(Path::new(t));
            let map: BTreeMap<String, String> = corpus
                .keys()
                .iter()
                .cloned()
                .zip(corpus.texts().iter().cloned())
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
        // Punctuation spacing knee/floor sweep + regression (ADR 0054 amend.):
        // over the vref fleet, the total `punct.spacing-anomaly` finding count
        // for a grid of (minority_recurrence_k, minority_rate_per_10k) at floor
        // 0.5, plus the six ADR 0050 calibration corpora at each cell — the
        // before/after regression counter and the ADR 0054 amendment knee-sweep
        // evidence, driven by the production rule under the per-side (left/right
        // attached-vs-spaced) denominators.
        [flag, dir] if flag == "--spacing-sweep" => {
            spacing_fleet_sweep(Path::new(dir));
            return;
        }
        // Pooled class-conditioned spacing SPIKE (plan rule 2 amendment,
        // 2026-07-10). Two designs head-to-head over the same sites at the
        // shipped ADR 0050/0054 reference constants: Design A conditions the
        // per-side attached-vs-spaced binary on the first non-whitespace
        // neighbour's class {Letter, Number, Punct} (crossing verse seams for
        // the class, seam ⇒ spaced), with a two-level hierarchy (class pool →
        // top-level fallback) and a quote/non-quote sub-split inside Punct
        // reported as data; Design B reads each side's IMMEDIATE context as a
        // four-way category {letter, number, ws, punct} (whitespace terminal),
        // scoring mode-dominance × recurrence on the observed category. A file
        // prints a per-corpus report; a vref directory runs the fleet sweep.
        [flag, path] if flag == "--pooled-spacing" => {
            let p = Path::new(path);
            if p.is_dir() {
                pooled_fleet(p);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                pooled_single_report(&analyze_pooled(id, &load_corpus(p)));
            }
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
        // Mark attachment-signatures SPIKE (plan rule 2, steps 1–2). For every
        // separator mark (GC Po minus quotes), the joint (left, right) context
        // signature over {letter, space, punct, digit, edge}; scored corpus-
        // relative as dominance-of-complement × minority recurrence. A file
        // prints a per-corpus report; a vref directory runs the fleet sweep.
        [flag, path] if flag == "--signatures" => {
            let p = Path::new(path);
            if p.is_dir() {
                signature_fleet(p);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                signature_single_report(&analyze_signatures(id, &load_corpus(p)));
            }
            return;
        }
        // Mixed-case word SPIKE (plan rule 3, measurement only). Per case-folded
        // letter-run word, the profile of observed case shapes {lower, Title,
        // ALLCAPS, other-mixed}; an `other-mixed` (`wOrd`) occurrence is scored
        // by the within-word route (dominance × rarity) and, for hapax words, by
        // a corpus-level fallback. A file prints a per-corpus report; a vref
        // directory runs the fleet sweep.
        [flag, path] if flag == "--mixedcase" => {
            let p = Path::new(path);
            if p.is_dir() {
                mixedcase_fleet(p);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                mixedcase_single_report(&analyze_mixedcase(id, &load_corpus(p)));
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
        // Behavior oracle (event-stream port): deterministic, sorted,
        // line-per-finding dump of `analyze_with_config` over a corpus file or
        // a whole vref directory, under either the v1 defaults or the
        // everything-on config. Byte-identical dumps across the port are the
        // acceptance gate. Source (proportionality reference) is WA-en-ulb
        // when present in the directory, else none. An optional trailing
        // `wa`|`full` token scopes the fleet: `wa` runs the ~251-corpus WA
        // subset (the fast inner-loop oracle), omitted/`full` the whole fleet
        // (the before/after gate). A `wa` dump only ever diffs against a `wa`
        // dump — never a `full` one.
        [flag, path, out, cfg_name, rest @ ..] if flag == "--dump-findings" => {
            dump_findings(
                Path::new(path),
                Path::new(out),
                cfg_name,
                OracleScope::parse(rest),
            );
            return;
        }
        // Incremental oracle: for each corpus, mutate one verse, then run the
        // complete-snapshot call (whole corpus + prior + changed=[book]) and
        // dump its findings + a stats digest. Pins the prior/merge/changed
        // path across the port. Trailing `wa`|`full` scopes the fleet as above.
        [flag, path, out, cfg_name, rest @ ..] if flag == "--dump-incremental" => {
            dump_incremental(
                Path::new(path),
                Path::new(out),
                cfg_name,
                false,
                OracleScope::parse(rest),
            );
            return;
        }
        // Same incremental oracle with the cross-call cache enabled. The
        // output must remain byte-identical to --dump-incremental (same scope).
        [flag, path, out, cfg_name, rest @ ..] if flag == "--dump-incremental-cached" => {
            dump_incremental(
                Path::new(path),
                Path::new(out),
                cfg_name,
                true,
                OracleScope::parse(rest),
            );
            return;
        }
        // Wall-clock probe: min-of-5 analyze_with_config on one corpus under
        // both configs (build serial or --features parallel to compare).
        [flag, t] if flag == "--time" => {
            time_configs(Path::new(t));
            return;
        }
        // Census (absolute mode): one corpus prints the section tables; a
        // vref directory runs the fleet dry-run (volumes per section, wire
        // sizes, timing vs an analyze pass). A sanity check, not a
        // calibration — the census has no knobs.
        [flag, path] if flag == "--census" => {
            let p = Path::new(path);
            if p.is_dir() {
                census_fleet(p);
            } else {
                census_single(p);
            }
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
    let books = ssc_core::corpus::by_book(&target);
    let findings = rule.judge(
        &rule.reduce(&books, Some(&source), None).0,
        &books,
        None,
        None,
    );
    eprintln!("proportionality check: {:?}", t0.elapsed());

    let mut per_book: BTreeMap<String, usize> = BTreeMap::new();
    for f in &findings {
        let book = ssc_core::key::parse_key(target.key(f.key_idx))
            .unwrap()
            .book
            .to_string();
        *per_book.entry(book).or_default() += 1;
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

fn print_findings<'a>(target: &Corpus, findings: impl Iterator<Item = &'a ssc_core::Finding>) {
    for f in findings {
        let Some(FindingArgs::LengthRatio { ratio_pct, scope }) = f.args else {
            continue;
        };
        let robust_z = scope_z(&scope);
        let text = target.text(f.key_idx);
        let preview: String = text.chars().take(60).collect();
        println!(
            "  {:<10} z={:+7.1} ratio={:6.0}% | {}",
            target.key(f.key_idx),
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
        let mut per_book: BTreeMap<String, usize> = BTreeMap::new();
        for f in fs {
            let book = ssc_core::key::parse_key(target.key(f.key_idx))
                .unwrap()
                .book
                .to_string();
            *per_book.entry(book).or_default() += 1;
        }
        let (worst_book, worst) = per_book
            .iter()
            .max_by_key(|&(_, n)| *n)
            .map(|(b, n)| (b.clone(), *n))
            .unwrap();
        println!("{rule}: {} (worst book {worst_book}: {worst})", fs.len());
        for f in fs.iter().take(5) {
            let key = target.key(f.key_idx);
            let text = target.text(f.key_idx);
            let slice: String = f.range.slice(text).chars().take(40).collect();
            let ctx_start = text[..f.range.start as usize]
                .char_indices()
                .rev()
                .nth(19)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let ctx: String = text[ctx_start..].chars().take(60).collect();
            println!("    {:<10} [{slice}] …{ctx}", key);
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
            let chars = map.texts().iter().map(|t| t.chars().count()).sum();
            let findings = if verses == 0 {
                Vec::new()
            } else {
                analyze_with_config(&map, None, &cfg)
            };

            let mut surfaced = vec![0u32; n_rules];
            let mut sites = vec![0u32; n_rules];
            let mut hists: Vec<Vec<u64>> = edges.iter().map(|e| vec![0u64; e.len() - 1]).collect();
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
                let better_than =
                    |x: &FleetSample| f.score.unwrap_or(0.0) > x.score.unwrap_or(f32::INFINITY);
                if sv.len() < 2 || sv.iter().any(better_than) {
                    let text = map.text(f.key_idx);
                    let sample = FleetSample {
                        corpus: id.clone(),
                        sid: map.key(f.key_idx).to_string(),
                        score: f.score,
                        slice: display_slice(f.range.slice(text), 24),
                        ctx: fleet_context(text, f.range.start as usize),
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
            FleetRow {
                id,
                verses,
                chars,
                surfaced,
                sites,
                hists,
                samples,
            }
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

    let raw: usize = target
        .texts()
        .iter()
        .map(|t| t.matches('\u{200B}').count())
        .sum();

    let f = analyze(&target, None);
    // Deterministic hygiene must still flag zero U+200B (checked by slicing the
    // char, not just the rule id — hyg.zero-width-misuse still owns BOM/bidi/WJ).
    let hyg_zwsp = f
        .iter()
        .filter(|f| f.code == RuleId::ZeroWidthMisuse)
        .filter(|f| {
            target
                .text(f.key_idx)
                .get(f.range.start as usize..f.range.end as usize)
                == Some("\u{200B}")
        })
        .count();
    let redundant: Vec<_> = f
        .iter()
        .filter(|f| f.code == RuleId::RedundantZeroWidthSpace)
        .collect();
    println!(
        "U+200B raw={raw}  redundant runs flagged={}  (hyg U+200B flags: {hyg_zwsp}, must be 0)",
        redundant.len()
    );
    for fd in redundant.iter().take(10) {
        let t = target.text(fd.key_idx);
        let n = t
            .get(fd.range.start as usize..fd.range.end as usize)
            .unwrap_or("")
            .matches('\u{200B}')
            .count();
        println!("  {}  run of {n} U+200B", target.key(fd.key_idx));
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

    for text in target.texts() {
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
    let books = ssc_core::corpus::by_book(&target);
    let repeat = rule.judge(&rule.reduce(&books, None, None).0, &books, None, None);
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
        let text = target.text(f.key_idx);
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
            target.key(f.key_idx),
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
    let mut per_verse: Vec<(&str, &str, Vec<ssc_core::Span>)> = Vec::new();
    for (key, text) in target.keys().iter().zip(target.texts()) {
        let spans = scan_punct_only_token(text);
        if spans.is_empty() {
            continue;
        }
        for s in &spans {
            *pattern_count.entry(s.slice(text).to_string()).or_default() += 1;
        }
        per_verse.push((key.as_str(), text.as_str(), spans));
    }
    let total: usize = pattern_count.values().sum();

    // Pass 2: emit per-finding rows.
    println!("corpus\tsid\tchunk\tchunk_count\ttotal_findings\tverses\tcontext");
    for (key, text, spans) in &per_verse {
        for s in spans {
            let chunk = s.slice(text);
            let ctx_start = text[..s.start as usize]
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
                "{corpus}\t{key}\t{chunk}\t{}\t{total}\t{}\t{ctx}",
                pattern_count[chunk],
                target.len(),
            );
        }
    }
    let mut top: Vec<_> = pattern_count.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    let head: Vec<String> = top
        .iter()
        .take(6)
        .map(|(p, n)| format!("[{p}]x{n}"))
        .collect();
    eprintln!(
        "{corpus}: {} verses, {total} candidates, {} distinct patterns | {}",
        target.len(),
        pattern_count.len(),
        head.join(" ")
    );

    // Production score distribution at floor 0, and the shipped-floor count.
    let rule = PunctOnlyToken {
        cfg: PunctOnlyTokenConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let books = ssc_core::corpus::by_book(&target);
    let findings = rule.judge(&rule.reduce(&books, None, None).0, &books, None, None);
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
        cfg: BracketBalanceConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let books = ssc_core::corpus::by_book(&target);
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
    for text in target.texts() {
        for c in text.chars() {
            if !class_of(c).is_punctuation() {
                continue;
            }
            let (family, is_open, open_glyph, close_glyph) =
                if let Some(close) = bracket_close_of(c) {
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
        let text = target.text(f.key_idx);
        let glyph = f.range.slice(text).chars().next().unwrap();
        let family = bracket_close_of(glyph)
            .map(|_| glyph)
            .or_else(|| bracket_open_of(glyph))
            .unwrap_or(glyph);
        match &f.args {
            Some(FindingArgs::BracketWindow {
                measure: BracketMeasure::Pairing,
                ..
            }) => {
                *orphans.entry(family).or_default() += 1;
            }
            Some(FindingArgs::BracketWindow {
                measure: BracketMeasure::ShortSpan,
                ..
            }) => {
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
            f.open, f.close, f.open as u32, events, f.opens, f.closes, orph, long, rate
        );
    }

    // ~20 sample orphan findings with their DelimObservation inventories, so
    // the family collisions (quote-role glyphs vs real brackets) are eyeballable.
    println!("\nsample findings (up to 20) with delimiter inventories:");
    let mut samples: Vec<&Finding> = findings.iter().collect();
    samples.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap()
    });
    for f in samples.iter().take(20) {
        let text = target.text(f.key_idx);
        let glyph = f.range.slice(text);
        let (measure, window, majority, total) = match &f.args {
            Some(FindingArgs::BracketWindow {
                measure,
                window,
                majority,
                total,
            }) => (*measure, window, *majority, *total),
            _ => continue,
        };
        let kind = match measure {
            BracketMeasure::Pairing => "orphan",
            BracketMeasure::ShortSpan => "long-span",
        };
        println!(
            "  {:<10} score={:.3} {kind} [{glyph}] {majority}/{total}",
            target.key(f.key_idx),
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
        cfg: PunctuationAdjacencyConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let t0 = std::time::Instant::now();
    let books = ssc_core::corpus::by_book(&target);
    let findings = rule.judge(&rule.reduce(&books, None, None).0, &books, None, None);
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

/// Punctuation-spacing knee/floor sweep + regression over the vref fleet
/// (ADR 0054). Drives the **production** `punct.spacing-anomaly` rule (the new
/// 16-cell signature model) under a grid of `(minority_recurrence_k,
/// minority_rate_per_10k)` at floor 0.5, reporting the fleet-wide finding total
/// and the six ADR 0050 calibration corpora at each cell. This is the
/// before/after regression counter and the ADR 0054 knee-sweep evidence: the
/// shipped `(32, 40)` cell is the one to compare against the old binary rule's
/// 3,928 fleet findings, and the six named corpora must keep their kept-sites.
fn spacing_fleet_sweep(dir: &Path) {
    use rayon::prelude::*;

    // ADR 0050 regression corpora (short id → file stem).
    const REG: &[&str] = &[
        "engwebster",
        "WA-kmr-IQ-badini-reg",
        "udu",
        "WA-ne-udb",
        "WA-pa-ulb",
        "mya",
    ];
    // (k, rate) grid; floor fixed at the shipped 0.5. The shipped cell is (32,40).
    const KS: &[f32] = &[16.0, 32.0, 64.0];
    const RATES: &[f32] = &[0.0, 20.0, 40.0, 80.0];

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total_corpora = files.len();
    eprintln!("spacing sweep fleet: {total_corpora} corpora");

    let count = |k: f32, rate: f32, map: &Corpus| -> usize {
        let rule = PunctuationSpacingAnomaly {
            cfg: PunctuationSpacingConfig {
                emit_score_min: 0.5,
                confidence_z: 1.96,
                minority_recurrence_k: k,
                minority_rate_per_10k: rate,
            },
        };
        let books = ssc_core::corpus::by_book(map);
        rule.judge(&rule.reduce(&books, None, None).0, &books, None, None)
            .len()
    };

    // Per-corpus: for each (k, rate) cell, the finding count. Reduce fleet in
    // parallel, summing per cell.
    let per_corpus: Vec<(String, Vec<usize>)> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let mut cells = Vec::new();
            for &k in KS {
                for &rate in RATES {
                    cells.push(count(k, rate, &map));
                }
            }
            (id, cells)
        })
        .collect();

    let ncells = KS.len() * RATES.len();
    let mut totals = vec![0usize; ncells];
    let mut corpora_with = vec![0usize; ncells];
    for (_, cells) in &per_corpus {
        for (i, &n) in cells.iter().enumerate() {
            totals[i] += n;
            if n > 0 {
                corpora_with[i] += 1;
            }
        }
    }

    println!("=== punct.spacing-anomaly fleet knee/floor sweep (floor 0.5, z 1.96) ===");
    println!(
        "production per-side (left/right) rule; cells = total fleet findings (corpora with ≥1)"
    );
    print!("      {:>6}", "k\\rate");
    for &rate in RATES {
        print!("  {:>14}", format!("{rate:.0}/10k"));
    }
    println!();
    for (ki, &k) in KS.iter().enumerate() {
        print!("      {k:>6.0}");
        for ri in 0..RATES.len() {
            let i = ki * RATES.len() + ri;
            print!("  {:>14}", format!("{} ({})", totals[i], corpora_with[i]));
        }
        println!();
    }

    println!("\n-- six ADR 0050 regression corpora, findings per (k, rate) cell --");
    print!("  {:<24}", "corpus");
    for &k in KS {
        for &rate in RATES {
            print!("  {:>8}", format!("{k:.0}/{rate:.0}"));
        }
    }
    println!();
    for &id in REG {
        if let Some((_, cells)) = per_corpus.iter().find(|(cid, _)| cid == id) {
            print!("  {id:<24}");
            for &n in cells {
                print!("  {n:>8}");
            }
            println!();
        } else {
            println!("  {id:<24}  (absent)");
        }
    }
    let shipped_idx = KS.iter().position(|&k| k == 32.0).unwrap() * RATES.len()
        + RATES.iter().position(|&r| r == 40.0).unwrap();
    println!(
        "\nshipped cell (k=32, rate=40/10k, floor 0.5): {} fleet findings across {} corpora",
        totals[shipped_idx], corpora_with[shipped_idx]
    );
}

/// Shared score-distribution report for the corpus-relative rules: total
/// scored sites, how many clear a ladder of floors, and the top/bottom samples
/// with their exact slice and a little context.
fn report_scored(name: &str, target: &Corpus, findings: &[Finding]) {
    println!("\n{name}: {} scored sites (floor 0)", findings.len());
    for floor in [0.5_f32, 0.7, 0.9, 0.99] {
        let n = findings
            .iter()
            .filter(|f| f.score.unwrap_or(0.0) >= floor)
            .count();
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
    by_score.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap()
    });
    println!("  top 10 by score:");
    print_scored(target, by_score.iter().take(10).copied());
    println!("  bottom 5 by score:");
    print_scored(target, by_score.iter().rev().take(5).copied());
}

fn print_scored<'a>(target: &Corpus, findings: impl Iterator<Item = &'a Finding>) {
    for f in findings {
        let text = target.text(f.key_idx);
        let slice: String = f.range.slice(text).chars().take(16).collect();
        let ctx_start = text[..f.range.start as usize]
            .char_indices()
            .rev()
            .nth(14)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let ctx: String = text[ctx_start..].chars().take(44).collect();
        println!(
            "    {:<10} score={:.3} [{}] …{}",
            target.key(f.key_idx),
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
    let intr = s
        .intrinsic
        .map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
    let pos = s
        .positional
        .map_or(0.0, |f| f.dominance * rarity_abs(f.minority, k));
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
fn analyze_casing(id: String, map: &Corpus) -> CasingCorpus {
    let books = ssc_core::corpus::by_book(map);
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
        let text = map.text(s.key_idx);
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
                    sid: map.key(s.key_idx).to_string(),
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
            && ANCHORS
                .iter()
                .any(|&(ac, asid, aw)| ac == id && asid == map.key(s.key_idx) && aw == word)
        {
            anchors.push(AnchorHit {
                corpus: id.clone(),
                sid: map.key(s.key_idx).to_string(),
                word,
                quad,
                intr: s
                    .intrinsic
                    .map(|f| (f.dominance, f.minority, f.opportunities)),
                pos: s
                    .positional
                    .map(|f| (f.dominance, f.minority, f.opportunities)),
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
            s.glyph
                .map(|c| format!("{c:?}"))
                .unwrap_or_else(|| "^".to_string()),
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
                analyze_casing(id, &Corpus::try_from_parts(Vec::new(), Vec::new()).unwrap())
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

    println!(
        "=== CASING TWO-FACTOR (ADR 0051) — fleet aggregate ({} corpora) ===",
        corpora.len()
    );
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
        (
            total,
            affected,
            if total > 0 {
                top5 as f64 / total as f64
            } else {
                0.0
            },
        )
    };
    println!(
        "\n-- packet 1: per-channel surfaced volume | total (affected corpora; top-5 share) --"
    );
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
        match all_anchors
            .iter()
            .find(|h| h.corpus == ac && h.sid == asid && h.word == aw)
        {
            Some(h) => {
                let (s8, s16, s32) = (h.score(8.0), h.score(16.0), h.score(32.0));
                let alive: Vec<String> = PACKET_FLOORS
                    .iter()
                    .filter(|&&fl| s32 >= fl)
                    .map(|fl| format!("{fl:.2}"))
                    .collect();
                let ifac = h
                    .intr
                    .map(|(d, m, o)| format!("i(d{d:.3} m{m} o{o})"))
                    .unwrap_or_default();
                let pfac = h
                    .pos
                    .map(|(d, m, o)| format!("p(d{d:.3} m{m} o{o})"))
                    .unwrap_or_default();
                println!(
                    "  {ac:<11} {asid:<9} {aw:<11} {:<11} {ifac} {pfac}  s@8={s8:.3} @16={s16:.3} @32={s32:.3}  alive≥[{}]",
                    h.quad,
                    if alive.is_empty() {
                        "dead@0.80+".to_string()
                    } else {
                        alive.join(",")
                    },
                );
            }
            None => println!(
                "  {ac:<11} {asid:<9} {aw:<11} — not captured (not a lowercase anomaly candidate)"
            ),
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
        "eng-web",
        "eng-kjv",
        "engwebster",
        "WA-en-ulb",
        "spaRV1909",
        "WA-es-419-ulb",
        "fraLSG",
        "WA-fr-ulb",
        "porblt",
        "ita1885",
        "ron1924",
        "deu1912",
        "swhulb",
        "WA-sw-ulb",
        "ind",
        "nld",
        "vie1934",
        "tglulb",
    ];
    println!("\n-- surfaced samples from major-language corpora (ref knee) --");
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples.is_empty() {
            continue;
        }
        let mut s: Vec<&CasingSample> = c.samples.iter().collect();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        println!(
            "  [{}] surfaced {} (i {}, p {}, b {}):",
            c.id,
            c.ref_intrinsic + c.ref_positional + c.ref_both,
            c.ref_intrinsic,
            c.ref_positional,
            c.ref_both
        );
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
    let rule = SentenceInitialLowercase {
        cfg: CasingConfig::default(),
    };
    let mut rows: Vec<(String, usize)> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let books = ssc_core::corpus::by_book(&map);
            let (stats, _) = rule.reduce(&books, None, None);
            let bytes = serde_json::to_string(&stats).map(|s| s.len()).unwrap_or(0);
            (id, bytes)
        })
        .collect();
    rows.sort_by_key(|r| r.1);
    let n = rows.len();
    let pct = |q: f64| rows[((n - 1) as f64 * q) as usize].1;
    println!("casing CasingStats JSON size over {n} corpora:");
    println!(
        "  p50 {} B  p90 {} B  max {} B",
        pct(0.5),
        pct(0.9),
        pct(1.0)
    );
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
const CLOSURE_SCALAR_SHARES: [f64; 8] = [0.00001, 0.0001, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02];
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

/// Round-5 titlecase-shape + forced-position facts for one letter-token
/// occurrence. `titlecase` is the name-shape test — uppercase first letter AND
/// at least one following lowercase letter (round 4 used bare capital-initial,
/// which leaked on lone capitals `Q`/`I` and all-caps common words like `YÖ`);
/// `forced` is the casing machinery's forced-position test (book-initial, or a
/// word that consumed a bare attached terminal — verse-initial is NOT forced,
/// per `CLAUDE.md`). Only consulted for hapax words, so recording each word's
/// latest occurrence is sufficient.
#[derive(Clone, Copy)]
struct WordShape {
    titlecase: bool,
    forced: bool,
}

/// Advance the pending-terminal machine over a gap (all scalars between two
/// letter tokens), mirroring `casing::advance_gap`. The pending state is
/// `None` = no terminal seen; `Some(false)` = a bare/quoted terminal is
/// pending; `Some(true)` = a non-quote intervening punctuation collapsed the
/// boundary to mid-flow (`...`).
fn glyph_advance_gap(gap: &str, pending: &mut Option<bool>, prev_letter: &mut bool) {
    for c in gap.chars() {
        let cl = class_of(c);
        if cl.is_whitespace() || cl.is_numeric() {
            *prev_letter = false;
        } else if cl.is_alphabetic() {
            *prev_letter = true;
        } else {
            match pending {
                Some(collapsed) if !cl.is_quote() => *collapsed = true,
                Some(_) => {}
                None if *prev_letter => *pending = Some(false),
                None => {}
            }
            *prev_letter = false;
        }
    }
}

/// Walk each book in canonical order, mirroring `casing::walk_book`'s pending-
/// terminal machine (carried across verse seams, reset per book; the book's
/// first word is forced), and record each letter token's capital-initial +
/// forced facts. Keyed by the same lowercase letter-token key the lexical
/// machinery uses, so the round-4 proper-noun test can look up a rare glyph's
/// hapax container. Only pure-letter tokens are recorded, matching the tokens
/// that feed `letter_words`/`glyph_words` (a hyphen-joined name is two ordinary
/// letter tokens in both, never one compound span).
fn letter_word_shapes(map: &Corpus) -> BTreeMap<String, WordShape> {
    let mut shapes: BTreeMap<String, WordShape> = BTreeMap::new();
    for group in &ssc_core::corpus::by_book(map) {
        let mut pending: Option<bool> = None;
        let mut book_initial = true;
        for text in group.texts {
            let mut prev_letter = false;
            let mut cursor = 0usize;
            for token in tokenize(text) {
                let word = token.span.slice(text);
                if !is_letter_token(word) {
                    // Not a word for the casing walk; its text stays in the gap
                    // the next letter token sees (cursor deliberately unmoved).
                    continue;
                }
                glyph_advance_gap(
                    &text[cursor..token.span.start as usize],
                    &mut pending,
                    &mut prev_letter,
                );
                let mut word_chars = word.chars();
                let first = word_chars.next().unwrap();
                // Titlecase shape: uppercase first letter AND >=1 following
                // lowercase letter. Spares genuine names (Quirinius, Roma) while
                // returning lone capitals and all-caps tokens to retained.
                let titlecase = class_of(first).is_uppercase()
                    && word_chars.any(|c| class_of(c).is_lowercase());
                let forced = book_initial || matches!(pending.take(), Some(false));
                book_initial = false;
                shapes.insert(word.to_lowercase(), WordShape { titlecase, forced });
                prev_letter = word
                    .chars()
                    .next_back()
                    .is_some_and(|c| class_of(c).is_alphabetic());
                cursor = token.span.end as usize;
            }
            glyph_advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
        }
    }
    shapes
}

fn letter_round2(
    inventory: &BTreeMap<char, u64>,
    word_tokens: BTreeMap<String, u64>,
    glyph_words: BTreeMap<char, BTreeMap<String, u64>>,
    shapes: &BTreeMap<String, WordShape>,
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
        if glyph_lane(glyph) != Some(GlyphLane::Letter)
            || count > *LETTER_RARE_MAX_COUNTS.last().unwrap()
        {
            continue;
        }
        let Some(words) = glyph_words.get(&glyph) else {
            rare.push(LetterRare {
                glyph,
                count,
                lexical_word: None,
                lexical_word_tokens: 0,
                proper_noun_shape: false,
            });
            continue;
        };
        let accounted: u64 = words.values().sum();
        let dominant = words.iter().max_by_key(|(_, occurrences)| **occurrences);
        let (lexical_word, lexical_word_tokens) = match dominant {
            Some((word, &occurrences))
                if accounted == count
                    && occurrences == count
                    && word_tokens.get(word).copied().unwrap_or(0) >= 2 =>
            {
                (Some(word.clone()), word_tokens[word])
            }
            _ => (None, 0),
        };
        // Round-5 proper-noun-shape discount: only where the recurring-word
        // lexical discount did NOT already fire. It applies when the glyph's
        // sole containing word type is a hapax (occurs once) AND that lone
        // occurrence is titlecase-shaped (upper first + >=1 following lower) AND
        // at a non-forced (mid-flow) position. A capital at a forced position is
        // capitalised for position reasons — shape says nothing — so no discount
        // there (the flag survives). The titlecase test (round 5, was bare
        // capital-initial) returns lone capitals and all-caps tokens to
        // retained. Bicameral-only by construction: `titlecase` is false for
        // caseless scripts, so the branch never fires for them.
        let proper_noun_shape = lexical_word.is_none()
            && words.len() == 1
            && accounted == count
            && words.values().next().is_some_and(|&occ| occ == count)
            && words
                .keys()
                .next()
                .and_then(|word| {
                    (word_tokens.get(word).copied().unwrap_or(0) == 1)
                        .then(|| shapes.get(word))
                        .flatten()
                })
                .is_some_and(|shape| shape.titlecase && !shape.forced);
        rare.push(LetterRare {
            glyph,
            count,
            lexical_word,
            lexical_word_tokens,
            proper_noun_shape,
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
    /// Round-5: the glyph's sole container is a titlecase-shaped hapax word at a
    /// non-forced position, so its capital is shape (a name), not position.
    proper_noun_shape: bool,
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
    proper_samples: Vec<GlyphSample>,
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
    proper_samples: Vec<GlyphSample>,
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
            proper_samples: corpus.proper_samples,
        }
    }
}

fn glyph_candidates(
    inventory: &BTreeMap<char, u64>,
    lane_totals: &[u64; 4],
) -> Vec<GlyphCandidate> {
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

fn glyph_sweep(
    candidates: &[GlyphCandidate],
    score: impl Fn(GlyphCandidate) -> f64,
) -> [GlyphSweep; 4] {
    candidates
        .iter()
        .copied()
        .fold([GlyphSweep::default(); 4], |mut out, candidate| {
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
    let before = text[..start]
        .char_indices()
        .rev()
        .nth(22)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let after = text[end..]
        .char_indices()
        .nth(22)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[before..after].replace(['\t', '\n'], " ")
}

/// Pick one source occurrence for the strongest rare candidates. The samples
/// are review leads, not stored rule sites: a production rule will forward or
/// re-scan its own spans under the stateful protocol.
fn glyph_samples(id: &str, map: &Corpus, candidates: &[GlyphCandidate]) -> Vec<GlyphSample> {
    let mut ranked: Vec<GlyphCandidate> = candidates
        .iter()
        .copied()
        .filter(|c| glyph_rarity_abs(c.count, 32.0) >= GLYPH_SWEEP_FLOOR)
        .collect();
    ranked.sort_by_key(|c| (std::cmp::Reverse(c.lane_total), c.count, c.glyph));

    let mut wanted = BTreeMap::new();
    for lane in GlyphLane::ALL {
        for candidate in ranked
            .iter()
            .copied()
            .filter(|candidate| candidate.lane == lane)
            .take(6)
        {
            wanted.insert(candidate.glyph, candidate);
        }
    }
    let mut samples = Vec::new();
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        for (start, glyph) in text.char_indices() {
            let Some(candidate) = wanted.remove(&glyph) else {
                continue;
            };
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

/// Review leads for rare letter glyphs (count ≤ knee) that survive the lexical-
/// concentration discount, split into two adjudication sets so a human can judge
/// signal quality on the set the rule would keep in a closed-alphabet corpus:
/// `(proper_killed, retained)`. `proper_killed` is what the round-4 proper-noun-
/// shape discount removes (expect Quirinius-class names); `retained` is what
/// survives all four factors (expect script-intrusion typos). Whether the corpus
/// itself clears closure is decided at fleet time.
fn glyph_retained_samples(
    id: &str,
    map: &Corpus,
    round2: &LetterRound2,
) -> (Vec<GlyphSample>, Vec<GlyphSample>) {
    // glyph -> (count, is_proper_killed)
    let mut wanted: BTreeMap<char, (u64, bool)> = BTreeMap::new();
    for candidate in round2
        .rare
        .iter()
        .filter(|c| c.count <= RETAINED_SAMPLE_KNEE && c.lexical_word.is_none())
    {
        wanted.insert(
            candidate.glyph,
            (candidate.count, candidate.proper_noun_shape),
        );
    }
    let (mut proper, mut retained) = (Vec::new(), Vec::new());
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        if wanted.is_empty() {
            break;
        }
        for (start, glyph) in text.char_indices() {
            let Some((count, is_proper)) = wanted.remove(&glyph) else {
                continue;
            };
            let sample = GlyphSample {
                corpus: id.to_string(),
                sid: sid.to_string(),
                glyph,
                lane: GlyphLane::Letter,
                count,
                lane_total: round2.letter_scalars,
                context: glyph_context(text, start, start + glyph.len_utf8()),
            };
            if is_proper {
                proper.push(sample);
            } else {
                retained.push(sample);
            }
        }
    }
    proper.sort_by_key(|sample| (sample.count, sample.glyph));
    retained.sort_by_key(|sample| (sample.count, sample.glyph));
    (proper, retained)
}

fn analyze_glyphs(id: String, map: &Corpus) -> GlyphCorpus {
    let mut inventory: BTreeMap<char, u64> = BTreeMap::new();
    let mut lane_totals = [0u64; 4];
    let mut decomposed_pairs: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_words: BTreeMap<String, u64> = BTreeMap::new();
    let mut letter_glyph_words: BTreeMap<char, BTreeMap<String, u64>> = BTreeMap::new();
    let mut scalar_count = 0u64;

    for text in map.texts() {
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
                *decomposed_pairs
                    .entry(format!("{base}{glyph}"))
                    .or_default() += 1;
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
            for glyph in word
                .chars()
                .filter(|&glyph| glyph_lane(glyph) == Some(GlyphLane::Letter))
            {
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
        .map(|&rate| {
            glyph_sweep(&candidates, |c| {
                glyph_rarity_rate(c.count, c.lane_total, rate)
            })
        })
        .collect();
    let samples = glyph_samples(&id, map, &candidates);
    let shapes = letter_word_shapes(map);
    let letter_round2 = letter_round2(&inventory, letter_words, letter_glyph_words, &shapes);
    let (proper_samples, retained_samples) = glyph_retained_samples(&id, map, &letter_round2);

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
        proper_samples,
    }
}

fn glyph_label(glyph: char) -> String {
    format!("{} U+{:04X}", glyph.escape_default(), glyph as u32)
}

fn print_glyph_sweeps(abs: &[[GlyphSweep; 4]], rate: &[[GlyphSweep; 4]]) {
    println!(
        "\nrecurrence sweeps (rows surface raw rarity >= {GLYPH_SWEEP_FLOOR:.2}; types / sites):"
    );
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
    proper_killed: GlyphSweep,
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
    add_glyph_sweep(&mut total.proper_killed, add.proper_killed);
    add_glyph_sweep(&mut total.retained, add.retained);
}

fn letter_round2_tally(
    round2: &LetterRound2,
    max_count: u64,
    closed_alphabet: bool,
) -> LetterRound2Tally {
    let mut out = LetterRound2Tally::default();
    for candidate in round2
        .rare
        .iter()
        .filter(|candidate| candidate.count <= max_count)
    {
        let candidate_sweep = GlyphSweep {
            types: 1,
            sites: candidate.count,
        };
        add_glyph_sweep(&mut out.base, candidate_sweep);
        if !closed_alphabet {
            add_glyph_sweep(&mut out.closure_killed, candidate_sweep);
        } else if candidate.lexical_word.is_some() {
            add_glyph_sweep(&mut out.lexical_killed, candidate_sweep);
        } else if candidate.proper_noun_shape {
            add_glyph_sweep(&mut out.proper_killed, candidate_sweep);
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
            "    <= {max_count}: base {}/{}  lexical-discount {}/{} ({:.1}%)  proper-noun {}/{} ({:.1}%)  retained {}/{}",
            tally.base.types,
            tally.base.sites,
            tally.lexical_killed.types,
            tally.lexical_killed.sites,
            kill_rate(tally.lexical_killed.sites, tally.base.sites),
            tally.proper_killed.types,
            tally.proper_killed.sites,
            kill_rate(tally.proper_killed.sites, tally.base.sites),
            tally.retained.types,
            tally.retained.sites,
        );
    }
    let lexical: Vec<_> = round2
        .rare
        .iter()
        .filter(|candidate| candidate.lexical_word.is_some())
        .collect();
    println!(
        "  lexical-concentration discounts (first {} of {}):",
        lexical.len().min(20),
        lexical.len()
    );
    for candidate in lexical.iter().take(20) {
        println!(
            "    {:<15} count={} word={} ({} tokens)",
            glyph_label(candidate.glyph),
            candidate.count,
            candidate.lexical_word.as_deref().unwrap_or_default(),
            candidate.lexical_word_tokens,
        );
    }
    let proper: Vec<_> = round2
        .rare
        .iter()
        .filter(|candidate| candidate.proper_noun_shape)
        .collect();
    println!(
        "  proper-noun-shape discounts (first {} of {}):",
        proper.len().min(20),
        proper.len()
    );
    for candidate in proper.iter().take(20) {
        println!(
            "    {:<15} count={} (titlecase-shaped hapax word at a non-forced position)",
            glyph_label(candidate.glyph),
            candidate.count,
        );
    }
}

fn glyph_single_report(corpus: &GlyphCorpus) {
    println!(
        "=== RARE-GLYPH SPIKE: {} ({} verses) ===",
        corpus.id, corpus.verses
    );
    println!(
        "raw scalar inventory: {} occurrences / {} distinct scalars",
        corpus.scalar_count,
        corpus.inventory.len()
    );
    println!("candidate lane opportunities:");
    for lane in GlyphLane::ALL {
        let types = corpus
            .inventory
            .keys()
            .filter(|&&c| glyph_lane(c) == Some(lane))
            .count();
        println!(
            "  {}  {:>10} occurrences / {:>5} glyph types",
            lane.label(),
            corpus.lane_totals[lane.index()],
            types
        );
    }
    print_glyph_histogram(&corpus.count_hist);
    print_glyph_sweeps(&corpus.abs_sweeps, &corpus.rate_sweeps);
    print_letter_round2_single(&corpus.letter_round2);

    let mut candidates = glyph_candidates(&corpus.inventory, &corpus.lane_totals);
    candidates.sort_by_key(|c| (c.count, std::cmp::Reverse(c.lane_total), c.glyph));
    println!(
        "\nrarest candidate glyphs (first {} of {}):",
        candidates.len().min(120),
        candidates.len()
    );
    println!(
        "  {:<15} {:<5} {:>8} {:>12} {:>14}",
        "glyph", "lane", "count", "lane total", "rate /10k"
    );
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
    let mut round2 = vec![
        vec![LetterRound2Tally::default(); LETTER_RARE_MAX_COUNTS.len()];
        CLOSURE_SCALAR_SHARES.len()
    ];
    let mut open_corpora = vec![0u64; CLOSURE_SCALAR_SHARES.len()];
    // (id, L scalars, hapax L scalars, scalar closure ppm, word-hapax share ppm)
    let mut closure_rows: Vec<(String, u64, u64, u64, u64)> = Vec::new();
    // Round-3 sanity checks: corpora that flip open (closed word-hapax → open
    // scalar closure), retained review leads, and lexical-kill mechanism leads.
    let mut flips: Vec<(String, u64, u64)> = Vec::new();
    let mut retained_samples: Vec<GlyphSample> = Vec::new();
    let mut proper_samples: Vec<GlyphSample> = Vec::new();
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
            for (sum, value) in count_hist[lane.index()]
                .iter_mut()
                .zip(corpus.count_hist[lane.index()])
            {
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
            proper_samples.extend(corpus.proper_samples.iter().cloned());
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
    println!(
        "  closure threshold is hapax L-scalar types / all L-scalar occurrences (letter-SCALAR closure)."
    );
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
                "    <= {max_count}: base {:>6}; closure -{:>6} ({:>5.1}%); lexical -{:>6} ({:>5.1}%); proper-noun -{:>6} ({:>5.1}%); keep {:>6}",
                tally.base.sites,
                tally.closure_killed.sites,
                kill_rate(tally.closure_killed.sites, tally.base.sites),
                tally.lexical_killed.sites,
                kill_rate(tally.lexical_killed.sites, tally.base.sites),
                tally.proper_killed.sites,
                kill_rate(tally.proper_killed.sites, tally.base.sites),
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
            if upper {
                "  [uppercase → folds to repeated lowercase]"
            } else {
                ""
            },
        );
    }

    // Round-5 proper-noun-kill table: ~20 diverse sites the shape discount
    // removes (letter, count<=3, non-lexical, titlecase-shaped hapax at a
    // non-forced position). Expect Quirinius-class names; the round-4 leaks
    // (lone capitals, all-caps common words) should no longer appear here.
    proper_samples.sort_by_key(|s| (s.corpus.clone(), s.count, s.glyph));
    proper_samples.dedup_by(|a, b| a.corpus == b.corpus && a.glyph == b.glyph);
    let mut proper_diverse: Vec<GlyphSample> = Vec::new();
    let mut proper_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for sample in &proper_samples {
        let seen = proper_per_corpus.entry(sample.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            proper_diverse.push(sample.clone());
        }
    }
    println!(
        "\nround-5 proper-noun-kill table ({} of {} proper-shape leads; closure<={:.3}%, knee<={}):",
        proper_diverse.len().min(20),
        proper_samples.len(),
        RETAINED_SAMPLE_THRESHOLD * 100.0,
        RETAINED_SAMPLE_KNEE,
    );
    print_glyph_samples(&proper_diverse.into_iter().take(20).collect::<Vec<_>>());

    // Retained review table: ~30 diverse retained sites (letter, count<=3, not
    // lexical, not proper-noun-shape) in corpora open at the representative
    // closure threshold — what survives all four factors.
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
        "\nretained review table ({} of {} retained leads; closure<={:.3}%, knee<={}, non-lexical, non-proper-noun — survives all four factors):",
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
            (
                std::cmp::Reverse(sample.lane_total),
                sample.count,
                sample.glyph,
            )
        });
        println!("  [{}]", lane.label());
        print_glyph_samples(&lane_samples.into_iter().take(12).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    fn one_verse(text: &str) -> Corpus {
        Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec![text.to_string()]).unwrap()
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

    fn rare(corpus: &GlyphCorpus, glyph: char) -> &LetterRare {
        corpus
            .letter_round2
            .rare
            .iter()
            .find(|c| c.glyph == glyph)
            .unwrap()
    }

    #[test]
    fn proper_noun_shape_discounts_titlecase_hapax_at_midflow() {
        // `Q` occurs once, inside the hapax name `Quirinius`, mid-flow (a common
        // word precedes it, no terminal). Its lone container is titlecase-shaped
        // (upper first + following lower) and at a non-forced position ⇒
        // proper-noun-shape discount fires. The recurring-word lexical discount
        // does not (the container is a hapax).
        let map = one_verse("in the days of Quirinius the governor");
        let corpus = analyze_glyphs("quirinius".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(q.lexical_word.is_none());
        assert!(q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_lone_capital_token() {
        // Round-5 tightening: a lone one-letter uppercase token (`Q` standing
        // alone mid-flow, the round-4 WA-dje MAT 11:4 leak) is capital-initial
        // but NOT titlecase (no following lowercase letter), so the discount no
        // longer fires — the stray capital stays flagged (the safe direction).
        let map = one_verse("he said to them Q go and tell the news");
        let corpus = analyze_glyphs("lone-capital".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(q.lexical_word.is_none());
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_all_caps_token() {
        // Round-5 tightening: an all-caps token carrying a stray glyph (the
        // Spanish `YÖ`-for-`YO` leak, WA-es-419 ZEC 3:4) is capital-initial but
        // has no following lowercase letter, so it is not titlecase and the
        // discount does not fire — the genuine typo stays flagged.
        let map = one_verse("and the voice cried YÖ am the one who speaks");
        let corpus = analyze_glyphs("all-caps".to_string(), &map);
        let o = rare(&corpus, 'Ö');
        assert!(o.lexical_word.is_none());
        assert!(!o.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_capital_at_a_forced_position() {
        // Same name, but now the word after a bare terminal: the capital is
        // position-forced, so shape says nothing and the discount must NOT fire
        // (conservative — the flag survives).
        let map = one_verse("it happened then. Quirinius ruled the land");
        let corpus = analyze_glyphs("forced".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_spares_book_initial_capital() {
        // Book-initial is forced with no terminal glyph (CLAUDE.md), so a rare
        // glyph inside the very first word gets no shape discount.
        let map = one_verse("Quirinius governed the far country");
        let corpus = analyze_glyphs("book-initial".to_string(), &map);
        let q = rare(&corpus, 'Q');
        assert!(!q.proper_noun_shape);
    }

    #[test]
    fn proper_noun_shape_ignores_lowercase_script_intrusion() {
        // A stray `q` intruding into an otherwise-lowercase word is not capital-
        // initial, so the shape branch never fires — script-intrusion typos in
        // ordinary lowercase words stay flagged (bicameral-only by construction
        // also means caseless scripts, which have no uppercase, never qualify).
        let map = one_verse("she walked into the woqden house today");
        let corpus = analyze_glyphs("intrusion".to_string(), &map);
        let q = rare(&corpus, 'q');
        assert!(!q.proper_noun_shape);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Mark attachment-signatures SPIKE (plan rule 2, steps 1–2). Measurement only —
// nothing frozen, production `punctuation.rs` untouched; every symbol here is
// harness-local (`sig_*` / `Ctx` / `Sig*`). It generalises the live
// `punct.spacing-anomaly` before-only binary (spaced/attached) to a joint
// (left, right) context signature over {letter, space, punct, digit},
// scored corpus-relative as `dominance(complement) × rarity(minority)` — the
// ADR 0048/0050 shape one dimension wider. NO `edge` category (2026-07-10
// ruling): verses are addressing only; the model cares solely about grapheme
// adjacency, so the verse/book seam reads as WHITESPACE. A verse-final `.` is
// `letter|space`, pooled with mid-verse `letter|space` (per repo CLAUDE.md a
// terminal is never "attached" across a seam, and the seam asserts nothing
// else).
// ═══════════════════════════════════════════════════════════════════════════

/// A separator mark's neighbour category on one side. Mirrors the live spacing
/// rule's governing-neighbour logic (`spacing_opportunities`): walk over
/// horizontal whitespace, then classify the first non-whitespace grapheme.
/// `Space` = whitespace was crossed (the live `spaced` bit) OR the verse seam
/// reached — the seam is whitespace to this model, never its own category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Ctx {
    Letter,
    Space,
    Punct,
    Digit,
}

impl Ctx {
    const ALL: [Self; 4] = [Self::Letter, Self::Space, Self::Punct, Self::Digit];
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Space => 1,
            Self::Punct => 2,
            Self::Digit => 3,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Space => "space",
            Self::Punct => "punct",
            Self::Digit => "digit",
        }
    }
}

/// Number of joint signatures (4 left × 4 right).
const SIG_CELLS: usize = 16;

/// A signature index (0..SIG_CELLS) packs `(left, right)`.
fn sig_index(left: Ctx, right: Ctx) -> usize {
    left.index() * 4 + right.index()
}
fn sig_ctx(index: usize) -> (Ctx, Ctx) {
    (Ctx::ALL[index / 4], Ctx::ALL[index % 4])
}
fn sig_label(index: usize) -> String {
    let (l, r) = sig_ctx(index);
    format!("{}|{}", l.label(), r.label())
}

const SIG_Z: f64 = 1.96;
const SIG_ABS_KS: [f64; 5] = [8.0, 16.0, 32.0, 64.0, 128.0];
const SIG_RATE_PER_10K: [f64; 4] = [10.0, 20.0, 40.0, 80.0];
const SIG_FLOORS: [f64; 3] = [0.5, 0.75, 0.9];
/// Reference cell for the "surfaced" volume, histogram, samples, specials and
/// regression join — the ADR 0050 spacing analog (absolute knee 32, floor 0.5,
/// z 1.96). NOT a proposed default.
const SIG_REF_K: f64 = 32.0;
const SIG_REF_FLOOR: f64 = 0.5;
const SAMPLE_CAP: usize = 12;

/// The ADR 0050 calibration corpora, with the doc's short id. `my_juds` has no
/// file in the current vref fleet (pre-rename); `mya` is the Burmese stand-in
/// (same spaced-final ` ၏` phenomenon, 46,617 finals).
const SIG_REGRESSION: &[(&str, &str)] = &[
    ("engwebster", "engwebster"),
    ("WA-kmr-IQ-badini-reg", "kmr-IQ"),
    ("udu", "udu"),
    ("WA-ne-udb", "ne_udb"),
    ("WA-pa-ulb", "pa_ulb"),
    ("mya", "my_juds→mya"),
];

/// Focus marks for the fleet-wide summed distribution table.
const SIG_FOCUS_MARKS: &[char] = &[
    '.', ',', ';', ':', '?', '!', '\u{00BF}', '\u{00A1}', '\u{0964}', '\u{06D4}', '\u{060C}',
    '\u{061F}', '\u{061B}', '\u{1362}', '\u{1364}', '\u{1365}', '\u{104A}', '\u{104B}', '\u{17D4}',
    '/',
];

/// Named per-corpus sanity checks (corpus, marks to print).
const SIG_SANITY: &[(&str, &[char])] = &[
    ("eng-web", &[',', '.']),
    ("spaRV1909", &['\u{00BF}', '\u{00A1}', '?', '!']),
    ("WA-es-419-ulb", &['\u{00BF}', '?']),
    ("fraLSG", &['?', '!', ';', ':']),
    ("WA-pa-ulb", &['?', '!', ':']),
];

/// Wilson lower bound — a harness-local copy of `evidence::wilson_lower_bound`
/// (that module is `pub(crate)`, unreachable from an example). Kept
/// byte-for-byte so the spike's dominance matches the production factor.
fn sig_wilson_lb(k: u64, n: u64, z: f64) -> f64 {
    let z = z.max(0.0);
    let n = n as f64;
    let p = (k as f64 / n).clamp(0.0, 1.0);
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    (center - margin).clamp(0.0, 1.0)
}

/// Conservative dominance of the *complement* of one signature: how strongly the
/// mark's other signatures hold the field (ADR 0029/0048). A dominant signature
/// (count ≈ total) has a tiny complement ⇒ ~0 ⇒ silent; a rare one ⇒ ~1.
fn sig_dominance(count: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    sig_wilson_lb(total.saturating_sub(count), total, SIG_Z)
}

/// Two-factor signature score at an absolute recurrence knee `k`
/// (`dominance(complement) × rarity(count)`), reusing the shared `rarity_abs`.
fn sig_score_abs(count: u64, total: u64, k: f64) -> f64 {
    sig_dominance(count, total) * rarity_abs(count, k)
}

/// Same score at a volume-scaled (rate) knee `K = 1 + rate·total/10k`.
fn sig_score_rate(count: u64, total: u64, rate: f64) -> f64 {
    sig_score_abs(count, total, 1.0 + rate * total as f64 / 10_000.0)
}

fn sig_is_spacing_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{202F}')
}

/// The live spacing rule's candidate domain: GC `Po` minus quotes (ADR 0033).
fn sig_is_separator(c: char) -> bool {
    ssc_core::unicode::is_other_punctuation(c) && !class_of(c).is_quote()
}

/// Classify a non-whitespace neighbour grapheme into a context category.
/// Letters (incl. base+combining clusters) → `Letter`; a leading numeric →
/// `Digit`; everything else non-word (punct, symbols, lone marks) → `Punct`.
fn sig_categorize(cluster: &str) -> Ctx {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return Ctx::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_numeric() => Ctx::Digit,
        _ => Ctx::Punct,
    }
}

/// One separator-mark occurrence's joint context signature.
struct SigOpp {
    mark: char,
    left: Ctx,
    right: Ctx,
    /// The verse seam was reached on that side (with only whitespace between).
    /// The side already reads `Space` — the seam IS whitespace to the model —
    /// these bools exist only for the dissolved-special-case tally and the
    /// new-coverage filter, never as a context category.
    left_seam: bool,
    right_seam: bool,
    /// Byte offset of the mark scalar within the verse (the join key with the
    /// live rule's finding, whose `range.end` is the mark end).
    mark_off: usize,
}

/// Extract every separator mark's `(left, right)` signature from a verse.
/// Unlike the live `spacing_opportunities`, the left neighbour need not be a
/// letter — a digit / punct context becomes its own signature rather than an
/// exclusion (the plan's dissolved-special-case dividend), and the verse seam
/// reads as whitespace (`Space`). A mark carrying a combining cluster is
/// excluded exactly as in the live rule.
fn signature_opportunities(text: &str, graphemes: &[ssc_core::grapheme::GSpan]) -> Vec<SigOpp> {
    let mut out = Vec::new();
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        let mark = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && sig_is_separator(c) => c,
            _ => continue,
        };
        // Left: walk over horizontal whitespace to the governing neighbour.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 {
            let ps = graphemes[j - 1].slice(text);
            if !ps.is_empty() && ps.chars().all(sig_is_spacing_ws) {
                left_ws = true;
                j -= 1;
            } else {
                break;
            }
        }
        let left_seam = j == 0;
        let left = if left_seam || left_ws {
            Ctx::Space
        } else {
            sig_categorize(graphemes[j - 1].slice(text))
        };
        // Right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() {
            let ns = graphemes[k + 1].slice(text);
            if !ns.is_empty() && ns.chars().all(sig_is_spacing_ws) {
                right_ws = true;
                k += 1;
            } else {
                break;
            }
        }
        let right_seam = k + 1 >= graphemes.len();
        let right = if right_seam || right_ws {
            Ctx::Space
        } else {
            sig_categorize(graphemes[k + 1].slice(text))
        };
        out.push(SigOpp {
            mark,
            left,
            right,
            left_seam,
            right_seam,
            mark_off: gs.start as usize,
        });
    }
    out
}

/// One sampled site for human review.
#[derive(Clone)]
struct SigSample {
    corpus: String,
    sid: String,
    mark: char,
    sig: usize,
    count: u64,
    total: u64,
    score: f64,
    ctx: String,
}

/// Keep the top-`cap` samples by score.
fn push_capped(v: &mut Vec<SigSample>, s: SigSample, cap: usize) {
    if v.len() < cap {
        v.push(s);
    } else if let Some((i, min)) = v
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap())
        && s.score > min.score
    {
        v[i] = s;
    }
}

fn sig_context(text: &str, start: usize, end: usize) -> String {
    let before = text[..start]
        .char_indices()
        .rev()
        .nth(24)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let after = text[end..]
        .char_indices()
        .nth(24)
        .map(|(i, _)| end + i)
        .unwrap_or(text.len());
    text[before..after].replace(['\t', '\n'], " ")
}

/// Per-corpus signature result, fleet-summable.
struct SigCorpus {
    id: String,
    verses: usize,
    total_scalars: u64,
    digit_scalars: u64,
    /// mark → 16-cell signature histogram (seam pooled into `space`; the
    /// seam-involved subset is tallied into `verse_edge` during analysis and
    /// not stored — the seam is whitespace to this model).
    marks: BTreeMap<char, [u64; SIG_CELLS]>,
    ref_hist: [u64; 40],
    ref_surfaced: u64,
    /// Surfaced-occurrence volume grids `[knee][floor]`.
    abs_grid: Vec<[u64; SIG_FLOORS.len()]>,
    rate_grid: Vec<[u64; SIG_FLOORS.len()]>,
    /// Dissolved special cases at the reference cell: (total occurrences, of
    /// which score < floor ⇒ learned-silent).
    colon_num: (u64, u64),
    cluster_tail: (u64, u64),
    verse_edge: (u64, u64),
    /// Surfaced occurrences whose signature carries a `Digit` side — the
    /// rare-context (not misplacement) false-positive class.
    digit_surfaced: u64,
    surfaced_samples: Vec<SigSample>,
    new_coverage: Vec<SigSample>,
    fp_samples: Vec<SigSample>,
}

fn sig_bucket(score: f64) -> usize {
    (score.clamp(0.0, 0.999_999) * 40.0) as usize
}

fn analyze_signatures(id: String, map: &Corpus) -> SigCorpus {
    let mut marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut seam_marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut total_scalars = 0u64;
    let mut digit_scalars = 0u64;
    let mut graphemes = Vec::new();

    // Pass 1 — build the per-mark signature distribution + scalar tallies.
    for text in map.texts() {
        for c in text.chars() {
            total_scalars += 1;
            if class_of(c).is_numeric() {
                digit_scalars += 1;
            }
        }
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let i = sig_index(opp.left, opp.right);
            marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            if opp.left_seam || opp.right_seam {
                seam_marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            }
        }
    }

    // Derive scored rollups from the distribution.
    let mut ref_hist = [0u64; 40];
    let mut ref_surfaced = 0u64;
    let mut abs_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_ABS_KS.len()];
    let mut rate_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_RATE_PER_10K.len()];
    let (mut colon_num, mut cluster_tail, mut verse_edge) =
        ((0u64, 0u64), (0u64, 0u64), (0u64, 0u64));
    let mut digit_surfaced = 0u64;

    for counts in marks.values() {
        let total: u64 = counts.iter().sum();
        for (i, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let (l, r) = sig_ctx(i);
            let ref_s = sig_score_abs(count, total, SIG_REF_K);
            ref_hist[sig_bucket(ref_s)] += count;
            let surfaced = ref_s >= SIG_REF_FLOOR;
            if surfaced {
                ref_surfaced += count;
                if l == Ctx::Digit || r == Ctx::Digit {
                    digit_surfaced += count;
                }
            }
            // Dissolved special cases (counted at the reference cell).
            if l == Ctx::Digit && r == Ctx::Digit {
                colon_num.0 += count;
                colon_num.1 += u64::from(!surfaced) * count;
            }
            if l == Ctx::Punct {
                cluster_tail.0 += count;
                cluster_tail.1 += u64::from(!surfaced) * count;
            }
            // Sweep grids.
            for (ki, &k) in SIG_ABS_KS.iter().enumerate() {
                let s = sig_score_abs(count, total, k);
                for (fi, &fl) in SIG_FLOORS.iter().enumerate() {
                    if s >= fl {
                        abs_grid[ki][fi] += count;
                    }
                }
            }
            for (ki, &rate) in SIG_RATE_PER_10K.iter().enumerate() {
                let s = sig_score_rate(count, total, rate);
                for (fi, &fl) in SIG_FLOORS.iter().enumerate() {
                    if s >= fl {
                        rate_grid[ki][fi] += count;
                    }
                }
            }
        }
    }

    // Dissolved verse-edge special case: seam-involved occurrences (a walk
    // that reached the verse boundary), judged by the score of their pooled
    // space-read signature — the seam contributes no category of its own.
    for (mark, scounts) in &seam_marks {
        let counts = &marks[mark];
        let total: u64 = counts.iter().sum();
        for (i, &n) in scounts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let surfaced = sig_score_abs(counts[i], total, SIG_REF_K) >= SIG_REF_FLOOR;
            verse_edge.0 += n;
            verse_edge.1 += u64::from(!surfaced) * n;
        }
    }

    // Pass 2 — bounded samples (surfaced / new-coverage / digit-context FP).
    let mut surfaced_samples = Vec::new();
    let mut new_coverage = Vec::new();
    let mut fp_samples = Vec::new();
    for (sid, text) in map.keys().iter().zip(map.texts()) {
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let counts = &marks[&opp.mark];
            let total: u64 = counts.iter().sum();
            let i = sig_index(opp.left, opp.right);
            let count = counts[i];
            let score = sig_score_abs(count, total, SIG_REF_K);
            if score < SIG_REF_FLOOR {
                continue;
            }
            let make = || SigSample {
                corpus: id.clone(),
                sid: sid.to_string(),
                mark: opp.mark,
                sig: i,
                count,
                total,
                score,
                ctx: sig_context(text, opp.mark_off, opp.mark_off + opp.mark.len_utf8()),
            };
            push_capped(&mut surfaced_samples, make(), SAMPLE_CAP);
            // New coverage = an anomaly on the AFTER side, invisible to the
            // before-only live rule: mark attached to a following word/glyph
            // (`word,word`, `away!Why`, and a verse-leading `.word`).
            if opp.right == Ctx::Letter {
                push_capped(&mut new_coverage, make(), SAMPLE_CAP);
            }
            if opp.left == Ctx::Digit || opp.right == Ctx::Digit {
                push_capped(&mut fp_samples, make(), SAMPLE_CAP);
            }
        }
    }

    SigCorpus {
        id,
        verses: map.len(),
        total_scalars,
        digit_scalars,
        marks,
        ref_hist,
        ref_surfaced,
        abs_grid,
        rate_grid,
        colon_num,
        cluster_tail,
        verse_edge,
        digit_surfaced,
        surfaced_samples,
        new_coverage,
        fp_samples,
    }
}

/// Print one mark's top signatures by share.
fn print_mark_dist(mark: char, counts: &[u64; SIG_CELLS], top: usize) {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return;
    }
    let mut cells: Vec<(usize, u64)> = counts
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, &n)| (i, n))
        .collect();
    cells.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let shown: Vec<String> = cells
        .iter()
        .take(top)
        .map(|(i, n)| {
            format!(
                "{}={} ({:.1}% s={:.2})",
                sig_label(*i),
                n,
                *n as f64 * 100.0 / total as f64,
                sig_score_abs(*n, total, SIG_REF_K),
            )
        })
        .collect();
    println!(
        "  {:?} U+{:04X}  N={:<7} sigs={:<2} | {}",
        mark,
        mark as u32,
        total,
        cells.len(),
        shown.join("  "),
    );
}

fn print_sig_samples(samples: &[SigSample]) {
    for s in samples {
        println!(
            "  {:<22} {:<10} {:?} {:<13} count={:<5} N={:<7} score={:.3} | {}",
            s.corpus,
            s.sid,
            s.mark,
            sig_label(s.sig),
            s.count,
            s.total,
            s.score,
            s.ctx,
        );
    }
}

fn print_sig_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!(
        "\nsignature-score histogram over all mark occurrences (ref knee k=32) — {total} occurrences:"
    );
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>9} {bar}", lo + 0.025);
    }
}

fn print_sig_grids(abs: &[[u64; SIG_FLOORS.len()]], rate: &[[u64; SIG_FLOORS.len()]]) {
    println!(
        "\nsurfaced-occurrence volume sweep (cells = occurrences whose signature clears the floor):"
    );
    let header = || {
        print!("    {:>8}", "knee");
        for fl in SIG_FLOORS {
            print!("  {:>10}", format!("floor {fl:.2}"));
        }
        println!();
    };
    println!("  absolute knee K = k:");
    header();
    for (&k, row) in SIG_ABS_KS.iter().zip(abs) {
        print!("    {k:>8.0}");
        for &cell in row {
            print!("  {cell:>10}");
        }
        println!();
    }
    println!("  rate knee K = 1 + rate·N/10k:");
    header();
    for (&rate, row) in SIG_RATE_PER_10K.iter().zip(rate) {
        print!("    {rate:>8.0}");
        for &cell in row {
            print!("  {cell:>10}");
        }
        println!();
    }
}

fn silent_pct(pair: (u64, u64)) -> f64 {
    pair.1 as f64 * 100.0 / pair.0.max(1) as f64
}

/// Detailed single-corpus signature report.
fn signature_single_report(c: &SigCorpus) {
    println!(
        "=== ATTACHMENT-SIGNATURES SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    println!(
        "separator-mark occurrences: {}  distinct marks: {}  digit share of scalars: {:.3}%",
        c.marks.values().map(|m| m.iter().sum::<u64>()).sum::<u64>(),
        c.marks.len(),
        c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
    );
    println!("\nper-mark signature distributions (top 6, ref-knee score shown):");
    let mut order: Vec<(&char, &[u64; SIG_CELLS])> = c.marks.iter().collect();
    order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().sum::<u64>()));
    for (mark, counts) in order {
        print_mark_dist(*mark, counts, 6);
    }
    print_sig_grids(&c.abs_grid, &c.rate_grid);
    print_sig_hist(&c.ref_hist);
    println!(
        "\nreference cell (k=32, floor 0.5): surfaced {} occurrences ({} digit-context)",
        c.ref_surfaced, c.digit_surfaced
    );
    println!("\ndissolved special cases (ref cell; silent = learned below floor):");
    println!(
        "  numeric-flanked (digit|digit): {} occ, {:.1}% silent",
        c.colon_num.0,
        silent_pct(c.colon_num)
    );
    println!(
        "  cluster tail   (punct|*)     : {} occ, {:.1}% silent",
        c.cluster_tail.0,
        silent_pct(c.cluster_tail)
    );
    println!(
        "  verse edge     (edge|* / *|edge): {} occ, {:.1}% silent",
        c.verse_edge.0,
        silent_pct(c.verse_edge)
    );
    let mut s = c.surfaced_samples.clone();
    s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop surfaced samples (ref cell):");
    print_sig_samples(&s);
    let mut nc = c.new_coverage.clone();
    nc.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\nnew-coverage samples (after-side anomaly, invisible to the live rule):");
    print_sig_samples(&nc);
    if SIG_REGRESSION.iter().any(|&(id, _)| id == c.id) {
        println!("\n-- regression vs the live spacing rule --");
        signature_regression(&c.id);
    }
}

/// Regression: for the sites the live `punct.spacing-anomaly` surfaces today
/// (shipped defaults), what does the signature model say? Reloads the corpus,
/// runs the production rule, and joins by (sid, mark byte-offset).
fn signature_regression(id: &str) {
    use std::collections::HashMap;

    let path = Path::new("corpora/vref").join(format!("{id}.txt"));
    let map = load_corpus(&path);
    if map.is_empty() {
        println!("  {id}: (no corpus file)");
        return;
    }
    let books = ssc_core::corpus::by_book(&map);

    // Live rule at shipped defaults, floor 0 — every scored minority site, so we
    // can split by the shipped floor ourselves.
    let live = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let live_floor = f64::from(PunctuationSpacingConfig::default().emit_score_min);
    let findings = live.judge(&live.reduce(&books, None, None).0, &books, None, None);

    // Signature distribution + a (key, mark_off) → signature index lookup.
    let mut marks: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut site_sig: HashMap<(String, usize), usize> = HashMap::new();
    let mut graphemes = Vec::new();
    for (key, text) in map.keys().iter().zip(map.texts()) {
        graphemes.clear();
        ssc_core::grapheme::segment(text, &mut graphemes);
        for opp in signature_opportunities(text, &graphemes) {
            let i = sig_index(opp.left, opp.right);
            marks.entry(opp.mark).or_insert([0u64; SIG_CELLS])[i] += 1;
            site_sig.insert((key.clone(), opp.mark_off), i);
        }
    }
    let sig_verdict = |mark: char, sig: usize| -> (u64, u64, f64) {
        let counts = &marks[&mark];
        let total: u64 = counts.iter().sum();
        let count = counts[sig];
        (count, total, sig_score_abs(count, total, SIG_REF_K))
    };

    let mut live_surfaced = 0u64;
    let mut kept = 0u64;
    let mut dropped = 0u64;
    let mut rows: Vec<String> = Vec::new();
    for f in &findings {
        let Some(FindingArgs::SpacingConvention { mark, .. }) = f.args else {
            continue;
        };
        let live_score = f.score.unwrap_or(0.0) as f64;
        if live_score < live_floor {
            continue;
        }
        live_surfaced += 1;
        let key = map.key(f.key_idx);
        let text = map.text(f.key_idx);
        // The redesigned rule's span is the mark's *neighbourhood* (ADR 0054),
        // not the bare mark, so recover the mark scalar's offset by locating it
        // inside the finding range rather than from `range.end`.
        let mark_off = text[f.range.start as usize..f.range.end as usize]
            .find(mark)
            .map(|rel| f.range.start as usize + rel);
        let Some(sig) = mark_off.and_then(|off| site_sig.get(&(key.to_string(), off)).copied())
        else {
            rows.push(format!(
                "    {:<10} {:?} live={:.3} | (no signature match)",
                key, mark, live_score
            ));
            continue;
        };
        let (count, total, s) = sig_verdict(mark, sig);
        if s >= SIG_REF_FLOOR {
            kept += 1;
        } else {
            dropped += 1;
        }
        if rows.len() < 14 {
            rows.push(format!(
                "    {:<10} {:?} live={:.3} → sig {} count={}/{} score={:.3} [{}]",
                key,
                mark,
                live_score,
                sig_label(sig),
                count,
                total,
                s,
                if s >= SIG_REF_FLOOR {
                    "KEPT"
                } else {
                    "dropped"
                },
            ));
        }
    }
    // Signature-model surfaced total (ref cell) for context.
    let mut sig_surfaced = 0u64;
    for counts in marks.values() {
        let total: u64 = counts.iter().sum();
        for &count in counts.iter() {
            if count > 0 && sig_score_abs(count, total, SIG_REF_K) >= SIG_REF_FLOOR {
                sig_surfaced += count;
            }
        }
    }

    println!(
        "  {id}: live surfaced today {live_surfaced} → signature model KEEPS {kept}, drops {dropped}  (signature-model total surfaced at ref: {sig_surfaced})"
    );
    for r in &rows {
        println!("{r}");
    }
}

/// Fleet aggregate over every vref corpus in `dir`.
fn signature_fleet(dir: &Path) {
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
    eprintln!("signatures fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<SigCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let c = analyze_signatures(id, &load_corpus(path));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("signatures fleet analyze: {:?}", t0.elapsed());

    // Aggregates.
    let mut ref_hist = [0u64; 40];
    let mut ref_surfaced = 0u64;
    let mut digit_surfaced = 0u64;
    let mut abs_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_ABS_KS.len()];
    let mut rate_grid = vec![[0u64; SIG_FLOORS.len()]; SIG_RATE_PER_10K.len()];
    let (mut colon_num, mut cluster_tail, mut verse_edge) =
        ((0u64, 0u64), (0u64, 0u64), (0u64, 0u64));
    let mut focus: BTreeMap<char, [u64; SIG_CELLS]> = BTreeMap::new();
    let mut mark_occ_total = 0u64;
    // Noisiest-by-digit-context corpora (FP class), with digit share.
    let mut digit_rows: Vec<(String, u64, f64)> = Vec::new();
    let mut new_coverage: Vec<SigSample> = Vec::new();
    let mut fp_samples: Vec<SigSample> = Vec::new();
    let mut surfaced_samples: Vec<SigSample> = Vec::new();

    for c in &corpora {
        for (h, ch) in ref_hist.iter_mut().zip(&c.ref_hist) {
            *h += ch;
        }
        ref_surfaced += c.ref_surfaced;
        digit_surfaced += c.digit_surfaced;
        for (g, cg) in abs_grid.iter_mut().zip(&c.abs_grid) {
            for (x, y) in g.iter_mut().zip(cg) {
                *x += y;
            }
        }
        for (g, cg) in rate_grid.iter_mut().zip(&c.rate_grid) {
            for (x, y) in g.iter_mut().zip(cg) {
                *x += y;
            }
        }
        colon_num.0 += c.colon_num.0;
        colon_num.1 += c.colon_num.1;
        cluster_tail.0 += c.cluster_tail.0;
        cluster_tail.1 += c.cluster_tail.1;
        verse_edge.0 += c.verse_edge.0;
        verse_edge.1 += c.verse_edge.1;
        for (&mark, counts) in &c.marks {
            mark_occ_total += counts.iter().sum::<u64>();
            if SIG_FOCUS_MARKS.contains(&mark) {
                let e = focus.entry(mark).or_insert([0u64; SIG_CELLS]);
                for (x, y) in e.iter_mut().zip(counts) {
                    *x += y;
                }
            }
        }
        if c.digit_surfaced > 0 {
            digit_rows.push((
                c.id.clone(),
                c.digit_surfaced,
                c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
            ));
        }
        new_coverage.extend(c.new_coverage.iter().cloned());
        fp_samples.extend(c.fp_samples.iter().cloned());
        surfaced_samples.extend(c.surfaced_samples.iter().cloned());
    }
    eprintln!("signatures fleet tally: {:?}", t0.elapsed());

    println!("=== ATTACHMENT-SIGNATURES SPIKE — fleet aggregate ({total} corpora) ===");
    println!("total separator-mark occurrences: {mark_occ_total}");

    println!("\n-- fleet-summed per-mark signature distributions (major marks; top 6) --");
    println!(
        "   (raw counts summed across corpora mix conventions — a shape check, not a per-corpus verdict)"
    );
    for &mark in SIG_FOCUS_MARKS {
        if let Some(counts) = focus.get(&mark) {
            print_mark_dist(mark, counts, 6);
        }
    }

    println!("\n-- per-corpus sanity checks --");
    for &(id, wanted) in SIG_SANITY {
        let Some(c) = corpora.iter().find(|c| c.id == id) else {
            println!("  {id}: (absent from fleet)");
            continue;
        };
        println!("  [{id}]");
        for &mark in wanted {
            if let Some(counts) = c.marks.get(&mark) {
                print_mark_dist(mark, counts, 5);
            } else {
                println!("  {mark:?} U+{:04X}  (not present)", mark as u32);
            }
        }
    }

    print_sig_grids(&abs_grid, &rate_grid);
    print_sig_hist(&ref_hist);
    println!(
        "\nreference cell (k=32, floor 0.5): surfaced {ref_surfaced} occurrences ({digit_surfaced} digit-context)"
    );

    println!("\n-- dissolved special cases (fleet; ref cell; silent = learned below floor) --");
    println!(
        "  numeric-flanked (digit|digit): {:>10} occ, {:.2}% silent  (the `1:1` colon class)",
        colon_num.0,
        silent_pct(colon_num)
    );
    println!(
        "  cluster tail   (punct|*)     : {:>10} occ, {:.2}% silent  (the `?!`-tail `!` class)",
        cluster_tail.0,
        silent_pct(cluster_tail)
    );
    println!(
        "  verse edge     (edge involved): {:>10} occ, {:.2}% silent  (verse-leading/trailing marks)",
        verse_edge.0,
        silent_pct(verse_edge)
    );

    println!("\n-- regression vs the live spacing rule (ADR 0050 calibration corpora) --");
    for &(id, short) in SIG_REGRESSION {
        println!("  ({short})");
        signature_regression(id);
    }

    // New-coverage review table: diverse after-side anomalies, ≤2 per corpus.
    new_coverage.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut nc_diverse: Vec<SigSample> = Vec::new();
    let mut per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &new_coverage {
        let seen = per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            nc_diverse.push(s.clone());
        }
        if nc_diverse.len() >= 24 {
            break;
        }
    }
    println!(
        "\n-- new-coverage samples: after-side anomalies the live rule cannot see (up to 24) --"
    );
    print_sig_samples(&nc_diverse);

    // False-positive focus: noisiest digit-context corpora + a sample.
    digit_rows.sort_by_key(|b| std::cmp::Reverse(b.1));
    println!(
        "\n-- false-positive focus: rare-CONTEXT signatures (digit side), noisiest corpora --"
    );
    println!(
        "   digit_surfaced = surfaced occurrences with a digit neighbour; a low digit share means the context is rare, not the mark misplaced"
    );
    for (id, n, share) in digit_rows.iter().take(15) {
        println!("  {id:<24} digit-context surfaced {n:>6}  (digit scalars {share:.3}% of corpus)");
    }
    fp_samples.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut fp_diverse: Vec<SigSample> = Vec::new();
    let mut fp_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &fp_samples {
        let seen = fp_per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            fp_diverse.push(s.clone());
        }
        if fp_diverse.len() >= 16 {
            break;
        }
    }
    println!("\n  digit-context sample sites (up to 16):");
    print_sig_samples(&fp_diverse);

    // Overall surfaced samples (top by score, diversified).
    surfaced_samples.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then_with(|| a.corpus.cmp(&b.corpus))
    });
    let mut top_diverse: Vec<SigSample> = Vec::new();
    let mut top_per_corpus: BTreeMap<String, u64> = BTreeMap::new();
    for s in &surfaced_samples {
        let seen = top_per_corpus.entry(s.corpus.clone()).or_default();
        if *seen < 2 {
            *seen += 1;
            top_diverse.push(s.clone());
        }
        if top_diverse.len() >= 20 {
            break;
        }
    }
    println!("\n-- top surfaced samples fleet-wide (up to 20, ≤2 per corpus) --");
    print_sig_samples(&top_diverse);
}

#[cfg(test)]
mod signature_tests {
    use super::*;

    fn seg(text: &str) -> Vec<ssc_core::grapheme::GSpan> {
        let mut g = Vec::new();
        ssc_core::grapheme::segment(text, &mut g);
        g
    }
    fn sigs(text: &str) -> Vec<(char, Ctx, Ctx)> {
        signature_opportunities(text, &seg(text))
            .into_iter()
            .map(|o| (o.mark, o.left, o.right))
            .collect()
    }

    #[test]
    fn comma_before_and_after_side() {
        // English attached comma: letter on the left, space on the right.
        assert_eq!(sigs("word, word"), vec![(',', Ctx::Letter, Ctx::Space)]);
        // Spaced-before comma: the live rule's minority form ⇒ space|space.
        assert_eq!(sigs("word , word"), vec![(',', Ctx::Space, Ctx::Space)]);
        // Missing space after (invisible to the before-only live rule).
        assert_eq!(sigs("word,word"), vec![(',', Ctx::Letter, Ctx::Letter)]);
    }

    #[test]
    fn numeric_colon_is_a_digit_signature_not_an_exclusion() {
        // `1:1` — the live rule drops it (no letter governs); here it is a
        // first-class digit|digit signature.
        assert_eq!(sigs("1:1"), vec![(':', Ctx::Digit, Ctx::Digit)]);
    }

    #[test]
    fn cluster_tail_reads_punct_on_the_left() {
        // `?!` — `?` is letter|punct, its tail `!` is punct|space (the plan's
        // prediction). Both are ordinary signatures, no special case.
        assert_eq!(
            sigs("what?! yes"),
            vec![
                ('?', Ctx::Letter, Ctx::Punct),
                ('!', Ctx::Punct, Ctx::Space)
            ]
        );
    }

    #[test]
    fn away_then_capital_is_letter_letter() {
        // `away!Why` — the `!` clings to a following word: letter|letter.
        assert_eq!(sigs("away!Why"), vec![('!', Ctx::Letter, Ctx::Letter)]);
    }

    #[test]
    fn verse_seam_reads_as_whitespace_not_a_category() {
        // Ruling 2026-07-10: verses are addressing only; a terminal is never
        // "attached" across a seam, so the seam pools with `space`. A
        // verse-leading mark reads space on the left; a verse-trailing mark
        // reads space on the right (with or without literal trailing ws).
        assert_eq!(sigs(".word"), vec![('.', Ctx::Space, Ctx::Letter)]);
        assert_eq!(sigs("word."), vec![('.', Ctx::Letter, Ctx::Space)]);
        assert_eq!(sigs("word.  "), vec![('.', Ctx::Letter, Ctx::Space)]);
    }

    #[test]
    fn combining_cluster_mark_is_excluded_like_the_live_rule() {
        // A separator mark carrying a combining accent is not a clean site.
        let text = "word\u{0301}. next"; // the '.' is clean; ensure the accent on 'd' does not create a mark site
        let s = sigs(text);
        assert_eq!(s, vec![('.', Ctx::Letter, Ctx::Space)]);
    }

    #[test]
    fn quotes_are_not_separator_marks() {
        // Straight quotes are GC Po but excluded by the quote predicate.
        assert!(sigs("\"hi\"").is_empty());
    }

    #[test]
    fn score_is_dominance_of_complement_times_rarity() {
        // One rare signature against a strong majority scores high; the
        // dominant one scores ~0.
        // 100 occurrences: 99 in signature A, 1 in signature B.
        assert!(sig_score_abs(1, 100, 32.0) > 0.9, "rare minority is high");
        assert!(
            sig_score_abs(99, 100, 32.0) < 0.1,
            "dominant signature is silent"
        );
        // A recurring minority is discounted toward a second convention.
        assert!(sig_score_abs(40, 100, 32.0) < sig_score_abs(1, 100, 32.0));
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
    println!(
        "=== terminal_strength SPIKE: {} ({} verses, {}) ===",
        c.id,
        c.verses,
        if c.bicameral { "bicameral" } else { "caseless" }
    );
    println!(
        "jurors (word-starts ≥10): {}  dropped classes (<30 events): {}",
        c.trust.n_jurors, c.trust.dropped_classes
    );
    if let Some(r) = c.trust.reference {
        println!("agreement reference class: {}", r.label());
    }
    println!("\nper-class witnesses (sorted by trust_B):");
    println!(
        "  {:<8} {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>7} {:>7} {:>7}",
        "class", "events", "s_case", "dev", "diff", "agree", "asym", "sR_A", "trustA", "trustB"
    );
    let mut cls: Vec<&ClassTrust> = c.trust.classes.values().collect();
    cls.sort_by(|a, b| b.trust_b.partial_cmp(&a.trust_b).unwrap());
    for t in cls {
        println!(
            "  {:<8} {:>7} {:>7.3} {:>6.1} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>7.3} {:>7.3}",
            t.class.label(),
            t.events,
            if t.s_case_seen { t.s_case } else { f64::NAN },
            deviate(t),
            t.diff,
            t.agree,
            t.asym,
            t.s_reshuffle_a,
            t.trust_a,
            t.trust_b
        );
    }
    println!("\nwiring deltas (floor 0.95, k=32) — baseline vs trust-wired (variant B):");
    println!("  intrinsic  {:>6} → {:<6}", c.base_i, c.tr_i);
    println!("  positional {:>6} → {:<6}", c.base_p, c.tr_p);
    println!("  both       {:>6} → {:<6}", c.base_b, c.tr_b);
    println!(
        "  pool: gained-cap {}  lost-cap {}  intrinsic-flip {:+}",
        c.pool_gained, c.pool_lost, c.intrinsic_flips
    );
    println!(
        "  quote-context sites promoted & surfaced: {}",
        c.promoted_surfaced
    );
    if !c.anchors.is_empty() {
        println!("\nanchor fates:");
        for a in &c.anchors {
            println!(
                "  {:<9} {:<11} base={:.3}({}) tr={:.3}({}) quad={} class={} trust={:.3} habit={:.3}",
                a.sid,
                a.word,
                a.base_score,
                if a.base_alive { "alive" } else { "dead" },
                a.tr_score,
                if a.tr_alive { "alive" } else { "dead" },
                a.quad,
                a.class,
                a.trust,
                a.habit
            );
        }
    }
    let mut ch: Vec<&terminal::Change> = c.changes.iter().collect();
    ch.sort_by(|a, b| {
        b.tr_score
            .max(b.base_score)
            .partial_cmp(&a.tr_score.max(a.base_score))
            .unwrap()
    });
    println!("\nverdict changes ({} total; up to 25):", c.changes.len());
    for x in ch.iter().take(25) {
        println!(
            "  [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
            x.direction,
            x.sid,
            x.word,
            x.base_score,
            x.tr_score,
            x.quad,
            x.trust,
            x.habit,
            x.dominance,
            x.minority,
            x.rarity,
            x.ctx
        );
    }
    if !c.samples_promoted.is_empty() {
        println!("\npromoted quote-context sites (up to 15):");
        for s in c.samples_promoted.iter().take(15) {
            println!(
                "  {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                s.sid, s.word, s.class, s.trust, s.score, s.ctx
            );
        }
    }
}

const MAJOR: &[&str] = &[
    "eng-web",
    "eng-kjv",
    "engwebster",
    "WA-en-ulb",
    "spaRV1909",
    "WA-es-419-ulb",
    "fraLSG",
    "WA-fr-ulb",
    "porblt",
    "ita1885",
    "ron1924",
    "deu1912",
    "swhulb",
    "WA-sw-ulb",
    "ind",
    "nld",
    "vie1934",
    "tglulb",
];

/// Fleet aggregate: per-mark trust distributions, W2 variant comparison,
/// sigmoid-refit evidence, and casing wiring deltas vs the shipped baseline.
fn terminal_fleet(dir: &Path, variant_b: bool) {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let total = files.len();
    eprintln!(
        "terminal fleet: {total} corpora (W2 variant {})",
        if variant_b {
            "B (guarded)"
        } else {
            "A (plain)"
        }
    );
    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<TermCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = terminal::analyze_corpus(id, &map, variant_b);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("terminal fleet evaluate: {:?}", t0.elapsed());

    // ── Per-mark trust distributions (bare classes) across the fleet. ──
    // Collect per (mark, quoted) the trust/witness samples over corpora.
    let mut by_class: BTreeMap<ClassKey, Vec<&ClassTrust>> = BTreeMap::new();
    for c in &corpora {
        for t in c.trust.classes.values() {
            by_class.entry(t.class).or_default().push(t);
        }
    }
    let focus_bare = [
        '.', ',', '?', '!', ':', ';', '\u{2014}', '"', '\u{201D}', '-', '\u{2026}',
    ];
    println!(
        "\n=== TERMINAL_STRENGTH SPIKE — fleet ({} corpora) ===",
        corpora.len()
    );
    println!(
        "\n-- per-mark trust distribution (bare classes; median [p25,p75] max over corpora) --"
    );
    println!(
        "  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
        "mark", "corpora", "s_case", "s_reshuffle_A(diff)", "trust_A", "trust_B"
    );
    let fmtq = |v: &mut Vec<f64>| {
        let (p25, med, p75, mx) = quartiles(v);
        format!("{med:.2}[{p25:.2},{p75:.2}]mx{mx:.2}")
    };
    for &m in &focus_bare {
        let key = ClassKey {
            mark: m,
            quoted: false,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut sc: Vec<f64> = ts
                .iter()
                .filter(|t| t.s_case_seen)
                .map(|t| t.s_case)
                .collect();
            let mut di: Vec<f64> = ts.iter().map(|t| t.s_reshuffle_a).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            println!(
                "  {:<7} {:>7} {:>24} {:>24} {:>24} {:>24}",
                key.label(),
                ts.len(),
                fmtq(&mut sc),
                fmtq(&mut di),
                fmtq(&mut ta),
                fmtq(&mut tb)
            );
        }
    }
    println!("\n-- quote-context classes (mark+\") — the shortlist item-7 sweep --");
    println!(
        "  {:<7} {:>7} {:>24} {:>24}",
        "class", "corpora", "trust_B", "trust_A"
    );
    for &m in &['.', '?', '!', ':', ',', ';'] {
        let key = ClassKey {
            mark: m,
            quoted: true,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut tb: Vec<f64> = ts.iter().map(|t| t.trust_b).collect();
            let mut ta: Vec<f64> = ts.iter().map(|t| t.trust_a).collect();
            println!(
                "  {:<7} {:>7} {:>24} {:>24}",
                key.label(),
                ts.len(),
                fmtq(&mut tb),
                fmtq(&mut ta)
            );
        }
    }

    // ── Sigmoid-refit evidence: standardized deviate for '.' vs ','. ──
    println!("\n-- W2 sigmoid refit evidence: standardized multinomial-G² deviate --");
    for &m in &['.', ',', '?', '!', ':'] {
        let key = ClassKey {
            mark: m,
            quoted: false,
        };
        if let Some(ts) = by_class.get(&key) {
            let mut d: Vec<f64> = ts.iter().map(|t| deviate(t)).collect();
            let (p25, med, p75, mx) = quartiles(&mut d);
            println!(
                "  {:<5} dev median {med:.1} [{p25:.1},{p75:.1}] max {mx:.1}",
                key.label()
            );
        }
    }

    // ── W2 variant comparison: genealogy guard — worst comma offenders. ──
    println!("\n-- genealogy guard: corpora where ',' is most over-trusted by variant A --");
    let mut comma_rows: Vec<(&str, f64, f64, f64, f64)> = corpora
        .iter()
        .filter_map(|c| {
            c.trust
                .classes
                .get(&ClassKey {
                    mark: ',',
                    quoted: false,
                })
                .map(|t| (c.id.as_str(), t.trust_a, t.trust_b, t.diff, t.agree))
        })
        .collect();
    comma_rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    println!(
        "  {:<20} {:>8} {:>8} {:>8} {:>8}",
        "corpus", "trustA", "trustB", "diff", "agree"
    );
    for (id, ta, tb, d, ag) in comma_rows.iter().take(12) {
        println!(
            "  {:<20} {:>8.3} {:>8.3} {:>8.3} {:>8.3}",
            id, ta, tb, d, ag
        );
    }

    // ── Wiring deltas vs baseline. ──
    let (mut bi, mut bp, mut bb, mut ti, mut tp, mut tb) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut pg, mut pl) = (0u64, 0u64);
    let mut promoted = 0u64;
    let mut corpora_changed = 0u32;
    for c in &corpora {
        bi += c.base_i;
        bp += c.base_p;
        bb += c.base_b;
        ti += c.tr_i;
        tp += c.tr_p;
        tb += c.tr_b;
        pg += c.pool_gained;
        pl += c.pool_lost;
        promoted += c.promoted_surfaced;
        if !c.changes.is_empty() {
            corpora_changed += 1;
        }
    }
    println!(
        "\n-- wiring deltas (floor 0.95, k=32; variant {}) --",
        if variant_b { "B" } else { "A" }
    );
    println!("  channel     baseline   trust-wired      Δ");
    println!(
        "  intrinsic  {:>9} {:>13} {:>+7}",
        bi,
        ti,
        ti as i64 - bi as i64
    );
    println!(
        "  positional {:>9} {:>13} {:>+7}",
        bp,
        tp,
        tp as i64 - bp as i64
    );
    println!(
        "  both       {:>9} {:>13} {:>+7}",
        bb,
        tb,
        tb as i64 - bb as i64
    );
    println!(
        "  TOTAL      {:>9} {:>13} {:>+7}",
        bi + bp + bb,
        ti + tp + tb,
        (ti + tp + tb) as i64 - (bi + bp + bb) as i64
    );
    println!("  corpora with ≥1 verdict change: {}", corpora_changed);
    println!("  pool recovery: word profiles gained-cap {pg}, lost-cap {pl}");
    println!("  quote-context sites promoted & surfaced (item-7 payoff): {promoted}");

    // ── Anchor fates. ──
    println!("\n-- anchor fates (12 ADR 0051 anchors) --");
    println!(
        "  {:<9} {:<11} {:<10} {:>7} {:>7} {:<7} {:<10} {:>6} {:>6}",
        "corpus", "sid", "word", "base", "tr", "verdict", "class", "trust", "habit"
    );
    for &(ac, asid, aw) in terminal::ANCHORS {
        let fate = corpora
            .iter()
            .flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                let verdict = match (a.base_alive, a.tr_alive) {
                    (true, true) => "kept",
                    (true, false) => "DIED",
                    (false, true) => "born",
                    (false, false) => "silent",
                };
                println!(
                    "  {:<9} {:<11} {:<10} {:>7.3} {:>7.3} {:<7} {:<10} {:>6.3} {:>6.3}",
                    ac, asid, aw, a.base_score, a.tr_score, verdict, a.class, a.trust, a.habit
                );
            }
            None => println!(
                "  {:<9} {:<11} {:<10}  (not a candidate site)",
                ac, asid, aw
            ),
        }
    }

    // ── Top-10 corpora by positional-channel change. ──
    let mut ranked: Vec<&TermCorpus> = corpora.iter().filter(|c| c.pos_delta > 0).collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.pos_delta));
    println!("\n-- top-10 corpora by positional-channel change --");
    for c in ranked.iter().take(10) {
        println!(
            "  {:<20} pos {}→{} (Δ{:+})  examples:",
            c.id,
            c.base_p,
            c.tr_p,
            c.tr_p as i64 - c.base_p as i64
        );
        let mut ch: Vec<&terminal::Change> =
            c.changes.iter().filter(|x| x.quad != "intrinsic").collect();
        ch.sort_by(|a, b| {
            b.tr_score
                .max(b.base_score)
                .partial_cmp(&a.tr_score.max(a.base_score))
                .unwrap()
        });
        for x in ch.iter().take(3) {
            println!(
                "      [{}] {:<9} {:<12} base={:.3} tr={:.3} trust={:.3} | {}",
                x.direction, x.sid, x.word, x.base_score, x.tr_score, x.trust, x.ctx
            );
        }
    }

    // ── Changed-verdict samples from major-language corpora. ──
    println!("\n-- changed-verdict samples from major-language corpora (parametric review) --");
    let mut shown = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.changes.is_empty() {
            continue;
        }
        println!("  [{}]:", c.id);
        let mut ch: Vec<&terminal::Change> = c.changes.iter().collect();
        ch.sort_by(|a, b| {
            b.tr_score
                .max(b.base_score)
                .partial_cmp(&a.tr_score.max(a.base_score))
                .unwrap()
        });
        for x in ch.iter().take(3) {
            println!(
                "    [{}] {:<9} {:<14} base={:.3} tr={:.3} {} trust={:.3} habit={:.3} dom={:.3} min={} rar={:.3} | {}",
                x.direction,
                x.sid,
                x.word,
                x.base_score,
                x.tr_score,
                x.quad,
                x.trust,
                x.habit,
                x.dominance,
                x.minority,
                x.rarity,
                x.ctx
            );
            shown += 1;
        }
        if shown >= 25 {
            break;
        }
    }

    // ── Context-class payoff samples. ──
    println!("\n-- promoted quote-context sites from major-language corpora (item-7) --");
    let mut cnt = 0;
    for c in &corpora {
        if !MAJOR.contains(&c.id.as_str()) || c.samples_promoted.is_empty() {
            continue;
        }
        for s in c.samples_promoted.iter().take(2) {
            println!(
                "  [{}] {:<9} {:<14} class={} trust={:.3} score={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.ctx
            );
            cnt += 1;
        }
        if cnt >= 10 {
            break;
        }
    }
    if cnt == 0 {
        println!("  (none surfaced in major-language corpora)");
    }

    terminal_gate_sweep(&corpora, bi, bp, bb, ti, tp, tb, promoted, variant_b);
}

/// Gate-threshold sweep report (2026-07-10). `b*` are the shipped-baseline
/// channel totals, `t*` the multiplier wiring, `mult_promoted` the multiplier's
/// promoted-and-surfaced count (the 237). Each item mirrors the ADR packet.
#[allow(clippy::too_many_arguments)]
fn terminal_gate_sweep(
    corpora: &[TermCorpus],
    bi: u64,
    bp: u64,
    bb: u64,
    ti: u64,
    tp: u64,
    tb: u64,
    mult_promoted: u64,
    variant_b: bool,
) {
    let sweep = terminal::GATE_SWEEP;
    let n_t = sweep.len();
    let base_total = bi + bp + bb;
    let mult_total = ti + tp + tb;

    println!(
        "\n═══ GATE-THRESHOLD SWEEP (2026-07-10; variant {}) ═══",
        if variant_b { "B" } else { "A" }
    );

    // 1. Surfaced volume per channel + deltas vs baseline and multiplier.
    println!("\n-- 1. surfaced volume per channel (fleet) --");
    println!("  baseline (shipped): i {bi}  p {bp}  b {bb}  TOTAL {base_total}");
    println!("  multiplier wiring:  i {ti}  p {tp}  b {tb}  TOTAL {mult_total}");
    println!(
        "  {:<5} {:>8} {:>9} {:>6} {:>8} {:>10} {:>10}",
        "T", "intrins", "positnl", "both", "TOTAL", "Δ vs base", "Δ vs mult"
    );
    for (i, &t) in sweep.iter().enumerate() {
        let (mut gi, mut gp, mut gb) = (0u64, 0u64, 0u64);
        for c in corpora {
            let (a, b2, c2) = c.gate.counts[i];
            gi += a;
            gp += b2;
            gb += c2;
        }
        let total = gi + gp + gb;
        println!(
            "  {:<5.2} {:>8} {:>9} {:>6} {:>8} {:>+10} {:>+10}",
            t,
            gi,
            gp,
            gb,
            total,
            total as i64 - base_total as i64,
            total as i64 - mult_total as i64
        );
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
        let cs = cv
            .iter()
            .take(6)
            .map(|(k, v)| format!("{}:{}", k.label(), v))
            .collect::<Vec<_>>()
            .join("  ");
        println!("  {:<14} {:>7}   {}", format!("{lo:.2}→{hi:.2}"), total, cs);
    }

    // 3. The 12 ADR 0051 anchors, alive at each threshold (first 7 = TP).
    println!("\n-- 3. the 12 ADR 0051 anchors: alive at each threshold --");
    print!(
        "  {:<9} {:<11} {:<10} {:<4} {:<4}",
        "corpus", "sid", "word", "base", "mult"
    );
    for &t in sweep {
        print!(" {:>5.2}", t);
    }
    println!("   kind");
    let mut tp_deaths: Vec<(String, f64)> = Vec::new();
    for (idx, &(ac, asid, aw)) in terminal::ANCHORS.iter().enumerate() {
        let is_tp = idx < 7;
        let fate = corpora
            .iter()
            .flat_map(|c| c.anchors.iter())
            .find(|a| a.corpus == ac && a.sid == asid && a.word == aw);
        match fate {
            Some(a) => {
                print!(
                    "  {:<9} {:<11} {:<10} {:<4} {:<4}",
                    ac,
                    asid,
                    aw,
                    if a.base_alive { "✓" } else { "·" },
                    if a.tr_alive { "✓" } else { "·" }
                );
                for (i, &t) in sweep.iter().enumerate() {
                    print!(" {:>5}", if a.gate_alive[i] { "✓" } else { "·" });
                    if is_tp && !a.gate_alive[i] {
                        tp_deaths.push((format!("{ac} {asid} {aw} @T={t:.2}"), a.gate_score[i]));
                    }
                }
                println!("   {}", if is_tp { "TP" } else { "FP" });
            }
            None => println!(
                "  {:<9} {:<11} {:<10}  (not a candidate site)",
                ac, asid, aw
            ),
        }
    }
    if tp_deaths.is_empty() {
        println!("  ALL 7 TPs stay alive at every swept threshold.");
    } else {
        println!(
            "  ⚠ TP deaths: {}",
            tp_deaths
                .iter()
                .map(|(s, _)| s.clone())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // 4. Readmissions vs the multiplier wiring.
    println!("\n-- 4. readmissions vs the multiplier wiring (fleet) --");
    for (i, &t) in sweep.iter().enumerate() {
        let r: u64 = corpora.iter().map(|c| c.gate.readmitted[i]).sum();
        println!("  T={t:.2}: {r} findings the multiplier eroded, readmitted by the gate");
    }
    // The documented-known fraLSG MIC 2:6 disent-ils FP (expected readmitted).
    let fralsg = corpora
        .iter()
        .find(|c| c.id == "fraLSG")
        .and_then(|c| c.gate.readmit_samples.iter().find(|s| s.sid == "MIC 2:6"));
    match fralsg {
        Some(s) => println!(
            "  fraLSG MIC 2:6 [{}]: trust={:.3} gate-score={:.3} base={:.3} (readmitted; known FP) | {}",
            s.word, s.trust, s.score, s.base_score, s.ctx
        ),
        None => println!(
            "  fraLSG MIC 2:6 disent-ils: NOT in the readmit set (unexpected — investigate)"
        ),
    }
    // Per-major-corpus readmit tally (T=0.50, the maximal readmit set) — shows
    // how much of the fleet-wide readmission lands in major vs minority langs.
    println!("\n  readmit count per major-language corpus (T=0.50):");
    let major_readmit: u64 = corpora
        .iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()))
        .map(|c| c.gate.readmitted[0])
        .sum();
    let mut mr: Vec<(&str, u64)> = corpora
        .iter()
        .filter(|c| MAJOR.contains(&c.id.as_str()) && c.gate.readmitted[0] > 0)
        .map(|c| (c.id.as_str(), c.gate.readmitted[0]))
        .collect();
    mr.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!(
        "    {} of {} fleet readmissions land in major-language corpora: {}",
        major_readmit,
        corpora.iter().map(|c| c.gate.readmitted[0]).sum::<u64>(),
        mr.iter()
            .map(|(id, n)| format!("{id}:{n}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!("\n  readmitted-site sample from major-language corpora (verse text):");
    let mut shown = 0;
    for c in corpora {
        if !MAJOR.contains(&c.id.as_str()) {
            continue;
        }
        for s in &c.gate.readmit_samples {
            println!(
                "    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx
            );
            shown += 1;
            if shown >= 10 {
                break;
            }
        }
        if shown >= 10 {
            break;
        }
    }
    if shown == 0 {
        println!("    (no readmissions in major-language corpora)");
    }
    // Erosion lands overwhelmingly in minority-language corpora, so also show a
    // fleet-wide sample from the highest-readmit corpora for adjudication.
    println!("\n  fleet-wide readmitted-site sample (highest-readmit corpora):");
    let mut ranked: Vec<&TermCorpus> = corpora
        .iter()
        .filter(|c| c.gate.readmitted[0] > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.gate.readmitted[0]));
    let mut fshown = 0;
    for c in ranked {
        for s in c.gate.readmit_samples.iter().take(2) {
            println!(
                "    [{}] {:<9} {:<14} class={} trust={:.3} gate={:.3} base={:.3} | {}",
                c.id, s.sid, s.word, s.class, s.trust, s.score, s.base_score, s.ctx
            );
            fshown += 1;
            if fshown >= 10 {
                break;
            }
        }
        if fshown >= 10 {
            break;
        }
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
        let mut losers: Vec<&TermCorpus> = corpora
            .iter()
            .filter(|c| c.gate.base_pos > 0 && !c.gate.pos_alive[i])
            .collect();
        losers.sort_by_key(|c| std::cmp::Reverse(c.gate.base_pos));
        let names = losers
            .iter()
            .take(5)
            .map(|c| format!("{}(base_pos {})", c.id, c.gate.base_pos))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  T={t:.2}: {} corpora  [largest: {names}]", losers.len());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Mixed-case word SPIKE (plan rule 3). A word written in an internal-capital
// shape (`wOrd`, `McDonald`, `kiSwahili`, `LORDs`) is a slip unless it is a
// convention the recurrence machinery excuses. Harness-only: no production
// rule, `RuleStats`, or `CasingConfig` is touched (READ-ONLY). The token unit
// is the plain UAX #29 letter-run word (`is_letter_token`), so `Obed-Edom`
// splits into two Titlecase tokens and never reads as one mixed-case word —
// this is deliberately NOT the hyphen-merged `compound_words` the casing walk
// uses, matching the plan's "letter-run tokens" unit for rule 3. Sweep
// constants (PACKET_KS/PACKET_FLOORS/REF_K/REF_FLOOR) and `rarity_abs` are
// shared with the casing spike; `sig_wilson_lb` is the harness-local Wilson.
// ═══════════════════════════════════════════════════════════════════════════

const MC_Z: f64 = 1.96;
/// Corpora sampled in the fleet report for the convention-class adjudication.
const MC_MAJOR: &[&str] = &[
    "eng-web",
    "eng-kjv",
    "engwebster",
    "WA-en-ulb",
    "spaRV1909",
    "WA-es-419-ulb",
    "fraLSG",
    "WA-fr-ulb",
    "porblt",
    "deu1912",
    "swhulb",
    "WA-sw-ulb",
    "WA-bem-reg",
    "ind",
    "nld",
    "vie1934",
    "tglulb",
    "ron1924",
    "engojb",
];

/// A word's observed case shape over its **cased** letters (marks and caseless
/// letters ignored). `OtherMixed` is the `wOrd` candidate: it has both cases and
/// is neither Titlecase nor ALLCAPS, so it necessarily carries an *internal*
/// capital (an uppercase letter that is not the first cased letter).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum McShape {
    Lower,
    Title,
    AllCaps,
    OtherMixed,
}

/// Classify a letter-run token by the case sequence of its cased letters.
/// `None` = no cased letter (caseless script / marks only): no shape, not a
/// candidate, not counted in the cased denominator. A single cased letter is
/// Lower or AllCaps, never OtherMixed (the single-letter guard the plan warns
/// about: a lone `I`/`A` must not read as mixed). Combining marks and non-cased
/// letters inside a cased word are skipped, so an intra-word caseless glyph
/// cannot by itself manufacture a mixed shape.
fn mc_classify(word: &str) -> Option<McShape> {
    let mut cases: Vec<bool> = Vec::new(); // true = uppercase
    for c in word.chars() {
        let cl = class_of(c);
        if cl.is_uppercase() {
            cases.push(true);
        } else if cl.is_lowercase() {
            cases.push(false);
        }
    }
    if cases.is_empty() {
        return None;
    }
    let up = cases.iter().filter(|&&u| u).count();
    let n = cases.len();
    Some(if up == 0 {
        McShape::Lower
    } else if up == n {
        McShape::AllCaps
    } else if cases[0] && cases[1..].iter().all(|&u| !u) {
        McShape::Title
    } else {
        McShape::OtherMixed
    })
}

/// True iff the first *cased* letter of a mixed word is uppercase — the
/// boundary axis against casing v2. An upper-first OtherMixed word (`McDonald`,
/// `LORDs`) has an uppercase first letter, so casing (which fires only on a
/// lowercase word-start) never sees it: it is unambiguously mixed-case's. A
/// lower-first OtherMixed word (`wOrd`, `kiSwahili`) shares casing's
/// lowercase-site domain and is the overlap class.
fn mc_first_cased_upper(word: &str) -> bool {
    word.chars()
        .find_map(|c| {
            let cl = class_of(c);
            if cl.is_uppercase() {
                Some(true)
            } else if cl.is_lowercase() {
                Some(false)
            } else {
                None
            }
        })
        .unwrap_or(false)
}

/// One word type's shape profile within a corpus (raw counts). `other_first_upper`
/// and `other_forced` split the OtherMixed occurrences by the two axes the spike
/// adjudicates (casing overlap, and position-independence).
#[derive(Clone, Copy, Default)]
struct McProfile {
    lower: u64,
    title: u64,
    allcaps: u64,
    other: u64,
    other_first_upper: u64,
    other_forced: u64,
}

impl McProfile {
    fn total(&self) -> u64 {
        self.lower + self.title + self.allcaps + self.other
    }
    fn not_other(&self) -> u64 {
        self.total() - self.other
    }
}

/// One OtherMixed occurrence, retained (capped) only to draw review samples;
/// the volume grids come analytically from the per-type profiles, not these.
struct McCand {
    sid: String,
    start: u32,
    end: u32,
    key: String,
    first_upper: bool,
    forced: bool,
}

/// One sampled OtherMixed site for the review tables.
struct McSample {
    sid: String,
    word: String,
    route: &'static str,
    other: u64,
    word_total: u64,
    dom: f64,
    score: f64,
    first_upper: bool,
    forced: bool,
    ctx: String,
}

/// Per-corpus mixed-case result. Grids are `[knee][floor]`, fleet-summable.
struct McCorpus {
    id: String,
    verses: usize,
    cased_tokens: u64,
    other_tokens: u64,
    other_types: u64,
    hapax_other_types: u64,
    recurring_other_types: u64,
    corpus_dominance: f64,
    // Route 4 — position: OtherMixed vs all-cased at forced / mid positions.
    cased_forced: u64,
    other_forced: u64,
    cased_mid: u64,
    other_mid: u64,
    // Boundary vs casing v2: OtherMixed occurrences by first-cased-letter case.
    other_first_upper: u64,
    other_first_lower: u64,
    // Of the ref-flagged (route A) OtherMixed sites, the same two splits.
    flagged_first_upper: u64,
    flagged_forced: u64,
    // Volume grids: sites (occurrences) surfaced. [knee][floor].
    grid_within: Vec<[u64; PACKET_FLOORS.len()]>,
    grid_fallback: Vec<[u64; PACKET_FLOORS.len()]>,
    hist_within: [u64; 40],
    ref_within: u64,
    ref_fallback: u64,
    flagged_samples: Vec<McSample>,
    excused_samples: Vec<McSample>,
    fallback_samples: Vec<McSample>,
}

/// Walk each book mirroring `casing::walk_book`'s pending-terminal machine
/// (carried across verse seams, reset per book, book-initial forced), over the
/// plain UAX letter-run tokens. Returns the per-type profiles, corpus counters,
/// and a capped list of OtherMixed occurrences for sampling.
fn mc_walk(
    map: &Corpus,
) -> (
    BTreeMap<String, McProfile>,
    [u64; 4], // cased_forced, other_forced, cased_mid, other_mid
    Vec<McCand>,
) {
    let mut profiles: BTreeMap<String, McProfile> = BTreeMap::new();
    let mut counters = [0u64; 4];
    let mut cands: Vec<McCand> = Vec::new();
    const CAND_CAP: usize = 8000;

    for group in &ssc_core::corpus::by_book(map) {
        let mut pending: Option<bool> = None;
        let mut book_initial = true;
        for (sid, text) in group.keys.iter().zip(group.texts) {
            let mut prev_letter = false;
            let mut cursor = 0usize;
            for token in tokenize(text) {
                let word = token.span.slice(text);
                if !is_letter_token(word) {
                    continue; // its text stays in the gap the next word sees
                }
                glyph_advance_gap(
                    &text[cursor..token.span.start as usize],
                    &mut pending,
                    &mut prev_letter,
                );
                let forced = book_initial || matches!(pending.take(), Some(false));
                book_initial = false;
                prev_letter = word
                    .chars()
                    .next_back()
                    .is_some_and(|c| class_of(c).is_alphabetic());
                cursor = token.span.end as usize;

                let Some(shape) = mc_classify(word) else {
                    continue;
                };
                let entry = profiles.entry(word.to_lowercase()).or_default();
                let is_forced = forced;
                if is_forced {
                    counters[0] += 1;
                } else {
                    counters[2] += 1;
                }
                match shape {
                    McShape::Lower => entry.lower += 1,
                    McShape::Title => entry.title += 1,
                    McShape::AllCaps => entry.allcaps += 1,
                    McShape::OtherMixed => {
                        entry.other += 1;
                        let first_upper = mc_first_cased_upper(word);
                        if first_upper {
                            entry.other_first_upper += 1;
                        }
                        if is_forced {
                            entry.other_forced += 1;
                            counters[1] += 1;
                        } else {
                            counters[3] += 1;
                        }
                        if cands.len() < CAND_CAP {
                            cands.push(McCand {
                                sid: sid.clone(),
                                start: token.span.start,
                                end: token.span.end,
                                key: word.to_lowercase(),
                                first_upper,
                                forced: is_forced,
                            });
                        }
                    }
                }
            }
            glyph_advance_gap(&text[cursor..], &mut pending, &mut prev_letter);
        }
    }
    (profiles, counters, cands)
}

fn analyze_mixedcase(id: String, map: &Corpus) -> McCorpus {
    let (profiles, counters, cands) = mc_walk(map);
    let [cased_forced, other_forced, cased_mid, other_mid] = counters;
    let cased_tokens: u64 = profiles.values().map(McProfile::total).sum();
    let other_tokens: u64 = profiles.values().map(|p| p.other).sum();
    let other_first_upper: u64 = profiles.values().map(|p| p.other_first_upper).sum();
    // Corpus-level "tokens are not other-mixed" dominance — the route-B factor.
    let corpus_dominance = sig_wilson_lb(
        cased_tokens.saturating_sub(other_tokens),
        cased_tokens,
        MC_Z,
    );

    let nk = PACKET_KS.len();
    let mut grid_within = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_fallback = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist_within = [0u64; 40];
    let (mut ref_within, mut ref_fallback) = (0u64, 0u64);
    let (mut other_types, mut hapax_other_types, mut recurring_other_types) = (0u64, 0u64, 0u64);
    let (mut flagged_first_upper, mut flagged_forced) = (0u64, 0u64);

    // Category sets for sampling, decided at the reference cell (k=32, 0.95).
    use std::collections::{HashMap, HashSet};
    let mut flagged: HashSet<String> = HashSet::new();
    let mut fallback_keys: HashSet<String> = HashSet::new();
    // Excused = recurring OtherMixed (other >= 2) that route A leaves silent.
    let mut excused: Vec<(String, u64, u64)> = Vec::new();

    for (key, p) in &profiles {
        if p.other == 0 {
            continue;
        }
        other_types += 1;
        if p.total() == 1 {
            hapax_other_types += 1;
        }
        if p.other >= 2 {
            recurring_other_types += 1;
        }
        let dom = sig_wilson_lb(p.not_other(), p.total(), MC_Z);

        // Route A (within-word): each OtherMixed occurrence surfaces iff the
        // type's score clears the floor.
        for (ki, &k) in PACKET_KS.iter().enumerate() {
            let sc = dom * rarity_abs(p.other, k);
            for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                if sc >= fl {
                    grid_within[ki][fi] += p.other;
                }
            }
        }
        let sc_ref = dom * rarity_abs(p.other, REF_K);
        hist_within[(sc_ref.clamp(0.0, 0.999_999) * 40.0) as usize] += p.other;
        if sc_ref >= REF_FLOOR {
            ref_within += p.other;
            flagged.insert(key.clone());
            flagged_first_upper += p.other_first_upper;
            flagged_forced += p.other_forced;
        } else if p.other >= 2 {
            excused.push((key.clone(), p.other, p.total()));
        }

        // Route B (corpus fallback) — hapax OtherMixed words only (route A is
        // structurally silent for them: not_other == 0 ⇒ dominance 0). Score is
        // corpus_dominance × rarity(1, k) = corpus_dominance (knee-independent).
        if p.total() == 1 {
            for (ki, _k) in PACKET_KS.iter().enumerate() {
                for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
                    if corpus_dominance >= fl {
                        grid_fallback[ki][fi] += 1;
                    }
                }
            }
            if corpus_dominance >= REF_FLOOR {
                ref_fallback += 1;
                fallback_keys.insert(key.clone());
            }
        }
    }

    excused.sort_by_key(|(_, other, _)| std::cmp::Reverse(*other));
    let excused_set: HashSet<&str> = excused
        .iter()
        .take(30)
        .map(|(k, _, _)| k.as_str())
        .collect();

    // Draw review samples from the capped candidate list.
    let (mut flagged_samples, mut fallback_samples, mut excused_samples) =
        (Vec::new(), Vec::new(), Vec::new());
    let mut excused_seen: HashSet<String> = HashSet::new();
    let key_to_text: HashMap<&str, &str> = map
        .keys()
        .iter()
        .zip(map.texts())
        .map(|(k, t)| (k.as_str(), t.as_str()))
        .collect();
    for c in &cands {
        let Some(p) = profiles.get(&c.key) else {
            continue;
        };
        let text = key_to_text[c.sid.as_str()];
        let word = text[c.start as usize..c.end as usize].to_string();
        let dom = sig_wilson_lb(p.not_other(), p.total(), MC_Z);
        let mk = |route: &'static str, score: f64| McSample {
            sid: c.sid.to_string(),
            word: word.clone(),
            route,
            other: p.other,
            word_total: p.total(),
            dom,
            score,
            first_upper: c.first_upper,
            forced: c.forced,
            ctx: casing_ctx(text, c.start as usize, c.end as usize),
        };
        if flagged.contains(&c.key) && flagged_samples.len() < 60 {
            flagged_samples.push(mk("within", dom * rarity_abs(p.other, REF_K)));
        } else if fallback_keys.contains(&c.key) && fallback_samples.len() < 60 {
            fallback_samples.push(mk("fallback", corpus_dominance));
        } else if excused_set.contains(c.key.as_str())
            && excused_seen.insert(c.key.clone())
            && excused_samples.len() < 40
        {
            excused_samples.push(mk("excused", dom * rarity_abs(p.other, REF_K)));
        }
    }

    McCorpus {
        id,
        verses: map.len(),
        cased_tokens,
        other_tokens,
        other_types,
        hapax_other_types,
        recurring_other_types,
        corpus_dominance,
        cased_forced,
        other_forced,
        cased_mid,
        other_mid,
        other_first_upper,
        other_first_lower: other_tokens.saturating_sub(other_first_upper),
        flagged_first_upper,
        flagged_forced,
        grid_within,
        grid_fallback,
        hist_within,
        ref_within,
        ref_fallback,
        flagged_samples,
        excused_samples,
        fallback_samples,
    }
}

fn print_mc_grid(name: &str, grid: &[[u64; PACKET_FLOORS.len()]]) {
    println!("  [{name}] rows = floor, cols = k");
    print!("    {:>6}", "fl\\k");
    for k in PACKET_KS {
        print!("  {:>10}", format!("k={k:.0}"));
    }
    println!();
    for (fi, &fl) in PACKET_FLOORS.iter().enumerate() {
        print!("    {fl:>6.2}");
        for row in grid {
            print!("  {:>10}", row[fi]);
        }
        println!();
    }
}

fn print_mc_hist(hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!(
        "\nroute-A score histogram at ref knee (k=32) — {total} OtherMixed sites, 40 buckets:"
    );
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>8} {bar}", lo + 0.025);
    }
}

fn print_mc_samples(samples: &[&McSample]) {
    for s in samples {
        println!(
            "    {:<12} {:<9} [{}] {} {} dom={:.3} other={} wtot={} score={:.3} | {}",
            s.sid,
            s.route,
            s.word,
            if s.first_upper { "Up1" } else { "lo1" },
            if s.forced { "FORCED" } else { "mid" },
            s.dom,
            s.other,
            s.word_total,
            s.score,
            s.ctx,
        );
    }
}

fn mixedcase_single_report(c: &McCorpus) {
    println!(
        "=== mixed-case word SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    println!(
        "cased letter-run tokens: {}  |  OtherMixed tokens: {} ({:.4}%)  types: {} (hapax {}, recurring>=2 {})",
        c.cased_tokens,
        c.other_tokens,
        c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
        c.other_types,
        c.hapax_other_types,
        c.recurring_other_types,
    );
    println!(
        "corpus 'not-other-mixed' dominance (route-B factor): {:.4}",
        c.corpus_dominance
    );
    println!(
        "\nposition (route 4): OtherMixed rate  forced {:.4}% ({}/{})  vs  mid {:.4}% ({}/{})",
        c.other_forced as f64 * 100.0 / c.cased_forced.max(1) as f64,
        c.other_forced,
        c.cased_forced,
        c.other_mid as f64 * 100.0 / c.cased_mid.max(1) as f64,
        c.other_mid,
        c.cased_mid,
    );
    println!(
        "boundary vs casing v2: OtherMixed first-letter  upper {} (casing-invisible)  lower {} (overlaps casing)",
        c.other_first_upper, c.other_first_lower
    );
    println!(
        "\nreference cell (k=32, floor 0.95): route-A within {}  |  route-B hapax-fallback {}",
        c.ref_within, c.ref_fallback
    );
    println!(
        "  of ref-flagged route-A sites: first-upper {}  forced-position {}",
        c.flagged_first_upper, c.flagged_forced
    );
    println!("\n-- route-A (within-word) volume sweep --");
    print_mc_grid("within-word", &c.grid_within);
    println!("\n-- route-B (corpus hapax fallback) volume sweep --");
    print_mc_grid("hapax-fallback", &c.grid_fallback);
    print_mc_hist(&c.hist_within);

    let mut fs: Vec<&McSample> = c.flagged_samples.iter().collect();
    fs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    println!("\ntop route-A flagged samples:");
    print_mc_samples(&fs.iter().take(25).copied().collect::<Vec<_>>());
    println!("\nexcused recurring-convention samples (route A silent):");
    print_mc_samples(&c.excused_samples.iter().take(25).collect::<Vec<_>>());
    println!("\nroute-B hapax-fallback samples:");
    print_mc_samples(&c.fallback_samples.iter().take(25).collect::<Vec<_>>());
}

fn mixedcase_fleet(dir: &Path) {
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
    eprintln!("mixedcase fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<McCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let map = load_corpus(path);
            let c = analyze_mixedcase(id, &map);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("mixedcase fleet evaluate: {:?}", t0.elapsed());

    let nk = PACKET_KS.len();
    let mut grid_within = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut grid_fallback = vec![[0u64; PACKET_FLOORS.len()]; nk];
    let mut hist = [0u64; 40];
    let (mut ref_within, mut ref_fallback) = (0u64, 0u64);
    let (mut cased, mut other) = (0u64, 0u64);
    let (mut cf, mut of, mut cm, mut om) = (0u64, 0u64, 0u64, 0u64);
    let (mut fup, mut flo) = (0u64, 0u64);
    let (mut fl_up, mut fl_forced) = (0u64, 0u64);
    let mut corpora_with_ref = 0u32;
    for c in &corpora {
        for ki in 0..nk {
            for fi in 0..PACKET_FLOORS.len() {
                grid_within[ki][fi] += c.grid_within[ki][fi];
                grid_fallback[ki][fi] += c.grid_fallback[ki][fi];
            }
        }
        for (h, ch) in hist.iter_mut().zip(&c.hist_within) {
            *h += ch;
        }
        ref_within += c.ref_within;
        ref_fallback += c.ref_fallback;
        cased += c.cased_tokens;
        other += c.other_tokens;
        cf += c.cased_forced;
        of += c.other_forced;
        cm += c.cased_mid;
        om += c.other_mid;
        fup += c.other_first_upper;
        flo += c.other_first_lower;
        fl_up += c.flagged_first_upper;
        fl_forced += c.flagged_forced;
        if c.ref_within + c.ref_fallback > 0 {
            corpora_with_ref += 1;
        }
    }

    println!(
        "=== MIXED-CASE WORD SPIKE — fleet aggregate ({} corpora) ===",
        corpora.len()
    );
    println!(
        "\ncased letter-run tokens {cased}  |  OtherMixed {other} ({:.4}% of cased)",
        other as f64 * 100.0 / cased.max(1) as f64
    );
    println!(
        "\n-- reference cell (k=32, floor 0.95) --\n  route-A within {ref_within}  |  route-B hapax-fallback {ref_fallback}  across {corpora_with_ref} corpora"
    );
    println!(
        "  of ref-flagged route-A sites: first-letter-upper {fl_up} (casing-invisible), forced-position {fl_forced}"
    );

    println!("\n-- route 4 (position independence): OtherMixed rate forced vs mid --");
    println!(
        "  forced {:.4}% ({of}/{cf})   mid {:.4}% ({om}/{cm})   ratio forced/mid = {:.3}",
        of as f64 * 100.0 / cf.max(1) as f64,
        om as f64 * 100.0 / cm.max(1) as f64,
        (of as f64 / cf.max(1) as f64) / (om as f64 / cm.max(1) as f64).max(1e-12),
    );
    println!(
        "\n-- boundary vs casing v2: all OtherMixed by first-cased-letter --\n  first-upper {fup} (casing-invisible: casing fires only on lowercase word-starts)  first-lower {flo} (overlaps casing's lowercase-site domain)"
    );

    println!("\n-- route-A (within-word) volume sweep (surfaced OtherMixed sites) --");
    print_mc_grid("within-word", &grid_within);
    println!("\n-- route-B (corpus hapax fallback) volume sweep --");
    print_mc_grid("hapax-fallback", &grid_fallback);
    print_mc_hist(&hist);

    // Noisiest corpora by route-A ref volume, with storm diagnosis inputs.
    let mut ranked: Vec<&McCorpus> = corpora
        .iter()
        .filter(|c| c.ref_within + c.ref_fallback > 0)
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.ref_within + c.ref_fallback));
    println!("\n-- top-20 noisiest corpora (ref cell) --");
    println!(
        "  {:<20} {:>8} {:>8} {:>9} {:>10} {:>8}",
        "corpus", "withinA", "hapaxB", "other%", "corpusDom", "hapaxTy"
    );
    for c in ranked.iter().take(20) {
        println!(
            "  {:<20} {:>8} {:>8} {:>8.3}% {:>10.4} {:>8}",
            c.id,
            c.ref_within,
            c.ref_fallback,
            c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
            c.corpus_dominance,
            c.hapax_other_types,
        );
    }

    // Convention-class adjudication across major-language corpora.
    println!(
        "\n-- convention adjudication (major corpora): flagged (route A), excused (recurring), hapax (route B) --"
    );
    for c in &corpora {
        if !MC_MAJOR.contains(&c.id.as_str()) {
            continue;
        }
        println!(
            "\n[{}] other {} ({:.3}%) corpusDom {:.4} | ref withinA {} hapaxB {}",
            c.id,
            c.other_tokens,
            c.other_tokens as f64 * 100.0 / c.cased_tokens.max(1) as f64,
            c.corpus_dominance,
            c.ref_within,
            c.ref_fallback,
        );
        let mut fs: Vec<&McSample> = c.flagged_samples.iter().collect();
        fs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        if !fs.is_empty() {
            println!("  flagged (route A):");
            print_mc_samples(&fs.iter().take(4).copied().collect::<Vec<_>>());
        }
        if !c.excused_samples.is_empty() {
            println!("  excused (recurring convention):");
            print_mc_samples(&c.excused_samples.iter().take(4).collect::<Vec<_>>());
        }
        if !c.fallback_samples.is_empty() {
            println!("  hapax (route B only):");
            print_mc_samples(&c.fallback_samples.iter().take(4).collect::<Vec<_>>());
        }
    }
}

#[cfg(test)]
mod mixedcase_tests {
    use super::{McShape, mc_classify, mc_first_cased_upper};

    #[test]
    fn plain_shapes() {
        assert_eq!(mc_classify("word"), Some(McShape::Lower));
        assert_eq!(mc_classify("Word"), Some(McShape::Title));
        assert_eq!(mc_classify("WORD"), Some(McShape::AllCaps));
        assert_eq!(mc_classify("wOrd"), Some(McShape::OtherMixed));
    }

    #[test]
    fn single_letter_is_never_mixed() {
        assert_eq!(mc_classify("I"), Some(McShape::AllCaps));
        assert_eq!(mc_classify("a"), Some(McShape::Lower));
    }

    #[test]
    fn caseless_has_no_shape() {
        // Han / digits-only-ish: no cased letter ⇒ None (not a candidate).
        assert_eq!(mc_classify("好"), None);
        assert_eq!(mc_classify("1"), None);
    }

    #[test]
    fn convention_shapes_are_othermixed() {
        // McX names, class-prefix orthographies, inflected all-caps names.
        assert_eq!(mc_classify("McDonald"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("kiSwahili"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("iPhone"), Some(McShape::OtherMixed));
        assert_eq!(mc_classify("LORDs"), Some(McShape::OtherMixed));
        // Pure ALLCAPS YHWH stays AllCaps — not a mixed candidate at all.
        assert_eq!(mc_classify("LORD"), Some(McShape::AllCaps));
    }

    #[test]
    fn combining_marks_and_caseless_do_not_manufacture_mixing() {
        // Base + combining acute (decomposed é): still Lower.
        assert_eq!(mc_classify("cafe\u{0301}"), Some(McShape::Lower));
        // Title with a trailing combining mark stays Title.
        assert_eq!(mc_classify("A\u{0301}bc"), Some(McShape::Title));
    }

    #[test]
    fn first_cased_axis() {
        assert!(mc_first_cased_upper("McDonald"));
        assert!(mc_first_cased_upper("LORDs"));
        assert!(!mc_first_cased_upper("wOrd"));
        assert!(!mc_first_cased_upper("kiSwahili"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Pooled class-conditioned spacing SPIKE (plan rule 2 amendment, 2026-07-10).
// Measurement only — nothing frozen, production `punctuation.rs` untouched;
// every symbol here is harness-local (`pool_*` / `PClass` / `BCat` / `Pool*`).
//
// Two designs measured head-to-head over the SAME sites at the shipped ADR
// 0050/0054 reference constants (z 1.96, knee k=32 + 40/10k on the pool, floor
// 0.5 — the production `side_verdict` shape):
//
//   Design A (class-conditioned binary). The typist chooses the SPACE, not the
//   neighbour: condition on content, judge the choice. Per (mark, side, class)
//   a binary attached-vs-spaced, where the class is the fused-Class of the
//   FIRST non-whitespace neighbour on that side {Letter, Number, Punct} —
//   crossing verse (and book) seams to reach the next/prev verse's edge
//   grapheme (book-ordered), the seam reading as an ordinary SPACED observation
//   (no forcedness, repo CLAUDE.md). Quote is MERGED into Punct in the model; a
//   quote/non-quote sub-split is tracked inside Punct purely as data. A site is
//   judged by its most specific pool that holds a Wilson-dominant convention
//   (class pool → top-level all-class fallback); Wilson self-gates thin pools.
//
//   Design B (immediate four-way category). The side reads its IMMEDIATE
//   context {letter, number, ws, punct} — whitespace is terminal, never looked
//   past. Verdict per (mark, side): mode-dominance (Wilson lower bound of the
//   modal category's share) × recurrence on the observed category's count; flag
//   non-modal occurrences above floor.
//
// A separately-reported Pd lane (dashes) rides both designs. The report ends
// with a head-to-head verdict table.
// ═══════════════════════════════════════════════════════════════════════════

const POOL_Z: f64 = 1.96;
const POOL_K: f64 = 32.0;
const POOL_RATE: f64 = 40.0;
const POOL_FLOOR: f64 = 0.5;

/// Design-A conditioning classes (Quote MERGED into Punct in the model; the
/// quote sub-split lives inside Punct and is reported as data only).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum PClass {
    Letter,
    Number,
    Punct,
}
impl PClass {
    const ALL: [Self; 3] = [Self::Letter, Self::Number, Self::Punct];
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Number => "number",
            Self::Punct => "punct",
        }
    }
}

/// Internal neighbour sub-class: the four buckets accumulated per side. The
/// model reads {Letter, Number, Punct=Quote+OtherPunct}; Quote is kept distinct
/// only for the sub-split census.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum SubClass {
    Letter,
    Number,
    Quote,
    OtherPunct,
}
impl SubClass {
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Quote => 2,
            Self::OtherPunct => 3,
        }
    }
    const fn pclass(self) -> PClass {
        match self {
            Self::Letter => PClass::Letter,
            Self::Number => PClass::Number,
            Self::Quote | Self::OtherPunct => PClass::Punct,
        }
    }
}

/// Design-B immediate category {letter, number, ws, punct} (quote⊆punct).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum BCat {
    Letter,
    Number,
    Ws,
    Punct,
}
impl BCat {
    const fn index(self) -> usize {
        match self {
            Self::Letter => 0,
            Self::Number => 1,
            Self::Ws => 2,
            Self::Punct => 3,
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Letter => "letter",
            Self::Number => "number",
            Self::Ws => "ws",
            Self::Punct => "punct",
        }
    }
}

/// Per-mark Design-A cells: `[side][subclass][bit]` — side 0=left/1=right,
/// subclass 0..4, bit 0=attached/1=spaced.
type ACell = [[[u64; 2]; 4]; 2];
/// Per-mark Design-B cells: `[side][category]`.
type BCell = [[u64; 4]; 2];

/// The two-factor pool score (production `side_verdict` shape): dominance of the
/// pool's OTHER form (a binary's complement is its majority) × volume-scaled
/// recurrence rarity of this form's own count.
fn pool_score(count: u64, n: u64) -> f64 {
    if n == 0 || count == 0 {
        return 0.0;
    }
    let knee = POOL_K + POOL_RATE * n as f64 / 10_000.0;
    let dominance = sig_wilson_lb(n.saturating_sub(count), n, POOL_Z);
    let recurrence = (count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
    dominance * (1.0 - recurrence)
}

/// Design-B occurrence score: mode-dominance × rarity of the observed category.
fn bcat_score(modal_count: u64, cat_count: u64, n: u64) -> f64 {
    if n == 0 || cat_count == 0 {
        return 0.0;
    }
    let knee = POOL_K + POOL_RATE * n as f64 / 10_000.0;
    let dominance = sig_wilson_lb(modal_count, n, POOL_Z);
    let recurrence = (cat_count.saturating_sub(1) as f64 / knee).clamp(0.0, 1.0);
    dominance * (1.0 - recurrence)
}

/// A pool holds a convention iff its majority form is Wilson-dominant at the
/// floor confidence — the "the other convention genuinely holds the field"
/// gate. Thin pools fail it automatically (Wilson self-gating, no min-samples).
fn pool_holds_convention(a: u64, b: u64) -> bool {
    let n = a + b;
    n > 0 && sig_wilson_lb(a.max(b), n, POOL_Z) >= POOL_FLOOR
}

/// The live spacing rule's candidate class: GC `Po` minus quotes (ADR 0033).
fn pool_is_separator(c: char) -> bool {
    ssc_core::unicode::is_other_punctuation(c) && !class_of(c).is_quote()
}

/// A pragmatic GC `Pd` (dash-punctuation) set for the separately-reported dash
/// lane — the fused Class table carries no `Pd` bit, so this spike enumerates
/// the dashes that actually occur in scripture corpora (ASCII/Unicode hyphens &
/// dashes, fullwidth, Armenian/Hebrew/Mongolian/Canadian). Measurement-only.
fn pool_is_dash(c: char) -> bool {
    matches!(
        c,
        '-' | '\u{2010}'
            | '\u{2011}'
            | '\u{2012}'
            | '\u{2013}'
            | '\u{2014}'
            | '\u{2015}'
            | '\u{FE58}'
            | '\u{FE63}'
            | '\u{FF0D}'
            | '\u{058A}'
            | '\u{05BE}'
            | '\u{1400}'
            | '\u{1806}'
            | '\u{2E17}'
            | '\u{301C}'
            | '\u{30A0}'
    )
}

/// Classify a non-whitespace neighbour cluster into a Design-A sub-class.
fn subclass_of(cluster: &str) -> SubClass {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return SubClass::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_quote() => SubClass::Quote,
        Some(c) if class_of(c).is_numeric() => SubClass::Number,
        _ => SubClass::OtherPunct,
    }
}

/// Classify an immediate non-whitespace neighbour cluster into a Design-B
/// category (quote⊆punct).
fn bcat_of(cluster: &str) -> BCat {
    if cluster.chars().any(|c| class_of(c).is_alphabetic()) {
        return BCat::Letter;
    }
    match cluster.chars().next() {
        Some(c) if class_of(c).is_numeric() && !class_of(c).is_quote() => BCat::Number,
        _ => BCat::Punct,
    }
}

/// First / last non-whitespace grapheme sub-classes of a verse — the edge
/// grapheme a neighbouring verse's mark reaches across the seam.
fn verse_edge_subclasses(text: &str) -> (Option<SubClass>, Option<SubClass>) {
    let mut g = Vec::new();
    ssc_core::grapheme::segment(text, &mut g);
    let nonws = |gs: &ssc_core::grapheme::GSpan| {
        let s = gs.slice(text);
        (!s.is_empty() && !s.chars().all(sig_is_spacing_ws)).then(|| subclass_of(s))
    };
    let first = g.iter().find_map(nonws);
    let last = g.iter().rev().find_map(nonws);
    (first, last)
}

/// One separator/dash occurrence with both designs' per-side reads.
struct PoolOpp {
    mark: char,
    is_dash: bool,
    /// Design A left/right: `Some((attached, subclass))`, `None` = no neighbour
    /// (a book edge whose seam-cross found nothing).
    a_left: Option<(bool, SubClass)>,
    a_right: Option<(bool, SubClass)>,
    /// Design B immediate category per side (seam ⇒ `Ws`).
    b_left: BCat,
    b_right: BCat,
    mark_off: usize,
}

/// Extract every separator/dash occurrence's per-side reads from one verse,
/// given the sub-classes reachable across the left/right seams (from the
/// book-ordered neighbour verses).
fn pool_opps(
    text: &str,
    graphemes: &[ssc_core::grapheme::GSpan],
    left_cross: Option<SubClass>,
    right_cross: Option<SubClass>,
) -> Vec<PoolOpp> {
    let mut out = Vec::new();
    let all_ws = |gs: &ssc_core::grapheme::GSpan| {
        let s = gs.slice(text);
        !s.is_empty() && s.chars().all(sig_is_spacing_ws)
    };
    for (idx, gs) in graphemes.iter().enumerate() {
        let g = gs.slice(text);
        let (mark, is_dash) = match g.chars().next() {
            Some(c) if g.len() == c.len_utf8() && pool_is_separator(c) => (c, false),
            Some(c) if g.len() == c.len_utf8() && pool_is_dash(c) => (c, true),
            _ => continue,
        };

        // Design A left: walk over horizontal whitespace to the neighbour.
        let mut j = idx;
        let mut left_ws = false;
        while j > 0 && all_ws(&graphemes[j - 1]) {
            left_ws = true;
            j -= 1;
        }
        let a_left = if j == 0 {
            left_cross.map(|sc| (false, sc)) // seam ⇒ spaced; class across the seam
        } else {
            Some((!left_ws, subclass_of(graphemes[j - 1].slice(text))))
        };
        // Design A right: the mirror.
        let mut k = idx;
        let mut right_ws = false;
        while k + 1 < graphemes.len() && all_ws(&graphemes[k + 1]) {
            right_ws = true;
            k += 1;
        }
        let a_right = if k + 1 >= graphemes.len() {
            right_cross.map(|sc| (false, sc))
        } else {
            Some((!right_ws, subclass_of(graphemes[k + 1].slice(text))))
        };

        // Design B: the immediate grapheme only (whitespace/seam ⇒ Ws).
        let b_left = if idx == 0 || all_ws(&graphemes[idx - 1]) {
            BCat::Ws
        } else {
            bcat_of(graphemes[idx - 1].slice(text))
        };
        let b_right = if idx + 1 >= graphemes.len() || all_ws(&graphemes[idx + 1]) {
            BCat::Ws
        } else {
            bcat_of(graphemes[idx + 1].slice(text))
        };

        out.push(PoolOpp {
            mark,
            is_dash,
            a_left,
            a_right,
            b_left,
            b_right,
            mark_off: gs.start as usize,
        });
    }
    out
}

/// Iterate every occurrence in book-reading order, resolving each verse's
/// seam-cross classes from its book neighbours (skipping empty/all-ws verses).
fn for_each_pool_opp(map: &Corpus, mut f: impl FnMut(&str, &str, &PoolOpp)) {
    // Group the corpus (already in book-contiguous order) into book-ordered
    // verse runs.
    let mut graphemes = Vec::new();
    for group in &ssc_core::corpus::by_book(map) {
        let edges: Vec<(Option<SubClass>, Option<SubClass>)> = group
            .texts
            .iter()
            .map(|t| verse_edge_subclasses(t))
            .collect();
        for (vi, (key, text)) in group.keys.iter().zip(group.texts).enumerate() {
            let left_cross = (0..vi).rev().find_map(|jj| edges[jj].1);
            let right_cross = (vi + 1..group.texts.len()).find_map(|jj| edges[jj].0);
            graphemes.clear();
            ssc_core::grapheme::segment(text, &mut graphemes);
            for opp in pool_opps(text, &graphemes, left_cross, right_cross) {
                f(key, text, &opp);
            }
        }
    }
}

fn a_class_counts(cell: &ACell, side: usize, cls: PClass) -> [u64; 2] {
    match cls {
        PClass::Letter => cell[side][0],
        PClass::Number => cell[side][1],
        PClass::Punct => [
            cell[side][2][0] + cell[side][3][0],
            cell[side][2][1] + cell[side][3][1],
        ],
    }
}
fn a_top_counts(cell: &ACell, side: usize) -> [u64; 2] {
    let mut r = [0u64; 2];
    for sub in &cell[side] {
        r[0] += sub[0];
        r[1] += sub[1];
    }
    r
}

/// One side's resolved Design-A verdict.
struct ASide {
    flagged: bool,
    score: f64,
    used_top: bool,
    cls: PClass,
    sub: SubClass,
    bit: usize, // 0 attached, 1 spaced
    class_flag: bool,
    top_flag: bool,
    class_holds: bool,
}

fn eval_a_side(cell: &ACell, side: usize, s: Option<(bool, SubClass)>) -> Option<ASide> {
    let (att, sub) = s?;
    let cls = sub.pclass();
    let bit = usize::from(!att);
    let cc = a_class_counts(cell, side, cls);
    let n_cls = cc[0] + cc[1];
    let class_holds = pool_holds_convention(cc[0], cc[1]);
    let class_score = pool_score(cc[bit], n_cls);
    let class_flag = class_holds && class_score >= POOL_FLOOR;
    let top = a_top_counts(cell, side);
    let n_top = top[0] + top[1];
    let top_holds = pool_holds_convention(top[0], top[1]);
    let top_score = pool_score(top[bit], n_top);
    let top_flag = top_holds && top_score >= POOL_FLOOR;
    let (flagged, score, used_top) = if class_holds {
        (class_flag, class_score, false)
    } else {
        (top_flag, top_score, true)
    };
    Some(ASide {
        flagged,
        score,
        used_top,
        cls,
        sub,
        bit,
        class_flag,
        top_flag,
        class_holds,
    })
}

/// One side's resolved Design-B verdict.
struct BSide {
    flagged: bool,
    score: f64,
    cat: BCat,
    count: u64,
    total: u64,
}
fn eval_b_side(cell: &BCell, side: usize, cat: BCat) -> BSide {
    let counts = cell[side];
    let n: u64 = counts.iter().sum();
    let (modal_idx, &modal_count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, &c)| c)
        .unwrap_or((0, &0));
    let ci = cat.index();
    let score = bcat_score(modal_count, counts[ci], n);
    let flagged = ci != modal_idx && score >= POOL_FLOOR;
    BSide {
        flagged,
        score,
        cat,
        count: counts[ci],
        total: n,
    }
}

#[derive(Clone, Default)]
struct ALevelTally {
    letter: u64,
    number: u64,
    punct_quote: u64,
    punct_other: u64,
    top: u64,
}
impl ALevelTally {
    fn add(&mut self, o: &ALevelTally) {
        self.letter += o.letter;
        self.number += o.number;
        self.punct_quote += o.punct_quote;
        self.punct_other += o.punct_other;
        self.top += o.top;
    }
    fn total(&self) -> u64 {
        self.letter + self.number + self.punct_quote + self.punct_other + self.top
    }
}

#[derive(Clone)]
struct PoolSample {
    corpus: String,
    sid: String,
    mark: char,
    side: char,
    label: String,
    count: u64,
    total: u64,
    score: f64,
    ctx: String,
}
fn pool_push(v: &mut Vec<PoolSample>, s: PoolSample, cap: usize) {
    if v.len() < cap {
        v.push(s);
    } else if let Some((i, min)) = v
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap())
        && s.score > min.score
    {
        v[i] = s;
    }
}

const POOL_FOCUS_MARKS: &[char] = SIG_FOCUS_MARKS;
const POOL_SAMPLE_CAP: usize = 14;

/// The six ADR 0050/0054 regression corpora (file stem, short id).
const POOL_REGRESSION: &[(&str, &str)] = &[
    ("engwebster", "engwebster"),
    ("WA-kmr-IQ-badini-reg", "kmr-IQ"),
    ("udu", "udu"),
    ("WA-ne-udb", "ne_udb"),
    ("WA-pa-ulb", "pa_ulb"),
    ("mya", "mya"),
];

struct PoolCorpus {
    id: String,
    verses: usize,
    total_scalars: u64,
    digit_scalars: u64,
    a_po: BTreeMap<char, ACell>,
    a_pd: BTreeMap<char, ACell>,
    b_po: BTreeMap<char, BCell>,
    b_pd: BTreeMap<char, BCell>,
    shipped_findings: u64,
    a_findings: u64,
    b_findings: u64,
    a_pd_findings: u64,
    b_pd_findings: u64,
    a_level: ALevelTally,
    b_cat_flags: [u64; 4],
    disagreements: u64,
    double_flags: u64,
    no_neighbour: u64,
    a_hist: [u64; 40],
    b_hist: [u64; 40],
    number_has_conv: bool,
    quote_has_conv: bool,
    number_flag_sites: u64,
    quote_flag_sites: u64,
    new_digit: Vec<PoolSample>,
    new_quote: Vec<PoolSample>,
    new_medial: Vec<PoolSample>,
    new_pd: Vec<PoolSample>,
    pred_a: Vec<PoolSample>,
    pred_b: Vec<PoolSample>,
    a_samples: Vec<PoolSample>,
}

fn analyze_pooled(id: String, map: &Corpus) -> PoolCorpus {
    let mut total_scalars = 0u64;
    let mut digit_scalars = 0u64;
    for text in map.texts() {
        for c in text.chars() {
            total_scalars += 1;
            if class_of(c).is_numeric() {
                digit_scalars += 1;
            }
        }
    }

    // Pass 1 — accumulate pools.
    let mut a_po: BTreeMap<char, ACell> = BTreeMap::new();
    let mut a_pd: BTreeMap<char, ACell> = BTreeMap::new();
    let mut b_po: BTreeMap<char, BCell> = BTreeMap::new();
    let mut b_pd: BTreeMap<char, BCell> = BTreeMap::new();
    for_each_pool_opp(map, |_sid, _text, opp| {
        let (am, bm) = if opp.is_dash {
            (&mut a_pd, &mut b_pd)
        } else {
            (&mut a_po, &mut b_po)
        };
        let ac = am.entry(opp.mark).or_insert([[[0u64; 2]; 4]; 2]);
        if let Some((att, sub)) = opp.a_left {
            ac[0][sub.index()][usize::from(!att)] += 1;
        }
        if let Some((att, sub)) = opp.a_right {
            ac[1][sub.index()][usize::from(!att)] += 1;
        }
        let bc = bm.entry(opp.mark).or_insert([[0u64; 4]; 2]);
        bc[0][opp.b_left.index()] += 1;
        bc[1][opp.b_right.index()] += 1;
    });

    // Make-or-break: does any Po mark hold a Wilson-dominant convention in its
    // Number pool / Quote sub-pool (either side)?
    let mut number_has_conv = false;
    let mut quote_has_conv = false;
    for cell in a_po.values() {
        for side in 0..2 {
            let num = a_class_counts(cell, side, PClass::Number);
            if pool_holds_convention(num[0], num[1]) {
                number_has_conv = true;
            }
            let q = cell[side][SubClass::Quote.index()];
            if pool_holds_convention(q[0], q[1]) {
                quote_has_conv = true;
            }
        }
    }

    // Shipped production rule at the reference constants (its default config).
    let books = ssc_core::corpus::by_book(map);
    let shipped_rule = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig::default(),
    };
    let shipped_findings = shipped_rule
        .judge(
            &shipped_rule.reduce(&books, None, None).0,
            &books,
            None,
            None,
        )
        .len() as u64;

    // Pass 2 — evaluate each site under both designs.
    let mut a_findings = 0u64;
    let mut b_findings = 0u64;
    let mut a_pd_findings = 0u64;
    let mut b_pd_findings = 0u64;
    let mut a_level = ALevelTally::default();
    let mut b_cat_flags = [0u64; 4];
    let mut disagreements = 0u64;
    let mut double_flags = 0u64;
    let mut no_neighbour = 0u64;
    let mut a_hist = [0u64; 40];
    let mut b_hist = [0u64; 40];
    let mut number_flag_sites = 0u64;
    let mut quote_flag_sites = 0u64;
    let mut new_digit = Vec::new();
    let mut new_quote = Vec::new();
    let mut new_medial = Vec::new();
    let mut new_pd = Vec::new();
    let mut pred_a = Vec::new();
    let mut pred_b = Vec::new();
    let mut a_samples = Vec::new();

    for_each_pool_opp(map, |sid, text, opp| {
        let (am, bm) = if opp.is_dash {
            (&a_pd, &b_pd)
        } else {
            (&a_po, &b_po)
        };
        let acell = &am[&opp.mark];
        let bcell = &bm[&opp.mark];
        let al = eval_a_side(acell, 0, opp.a_left);
        let ar = eval_a_side(acell, 1, opp.a_right);
        let bl = eval_b_side(bcell, 0, opp.b_left);
        let br = eval_b_side(bcell, 1, opp.b_right);

        no_neighbour += u64::from(opp.a_left.is_none()) + u64::from(opp.a_right.is_none());

        let a_hit =
            al.as_ref().is_some_and(|s| s.flagged) || ar.as_ref().is_some_and(|s| s.flagged);
        let b_hit = bl.flagged || br.flagged;
        if opp.is_dash {
            a_pd_findings += u64::from(a_hit);
            b_pd_findings += u64::from(b_hit);
        } else {
            a_findings += u64::from(a_hit);
            b_findings += u64::from(b_hit);
        }
        if b_hit {
            for bs in [&bl, &br] {
                if bs.flagged {
                    b_cat_flags[bs.cat.index()] += 1;
                }
            }
        }

        let make = |side: char, label: String, count: u64, total: u64, score: f64| PoolSample {
            corpus: id.clone(),
            sid: sid.to_string(),
            mark: opp.mark,
            side,
            label,
            count,
            total,
            score,
            ctx: sig_context(text, opp.mark_off, opp.mark_off + opp.mark.len_utf8()),
        };

        // Design-A side telemetry, samples, hierarchy.
        for (side_idx, side_ch, aside, bside) in [(0usize, 'L', &al, &bl), (1usize, 'R', &ar, &br)]
        {
            let Some(a) = aside else { continue };
            a_hist[sig_bucket(a.score)] += 1;
            if a.class_holds && a.class_flag != a.top_flag {
                disagreements += 1;
            }
            if a.class_flag && a.top_flag {
                double_flags += 1;
            }
            if !a.flagged {
                // Design-B-only flag on a side Design A leaves silent: the
                // rare-content prediction (b). Attached content the thin A pool
                // can't judge.
                if !opp.is_dash && bside.flagged && matches!(bside.cat, BCat::Number | BCat::Punct)
                {
                    pool_push(
                        &mut pred_b,
                        make(
                            side_ch,
                            format!("B:cat={} (A silent)", bside.cat.label()),
                            bside.count,
                            bside.total,
                            bside.score,
                        ),
                        POOL_SAMPLE_CAP,
                    );
                }
                continue;
            }
            // A flagged this side.
            let cc = a_class_counts(acell, side_idx, a.cls);
            let n_cls = cc[0] + cc[1];
            let top = a_top_counts(acell, side_idx);
            let n_top = top[0] + top[1];
            let (count, total) = if a.used_top {
                (top[a.bit], n_top)
            } else {
                (cc[a.bit], n_cls)
            };
            let form = if a.bit == 0 { "attached" } else { "spaced" };
            let lvl = if a.used_top { "top" } else { a.cls.label() };
            let label = format!("A:{lvl}/{form}");
            let s = make(side_ch, label.clone(), count, total, a.score);
            pool_push(&mut a_samples, s.clone(), POOL_SAMPLE_CAP);

            // Level attribution + make-or-break coverage.
            if a.used_top {
                a_level.top += 1;
            } else {
                match a.cls {
                    PClass::Letter => a_level.letter += 1,
                    PClass::Number => {
                        a_level.number += 1;
                        number_flag_sites += 1;
                    }
                    PClass::Punct => {
                        if a.sub == SubClass::Quote {
                            a_level.punct_quote += 1;
                            quote_flag_sites += 1;
                        } else {
                            a_level.punct_other += 1;
                        }
                    }
                }
            }

            // New-coverage sample classes.
            if opp.is_dash {
                pool_push(&mut new_pd, s.clone(), POOL_SAMPLE_CAP);
            } else {
                if a.cls == PClass::Number {
                    pool_push(&mut new_digit, s.clone(), POOL_SAMPLE_CAP);
                }
                if a.sub == SubClass::Quote {
                    pool_push(&mut new_quote, s.clone(), POOL_SAMPLE_CAP);
                }
                if opp.mark == '.' && a.cls == PClass::Letter && a.bit == 0 {
                    pool_push(&mut new_medial, s.clone(), POOL_SAMPLE_CAP);
                }
            }

            // Prediction (a): A flags a SPACED side conditioned on content
            // (Number/Punct); Design B is structurally blind (its immediate
            // read is Ws whenever A is spaced).
            if !opp.is_dash && a.bit == 1 && a.cls != PClass::Letter && bside.cat == BCat::Ws {
                pool_push(
                    &mut pred_a,
                    make(
                        side_ch,
                        format!("A:{}/spaced (B blind=ws)", a.cls.label()),
                        count,
                        total,
                        a.score,
                    ),
                    POOL_SAMPLE_CAP,
                );
            }
        }

        // Design-B histogram (per side).
        for bs in [&bl, &br] {
            b_hist[sig_bucket(bs.score)] += 1;
        }
    });

    PoolCorpus {
        id,
        verses: map.len(),
        total_scalars,
        digit_scalars,
        a_po,
        a_pd,
        b_po,
        b_pd,
        shipped_findings,
        a_findings,
        b_findings,
        a_pd_findings,
        b_pd_findings,
        a_level,
        b_cat_flags,
        disagreements,
        double_flags,
        no_neighbour,
        a_hist,
        b_hist,
        number_has_conv,
        quote_has_conv,
        number_flag_sites,
        quote_flag_sites,
        new_digit,
        new_quote,
        new_medial,
        new_pd,
        pred_a,
        pred_b,
        a_samples,
    }
}

fn pool_dominant(counts: [u64; 2]) -> (&'static str, f64, u64) {
    let n = counts[0] + counts[1];
    if n == 0 {
        return ("—", 0.0, 0);
    }
    if counts[0] >= counts[1] {
        ("attached", counts[0] as f64 * 100.0 / n as f64, n)
    } else {
        ("spaced", counts[1] as f64 * 100.0 / n as f64, n)
    }
}

fn print_pool_samples(samples: &[PoolSample]) {
    for s in samples {
        println!(
            "  {:<22} {:<11} {:?} {} {:<26} count={:<5} N={:<7} score={:.3} | {}",
            s.corpus, s.sid, s.mark, s.side, s.label, s.count, s.total, s.score, s.ctx,
        );
    }
}

fn print_pool_hist(name: &str, hist: &[u64; 40]) {
    let total: u64 = hist.iter().sum();
    println!("\n{name} score histogram over site-sides ({total} sides):");
    for (i, &n) in hist.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let lo = i as f64 / 40.0;
        let bar = "#".repeat((n as f64).sqrt() as usize);
        println!("  [{lo:.3},{:.3}) {n:>9} {bar}", lo + 0.025);
    }
}

/// Per-mark Design-A per-side per-class census line (with the Punct quote
/// sub-split reported as data).
fn print_pool_census(mark: char, cell: &ACell) {
    let n_total: u64 = cell.iter().flatten().flatten().sum();
    if n_total == 0 {
        return;
    }
    print!("  {mark:?} U+{:04X} N={n_total:<7}", mark as u32);
    for (side, tag) in [(0usize, 'L'), (1usize, 'R')] {
        for cls in PClass::ALL {
            let cc = a_class_counts(cell, side, cls);
            let (form, share, n) = pool_dominant(cc);
            if n == 0 {
                continue;
            }
            let conv = if pool_holds_convention(cc[0], cc[1]) {
                "*"
            } else {
                " "
            };
            print!(" | {tag}.{}={n}:{form}{share:.0}%{conv}", cls.label());
        }
    }
    // Punct quote sub-split (data only).
    for (side, tag) in [(0usize, 'L'), (1usize, 'R')] {
        let q = cell[side][SubClass::Quote.index()];
        let o = cell[side][SubClass::OtherPunct.index()];
        if q[0] + q[1] + o[0] + o[1] == 0 {
            continue;
        }
        let (qf, qs, qn) = pool_dominant(q);
        let (of, os, on) = pool_dominant(o);
        print!(" || {tag}.punct[quote {qn}:{qf}{qs:.0}% / other {on}:{of}{os:.0}%]");
    }
    println!();
}

fn pooled_single_report(c: &PoolCorpus) {
    println!(
        "=== POOLED-SPACING SPIKE: {} ({} verses) ===",
        c.id, c.verses
    );
    let po_occ: u64 = c.a_po.values().flatten().flatten().flatten().sum();
    let pd_occ: u64 = c.a_pd.values().flatten().flatten().flatten().sum();
    println!(
        "Po-separator side-observations: {po_occ}  Pd-dash: {pd_occ}  digit share of scalars: {:.3}%  no-neighbour sides: {}",
        c.digit_scalars as f64 * 100.0 / c.total_scalars.max(1) as f64,
        c.no_neighbour,
    );
    println!(
        "\n-- per-mark per-side per-class census (Design A; * = Wilson-dominant convention) --"
    );
    let mut order: Vec<(&char, &ACell)> = c.a_po.iter().collect();
    order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().flatten().flatten().sum::<u64>()));
    for (mark, cell) in order.iter().take(14) {
        print_pool_census(**mark, cell);
    }
    println!(
        "\nfindings @ ref (k=32,rate=40,floor0.5,z1.96):  shipped {}  Design A {}  Design B {}",
        c.shipped_findings, c.a_findings, c.b_findings
    );
    println!(
        "Design A level attribution: letter {} number {} punct(quote {}, other {}) top-fallback {}",
        c.a_level.letter,
        c.a_level.number,
        c.a_level.punct_quote,
        c.a_level.punct_other,
        c.a_level.top
    );
    println!(
        "hierarchy: class-vs-top disagreements {}  double-flags {}",
        c.disagreements, c.double_flags
    );
    println!(
        "Pd-lane findings: Design A {}  Design B {}",
        c.a_pd_findings, c.b_pd_findings
    );
    print_pool_hist("Design A", &c.a_hist);
    print_pool_hist("Design B", &c.b_hist);
    let sorted = |v: &[PoolSample]| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        s
    };
    println!("\n-- Design A top surfaced --");
    print_pool_samples(&sorted(&c.a_samples));
    println!("\n-- new-coverage: digit pools (`7. 800`) --");
    print_pool_samples(&sorted(&c.new_digit));
    println!("\n-- new-coverage: quote-adjacent (`word .\"`) --");
    print_pool_samples(&sorted(&c.new_quote));
    println!("\n-- new-coverage: medial periods (`word.word`) --");
    print_pool_samples(&sorted(&c.new_medial));
    println!("\n-- new-coverage: Pd dashes --");
    print_pool_samples(&sorted(&c.new_pd));
    println!("\n-- disagreement (a): A flags spaced-content, B blind --");
    print_pool_samples(&sorted(&c.pred_a));
    println!("\n-- disagreement (b): B flags rare-content, A silent --");
    print_pool_samples(&sorted(&c.pred_b));
    if POOL_REGRESSION.iter().any(|&(f, _)| f == c.id) {
        println!("\n-- regression vs shipped rule --");
        pooled_regression(&c.id);
    }
}

/// Regression: for the sites the shipped `punct.spacing-anomaly` surfaces
/// today, what do Design A (its Letter pool, and its operational verdict) and
/// Design B say? Reloads the corpus, runs the production rule, joins by
/// (sid, mark byte-offset, side).
fn pooled_regression(id: &str) {
    use std::collections::HashMap;

    let path = Path::new("corpora/vref").join(format!("{id}.txt"));
    let map = load_corpus(&path);
    if map.is_empty() {
        println!("  {id}: (no corpus file)");
        return;
    }
    let books = ssc_core::corpus::by_book(&map);
    let live = PunctuationSpacingAnomaly {
        cfg: PunctuationSpacingConfig {
            emit_score_min: 0.0,
            ..Default::default()
        },
    };
    let live_floor = f64::from(PunctuationSpacingConfig::default().emit_score_min);
    let findings = live.judge(&live.reduce(&books, None, None).0, &books, None, None);

    // Build the pools + a (key, mark_off) → opp reads lookup.
    let mut a_po: BTreeMap<char, ACell> = BTreeMap::new();
    let mut b_po: BTreeMap<char, BCell> = BTreeMap::new();
    type OppRead = (
        Option<(bool, SubClass)>,
        Option<(bool, SubClass)>,
        BCat,
        BCat,
    );
    let mut reads: HashMap<(String, usize), OppRead> = HashMap::new();
    for_each_pool_opp(&map, |key, _text, opp| {
        if opp.is_dash {
            return;
        }
        let ac = a_po.entry(opp.mark).or_insert([[[0u64; 2]; 4]; 2]);
        if let Some((att, sub)) = opp.a_left {
            ac[0][sub.index()][usize::from(!att)] += 1;
        }
        if let Some((att, sub)) = opp.a_right {
            ac[1][sub.index()][usize::from(!att)] += 1;
        }
        let bc = b_po.entry(opp.mark).or_insert([[0u64; 4]; 2]);
        bc[0][opp.b_left.index()] += 1;
        bc[1][opp.b_right.index()] += 1;
        reads.insert(
            (key.to_string(), opp.mark_off),
            (opp.a_left, opp.a_right, opp.b_left, opp.b_right),
        );
    });

    let mut shipped = 0u64;
    let (mut a_op_keep, mut a_letter_keep, mut b_keep) = (0u64, 0u64, 0u64);
    let mut changed: Vec<String> = Vec::new();
    for f in &findings {
        let Some(FindingArgs::SpacingConvention { mark, left, right }) = &f.args else {
            continue;
        };
        if f.score.unwrap_or(0.0) as f64 <= 0.0 || (f.score.unwrap_or(0.0) as f64) < live_floor {
            continue;
        }
        shipped += 1;
        let mark = *mark;
        let key = map.key(f.key_idx);
        let text = map.text(f.key_idx);
        let mark_off = text[f.range.start as usize..f.range.end as usize]
            .find(mark)
            .map(|rel| f.range.start as usize + rel);
        let Some((al, ar, blc, brc)) =
            mark_off.and_then(|o| reads.get(&(key.to_string(), o)).copied())
        else {
            changed.push(format!("    {:<10} {:?} (no opp match)", key, mark));
            continue;
        };
        let acell = &a_po[&mark];
        let bcell = &b_po[&mark];
        // Which side(s) did shipped flag?
        type SideRead = (bool, usize, Option<(bool, SubClass)>, BCat);
        let sides: [SideRead; 2] = [(left.is_some(), 0, al, blc), (right.is_some(), 1, ar, brc)];
        let mut op = false;
        let mut lp = false;
        let mut bp = false;
        for (shipped_side, side_idx, aread, bcat) in sides {
            if !shipped_side {
                continue;
            }
            if let Some(a) = eval_a_side(acell, side_idx, aread) {
                op |= a.flagged;
                // Letter-pool-specific verdict (the "Letter pool reproduces
                // shipped" claim): only meaningful when the class IS Letter.
                if a.cls == PClass::Letter {
                    let cc = a_class_counts(acell, side_idx, PClass::Letter);
                    lp |= pool_holds_convention(cc[0], cc[1])
                        && pool_score(cc[a.bit], cc[0] + cc[1]) >= POOL_FLOOR;
                }
            }
            bp |= eval_b_side(bcell, side_idx, bcat).flagged;
        }
        a_op_keep += u64::from(op);
        a_letter_keep += u64::from(lp);
        b_keep += u64::from(bp);
        if (!op || !lp) && changed.len() < 12 {
            changed.push(format!(
                "    {:<10} {:?} shipped→ A-op {} A-letter {} B {}",
                key,
                mark,
                if op { "kept" } else { "DROP" },
                if lp { "kept" } else { "drop" },
                if bp { "kept" } else { "drop" },
            ));
        }
    }
    println!(
        "  {id}: shipped {shipped} → A-operational keeps {a_op_keep}, A-Letter-pool keeps {a_letter_keep}, B keeps {b_keep}"
    );
    for r in &changed {
        println!("{r}");
    }
}

fn pooled_fleet(dir: &Path) {
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
    eprintln!("pooled-spacing fleet: {total} corpora in {}", dir.display());

    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    let corpora: Vec<PoolCorpus> = files
        .par_iter()
        .map(|path| {
            let id = path.file_stem().unwrap().to_string_lossy().to_string();
            let c = analyze_pooled(id, &load_corpus(path));
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(200) {
                eprintln!("  …{n}/{total}");
            }
            c
        })
        .collect();
    eprintln!("pooled-spacing fleet analyze: {:?}", t0.elapsed());

    // Aggregates.
    let mut focus_a: BTreeMap<char, ACell> = BTreeMap::new();
    let mut focus_b: BTreeMap<char, BCell> = BTreeMap::new();
    let mut pd_a: BTreeMap<char, ACell> = BTreeMap::new();
    let mut pd_b: BTreeMap<char, BCell> = BTreeMap::new();
    let (mut shipped, mut a_tot, mut b_tot) = (0u64, 0u64, 0u64);
    let (mut a_pd_tot, mut b_pd_tot) = (0u64, 0u64);
    let mut a_level = ALevelTally::default();
    let mut b_cat = [0u64; 4];
    let (mut disagree, mut double, mut no_neighbour) = (0u64, 0u64, 0u64);
    let mut a_hist = [0u64; 40];
    let mut b_hist = [0u64; 40];
    let (mut num_conv_corpora, mut quote_conv_corpora) = (0u64, 0u64);
    let (mut num_cover_corpora, mut quote_cover_corpora) = (0u64, 0u64);
    let mut new_digit = Vec::new();
    let mut new_quote = Vec::new();
    let mut new_medial = Vec::new();
    let mut new_pd = Vec::new();
    let mut pred_a = Vec::new();
    let mut pred_b = Vec::new();
    // Noisiest corpora by new-pool activity (number+quote+dash flag volume).
    let mut noisy: Vec<(String, u64, u64, u64)> = Vec::new();

    for c in &corpora {
        shipped += c.shipped_findings;
        a_tot += c.a_findings;
        b_tot += c.b_findings;
        a_pd_tot += c.a_pd_findings;
        b_pd_tot += c.b_pd_findings;
        a_level.add(&c.a_level);
        for (x, y) in b_cat.iter_mut().zip(&c.b_cat_flags) {
            *x += y;
        }
        disagree += c.disagreements;
        double += c.double_flags;
        no_neighbour += c.no_neighbour;
        for (h, ch) in a_hist.iter_mut().zip(&c.a_hist) {
            *h += ch;
        }
        for (h, ch) in b_hist.iter_mut().zip(&c.b_hist) {
            *h += ch;
        }
        num_conv_corpora += u64::from(c.number_has_conv);
        quote_conv_corpora += u64::from(c.quote_has_conv);
        num_cover_corpora += u64::from(c.number_flag_sites > 0);
        quote_cover_corpora += u64::from(c.quote_flag_sites > 0);
        for (&mark, cell) in &c.a_po {
            if POOL_FOCUS_MARKS.contains(&mark) {
                let e = focus_a.entry(mark).or_insert([[[0u64; 2]; 4]; 2]);
                for s in 0..2 {
                    for sub in 0..4 {
                        for bit in 0..2 {
                            e[s][sub][bit] += cell[s][sub][bit];
                        }
                    }
                }
            }
        }
        for (&mark, cell) in &c.b_po {
            if POOL_FOCUS_MARKS.contains(&mark) {
                let e = focus_b.entry(mark).or_insert([[0u64; 4]; 2]);
                for s in 0..2 {
                    for cat in 0..4 {
                        e[s][cat] += cell[s][cat];
                    }
                }
            }
        }
        for (&mark, cell) in &c.a_pd {
            let e = pd_a.entry(mark).or_insert([[[0u64; 2]; 4]; 2]);
            for s in 0..2 {
                for sub in 0..4 {
                    for bit in 0..2 {
                        e[s][sub][bit] += cell[s][sub][bit];
                    }
                }
            }
        }
        for (&mark, cell) in &c.b_pd {
            let e = pd_b.entry(mark).or_insert([[0u64; 4]; 2]);
            for s in 0..2 {
                for cat in 0..4 {
                    e[s][cat] += cell[s][cat];
                }
            }
        }
        new_digit.extend(c.new_digit.iter().cloned());
        new_quote.extend(c.new_quote.iter().cloned());
        new_medial.extend(c.new_medial.iter().cloned());
        new_pd.extend(c.new_pd.iter().cloned());
        pred_a.extend(c.pred_a.iter().cloned());
        pred_b.extend(c.pred_b.iter().cloned());
        if c.number_flag_sites + c.quote_flag_sites + c.a_pd_findings > 0 {
            noisy.push((
                c.id.clone(),
                c.number_flag_sites,
                c.quote_flag_sites,
                c.a_pd_findings,
            ));
        }
    }

    println!("=== POOLED-SPACING SPIKE — fleet aggregate ({total} corpora) ===");
    println!(
        "SPIKE — measurement only, nothing frozen. Reference constants: z 1.96, knee k=32 + 40/10k on the pool, floor 0.5."
    );
    println!("no-neighbour sides (book-edge seam-cross found nothing): {no_neighbour}");

    // 1. Per-pool volume census.
    println!(
        "\n══ 1. Per-pool volume census (Design A; * = Wilson-dominant convention at floor) ══"
    );
    for &mark in POOL_FOCUS_MARKS {
        if let Some(cell) = focus_a.get(&mark) {
            print_pool_census(mark, cell);
        }
    }
    println!(
        "\nMAKE-OR-BREAK — corpora reaching a Wilson-dominant convention:\n  Number pool: {num_conv_corpora}/{total} corpora  (of which {num_cover_corpora} actually FLAG ≥1 Number-pool site — real coverage vs silent theory)\n  Quote sub-pool: {quote_conv_corpora}/{total} corpora  (of which {quote_cover_corpora} FLAG ≥1 Quote-pool site)"
    );

    // 2. What the pooled model newly flags vs shipped.
    println!("\n══ 2. New flags vs the shipped rule (Po lane, ref constants) ══");
    println!("  shipped {shipped}   Design A {a_tot}   Design B {b_tot}");
    let diverse = |v: &[PoolSample], cap: usize| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap()
                .then_with(|| a.corpus.cmp(&b.corpus))
        });
        let mut out = Vec::new();
        let mut per: BTreeMap<String, u64> = BTreeMap::new();
        for x in s {
            let e = per.entry(x.corpus.clone()).or_default();
            if *e < 2 {
                *e += 1;
                out.push(x);
            }
            if out.len() >= cap {
                break;
            }
        }
        out
    };
    println!("\n  digit pools (`7. 800` / decimals):");
    print_pool_samples(&diverse(&new_digit, 20));
    println!("\n  quote-adjacent (`word .\"` vs `word.\"`):");
    print_pool_samples(&diverse(&new_quote, 20));
    println!("\n  medial periods (`word.word`, letter attached on the right):");
    print_pool_samples(&diverse(&new_medial, 20));

    // 3. Six-corpus regression.
    println!("\n══ 3. Six-corpus regression vs the shipped rule ══");
    println!("  (shipped findings must be reproduced by their Letter pools)");
    for &(f, short) in POOL_REGRESSION {
        println!("  ({short})");
        pooled_regression(f);
    }

    // 4. Fleet totals + per-class delta.
    println!("\n══ 4. Fleet totals + per-class delta ══");
    println!(
        "  shipped {shipped}  →  Design A {a_tot}  (delta {:+})   Design B {b_tot}  (delta {:+})",
        a_tot as i64 - shipped as i64,
        b_tot as i64 - shipped as i64
    );
    println!(
        "  Design A findings by pool level: letter {} | number {} | punct(quote {} / other {}) | top-fallback {}  (total flagged sides {})",
        a_level.letter,
        a_level.number,
        a_level.punct_quote,
        a_level.punct_other,
        a_level.top,
        a_level.total()
    );
    println!(
        "  Design B flagged sides by observed category: letter {} | number {} | ws {} | punct {}",
        b_cat[0], b_cat[1], b_cat[2], b_cat[3]
    );
    println!("  hierarchy telemetry: class-vs-top disagreements {disagree}  double-flags {double}");

    // 5. Histograms + noisiest + FP adjudication.
    println!("\n══ 5. Score histograms + noisiest new-pool corpora + FP adjudication ══");
    print_pool_hist("Design A", &a_hist);
    print_pool_hist("Design B", &b_hist);
    noisy.sort_by_key(|b| std::cmp::Reverse(b.1 + b.2 + b.3));
    println!("\n  noisiest new-pool corpora (number-flag / quote-flag / dash-flag sites):");
    for (id, nf, qf, df) in noisy.iter().take(15) {
        println!("  {id:<26} number {nf:>5}  quote {qf:>5}  dash {df:>5}");
    }
    println!("\n  disagreement (a) — A flags spaced-content, Design B structurally blind (Ws):");
    print_pool_samples(&diverse(&pred_a, 16));
    println!(
        "\n  disagreement (b) — Design B flags rare-content attachment, Design A's thin pool silent:"
    );
    print_pool_samples(&diverse(&pred_b, 16));

    // Pd lane.
    println!(
        "\n══ Pd dash lane (separately reported — domain widening is an adjudication, not this spike's decision) ══"
    );
    println!("  Design A dash findings {a_pd_tot}   Design B dash findings {b_pd_tot}");
    println!("  fleet-summed dash per-side per-class census:");
    let mut pd_order: Vec<(&char, &ACell)> = pd_a.iter().collect();
    pd_order.sort_by_key(|(_, m)| std::cmp::Reverse(m.iter().flatten().flatten().sum::<u64>()));
    for (mark, cell) in pd_order.iter().take(10) {
        print_pool_census(**mark, cell);
    }
    println!("\n  Pd new-coverage samples:");
    print_pool_samples(&diverse(&new_pd, 16));

    // Head-to-head verdict scaffold (numbers above fill it in).
    println!("\n══ Head-to-head verdict ══");
    println!("  criterion                              Design A                         Design B");
    println!("  fleet findings (Po)                    {a_tot:<32} {b_tot}");
    println!(
        "  spaced-side-vs-content judgeable        yes (class conditions the pool)  NO (ws is terminal)"
    );
    println!(
        "  rare-content hapax over-flag           thin pool self-gates (Wilson)    flags (non-modal content)"
    );
    println!("  see pred(a)/pred(b) samples + regression above for the confirmed/refuted calls.");
}

#[cfg(test)]
mod pooled_tests {
    use super::*;

    fn seg(text: &str) -> Vec<ssc_core::grapheme::GSpan> {
        let mut g = Vec::new();
        ssc_core::grapheme::segment(text, &mut g);
        g
    }
    /// Design-A reads for a standalone verse (no seam neighbours).
    fn a_reads(text: &str) -> Vec<(char, Option<(bool, SubClass)>, Option<(bool, SubClass)>)> {
        pool_opps(text, &seg(text), None, None)
            .into_iter()
            .map(|o| (o.mark, o.a_left, o.a_right))
            .collect()
    }
    /// Design-B immediate reads for a standalone verse.
    fn b_reads(text: &str) -> Vec<(char, BCat, BCat)> {
        pool_opps(text, &seg(text), None, None)
            .into_iter()
            .map(|o| (o.mark, o.b_left, o.b_right))
            .collect()
    }

    #[test]
    fn design_a_conditions_on_neighbour_class() {
        // English attached comma: letter-attached left, letter-spaced right.
        assert_eq!(
            a_reads("word, word"),
            vec![(
                ',',
                Some((true, SubClass::Letter)),
                Some((false, SubClass::Letter))
            )]
        );
        // Missing space after: letter-attached both sides.
        assert_eq!(
            a_reads("word,word"),
            vec![(
                ',',
                Some((true, SubClass::Letter)),
                Some((true, SubClass::Letter))
            )]
        );
        // A decimal: number-attached both sides (the digit pool).
        assert_eq!(
            a_reads("7.8"),
            vec![(
                '.',
                Some((true, SubClass::Number)),
                Some((true, SubClass::Number))
            )]
        );
        // Spaced-from-a-number (`7. 800`): number class, SPACED bit on the right.
        assert_eq!(
            a_reads("7. 800"),
            vec![(
                '.',
                Some((true, SubClass::Number)),
                Some((false, SubClass::Number))
            )]
        );
    }

    #[test]
    fn quote_neighbour_subclass_merges_into_punct() {
        // `word."` — the period's right neighbour is a straight quote: sub-class
        // Quote (attached), whose model class is Punct.
        let r = a_reads("word.\"");
        assert_eq!(
            r,
            vec![(
                '.',
                Some((true, SubClass::Letter)),
                Some((true, SubClass::Quote))
            )]
        );
        assert_eq!(SubClass::Quote.pclass(), PClass::Punct);
        // Spaced from the quote: `word ."` — quote sub-class, spaced bit.
        assert_eq!(
            a_reads("word .\""),
            vec![(
                '.',
                Some((false, SubClass::Letter)),
                Some((true, SubClass::Quote))
            )]
        );
    }

    #[test]
    fn design_b_reads_immediate_only() {
        // `7. 800` — Design B sees Ws on the right (whitespace is terminal); it
        // cannot tell this from `word. Word`.
        assert_eq!(b_reads("7. 800"), vec![('.', BCat::Number, BCat::Ws)]);
        assert_eq!(b_reads("word. Word"), vec![('.', BCat::Letter, BCat::Ws)]);
        // Attached decimal: number immediate on both sides.
        assert_eq!(b_reads("7.8"), vec![('.', BCat::Number, BCat::Number)]);
        // Quote merges into punct.
        assert_eq!(b_reads("word.\""), vec![('.', BCat::Letter, BCat::Punct)]);
    }

    #[test]
    fn verse_final_mark_reads_spaced_with_next_verse_edge_class() {
        // Two verses in one book: the first ends with a mark, so its right side
        // reaches the seam (spaced) and takes the NEXT verse's first edge class.
        let vm = Corpus::try_from_parts(
            vec!["GEN 1:1".to_string(), "GEN 1:2".to_string()],
            vec!["Alpha.".to_string(), "Beta".to_string()],
        )
        .unwrap();
        let mut got = None;
        for_each_pool_opp(&vm, |key, _t, opp| {
            if key == "GEN 1:1" && opp.mark == '.' {
                got = Some((opp.a_left, opp.a_right));
            }
        });
        // Left = letter attached (Alpha); right = spaced (seam), class Letter
        // (Beta's first edge grapheme across the seam).
        assert_eq!(
            got,
            Some((
                Some((true, SubClass::Letter)),
                Some((false, SubClass::Letter))
            ))
        );
    }

    #[test]
    fn book_edge_has_no_neighbour() {
        // A mark at the very end of the last verse of the book: right seam finds
        // nothing → no neighbour on that side.
        let vm =
            Corpus::try_from_parts(vec!["GEN 1:1".to_string()], vec!["End.".to_string()]).unwrap();
        let mut got = None;
        for_each_pool_opp(&vm, |_key, _t, opp| {
            if opp.mark == '.' {
                got = Some(opp.a_right);
            }
        });
        assert_eq!(got, Some(None));
    }

    #[test]
    fn period_letter_letter_medial_is_the_flagged_shape() {
        // In a corpus of clean sentence periods, one medial `word.word` is the
        // rare attached-right minority in the Letter pool ⇒ flagged.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        // Right side, Letter class: 200 spaced (sentence periods), 1 attached.
        cell[1][SubClass::Letter.index()][1] = 200; // spaced
        cell[1][SubClass::Letter.index()][0] = 1; // attached (the medial)
        let v = eval_a_side(&cell, 1, Some((true, SubClass::Letter))).unwrap();
        assert!(v.flagged, "medial attached period is the rare minority");
        assert_eq!(v.cls, PClass::Letter);
        // The dominant spaced form is silent.
        let maj = eval_a_side(&cell, 1, Some((false, SubClass::Letter))).unwrap();
        assert!(!maj.flagged, "the spaced convention is silent");
    }

    #[test]
    fn en_dash_medial_both_attached_is_the_conventional_shape() {
        // A dash used word-medially both-attached (`para-dais`) corpus-wide is
        // the CONVENTION for a dash — attached is the majority, so it is silent.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        cell[0][SubClass::Letter.index()][0] = 300; // left letter attached
        cell[1][SubClass::Letter.index()][0] = 300; // right letter attached
        let l = eval_a_side(&cell, 0, Some((true, SubClass::Letter))).unwrap();
        let r = eval_a_side(&cell, 1, Some((true, SubClass::Letter))).unwrap();
        assert!(
            !l.flagged && !r.flagged,
            "medial-attached dash is the convention, silent"
        );
        // A lone SPACED dash in that attached-convention corpus is the anomaly.
        cell[1][SubClass::Letter.index()][1] = 1; // one spaced-right
        let anom = eval_a_side(&cell, 1, Some((false, SubClass::Letter))).unwrap();
        assert!(
            anom.flagged,
            "the lone spaced dash surfaces against the attached convention"
        );
    }

    #[test]
    fn thin_pool_self_gates_where_design_b_over_flags() {
        // A single decimal `7.8` (attached number) in a corpus with no other
        // number neighbours: Design A's Number pool is N=1, holds no convention,
        // and (alone) cannot flag; Design B flags the non-modal number category.
        // Here we assert the pool-level self-gate directly.
        let mut cell: ACell = [[[0u64; 2]; 4]; 2];
        cell[1][SubClass::Number.index()][0] = 1; // one attached number-right
        let cc = a_class_counts(&cell, 1, PClass::Number);
        assert!(
            !pool_holds_convention(cc[0], cc[1]),
            "N=1 number pool holds no convention"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Behavior oracle for the event-stream port (Phase 0). Deterministic dumps
// of the real `analyze_with_config` / `analyze_stateful` outputs; byte-
// identical dumps across the port are the acceptance gate.
// ─────────────────────────────────────────────────────────────────────────

/// The two oracle configs: shipped defaults, and everything-on (all rules
/// enabled, knobs at their defaults).
fn oracle_config(name: &str) -> Config {
    match name {
        "default" => Config::v1_defaults(),
        "all" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg
        }
        other => panic!("unknown oracle config {other:?} (want default|all)"),
    }
}

/// Which slice of the vref fleet an oracle pass covers.
///
/// `Full` is the whole directory (~1,504 corpora) — the real behavior
/// contract for a before/after gate. `Wa` is the `WA-*` subset (~251, the
/// Wycliffe Associates translations) — a ~6× faster inner-loop oracle for
/// intermediate steps. A `Wa` dump is only ever diffed against another `Wa`
/// dump; the two scopes are different contracts, never compared to each other.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OracleScope {
    Full,
    Wa,
}

impl OracleScope {
    /// Parses the optional trailing scope token on a dump command; absent or
    /// `full` → `Full`, `wa` → `Wa`. Anything else is a hard error so a typo
    /// can't silently widen the pass back to the full fleet.
    fn parse(rest: &[String]) -> Self {
        match rest
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | ["full"] => Self::Full,
            ["wa"] => Self::Wa,
            other => panic!("unknown oracle scope {other:?} (want wa|full)"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Wa => "wa",
        }
    }
}

fn oracle_files(path: &Path, scope: OracleScope) -> Vec<std::path::PathBuf> {
    if path.is_dir() {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .filter(|p| match scope {
                OracleScope::Full => true,
                OracleScope::Wa => p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("WA-")),
            })
            .collect();
        files.sort();
        files
    } else {
        // A single-file target ignores scope — there's nothing to subset.
        vec![path.to_path_buf()]
    }
}

/// The proportionality reference: WA-en-ulb from the same directory, if there.
fn oracle_source(path: &Path) -> Option<Corpus> {
    let dir = if path.is_dir() { path } else { path.parent()? };
    let src = dir.join("WA-en-ulb.txt");
    src.exists().then(|| load_corpus(&src))
}

/// Write each finding's oracle-column row, resolving `key_idx` back to its
/// wire-format key string (`GEN 1:1`) via `resolve_findings` so the dumped
/// column is byte-identical to the pre-migration `sid` column.
fn write_findings(
    out: &mut impl Write,
    corpus_id: &str,
    tag: &str,
    corpus: &Corpus,
    findings: &[Finding],
) {
    for f in ssc_core::corpus::resolve_findings(corpus, findings) {
        let score = f
            .score
            .map_or_else(|| "-".to_string(), |s| format!("{s:.6}"));
        let args = f
            .args
            .as_ref()
            .map_or_else(|| "-".to_string(), |a| serde_json::to_string(a).unwrap());
        writeln!(
            out,
            "{corpus_id}\t{tag}\t{}\t{}\t{}\t{}\t{:?}\t{score}\t{args}",
            f.sid,
            f.code.code(),
            f.range.start,
            f.range.end,
            f.severity,
        )
        .unwrap();
    }
}

fn dump_findings(path: &Path, out_path: &Path, cfg_name: &str, scope: OracleScope) {
    let cfg = oracle_config(cfg_name);
    let source = oracle_source(path);
    let files = oracle_files(path, scope);
    let total = files.len();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).unwrap());
    for (i, file) in files.iter().enumerate() {
        let id = file.file_stem().unwrap().to_string_lossy().to_string();
        let target = load_corpus(file);
        let findings = analyze_with_config(&target, source.as_ref(), &cfg);
        write_findings(&mut out, &id, "full", &target, &findings);
        if (i + 1) % 100 == 0 {
            eprintln!("{}/{total}", i + 1);
        }
    }
    eprintln!(
        "dumped {total} corpora ({cfg_name}, scope={}) -> {}",
        scope.label(),
        out_path.display()
    );
}

/// FNV-1a 64 over a string — a dependency-free stats digest.
fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// One stats-digest line for the incremental oracle:
/// `stats<TAB>id<TAB>mode<TAB>rules_len<TAB>rules_fnv<TAB>prov_fnv`. The `stats`
/// sentinel is column 1 so every digest line is mechanically separable from
/// finding lines (which start with the corpus id). `rules_len`/`rules_fnv`
/// digest the per-rule sections alone; `prov_fnv` digests the provenance map
/// alone — split so the gate can prove a wire change touched only provenance.
///
/// The rules view (`{"rules":…}`) is byte-identical to the whole-`Stats`
/// serialization before provenance existed, so `rules_fnv` stays pinned across
/// the provenance addition; only `prov_fnv` moves.
fn write_stats_digest(out: &mut impl Write, id: &str, mode: &str, stats: &ssc_core::Stats) {
    #[derive(serde::Serialize)]
    struct RulesView<'a> {
        rules: &'a std::collections::BTreeMap<ssc_core::RuleId, ssc_core::RuleStats>,
    }
    let rules = serde_json::to_string(&RulesView {
        rules: stats.oracle_rules(),
    })
    .unwrap();
    let prov = serde_json::to_string(&stats.tallied).unwrap();
    writeln!(
        out,
        "stats\t{id}\t{mode}\t{}\t{:016x}\t{:016x}",
        rules.len(),
        fnv64(&rules),
        fnv64(&prov),
    )
    .unwrap();
}

/// A fixed, multi-rule-provoking edit applied to the last verse of the first
/// book: doubles punctuation, excess whitespace, a rare glyph, a mixed-case
/// word, a spaced comma, an unbalanced paren.
const EDIT_TEXT: &str = "He fell ,, the  gate stood.. qQx deJésus (broken";

fn dump_incremental(
    path: &Path,
    out_path: &Path,
    cfg_name: &str,
    cached: bool,
    scope: OracleScope,
) {
    let cfg = oracle_config(cfg_name);
    let source = oracle_source(path);
    let files = oracle_files(path, scope);
    // Every 8th corpus (plus the first): the incremental gate needs breadth,
    // not the whole fleet, and this dump runs three analyses per corpus. The
    // WA subset is subsampled the same way (~32 corpora) after scope filtering.
    let files: Vec<_> = files.into_iter().step_by(8).collect();
    let total = files.len();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).unwrap());
    for (i, file) in files.iter().enumerate() {
        let id = file.file_stem().unwrap().to_string_lossy().to_string();
        let target = load_corpus(file);
        if target.is_empty() {
            continue;
        }
        let mut cache = cached.then(AnalysisCache::new);
        let (_, prior) =
            ssc_core::analyze_stateful(&target, source.as_ref(), &cfg, None, cache.as_mut());
        // The edit: last verse of the first book. `Books` from `by_book` is
        // in presented order, so the first group always starts at position 0
        // — no need to resolve its global `KeyIdx` base (which this example,
        // a separate compilation unit, cannot do; `KeyIdx`'s constructor is
        // crate-private).
        let first_books = ssc_core::corpus::by_book(&target);
        let first_group = first_books.first().unwrap();
        let first_len = first_group.keys.len();
        drop(first_books);

        let mut edited_texts = target.texts().to_vec();
        edited_texts[first_len - 1] = EDIT_TEXT.to_string();
        let edited = Corpus::try_from_parts(target.keys().to_vec(), edited_texts).unwrap();

        // Local echo: edited book only + prior.
        let echo = Corpus::try_from_parts(
            edited.keys()[..first_len].to_vec(),
            edited.texts()[..first_len].to_vec(),
        )
        .unwrap();
        let (echo_findings, echo_stats) = ssc_core::analyze_stateful(
            &echo,
            source.as_ref(),
            &cfg,
            Some(prior.clone()),
            cache.as_mut(),
        );
        write_findings(&mut out, &id, "echo", &echo, &echo_findings);
        write_stats_digest(&mut out, &id, "echo", &echo_stats);

        // Complete snapshot: whole corpus + prior. The edited book re-tallies by
        // content hash; clean siblings carry — no changed hint.
        let (snap, stats) = ssc_core::analyze_stateful(
            &edited,
            source.as_ref(),
            &cfg,
            Some(prior),
            cache.as_mut(),
        );
        write_findings(&mut out, &id, "snap", &edited, &snap);
        write_stats_digest(&mut out, &id, "snap", &stats);
        if (i + 1) % 20 == 0 {
            eprintln!("{}/{total}", i + 1);
        }
    }
    eprintln!(
        "dumped {total} corpora incremental ({cfg_name}, scope={}) -> {}",
        scope.label(),
        out_path.display()
    );
}

fn time_configs(path: &Path) {
    let target = load_corpus(path);
    let source = oracle_source(path);
    for name in ["default", "all"] {
        let cfg = oracle_config(name);
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let t0 = std::time::Instant::now();
            let f = analyze_with_config(&target, source.as_ref(), &cfg);
            let dt = t0.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(f);
            best = best.min(dt);
        }
        println!("{name}: {best:.1} ms (min of 5)");
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Census (absolute mode) harness — plan 2026-07-10.
// ─────────────────────────────────────────────────────────────────────────

fn census_single(path: &Path) {
    let target = load_corpus(path);
    let t0 = std::time::Instant::now();
    let inv = ssc_core::census(&target, &ssc_core::CensusOptions::default());
    let dt = t0.elapsed().as_secs_f64() * 1000.0;
    let wire = serde_json::to_string(&inv).unwrap().len();
    println!(
        "census of {} — {} verses, {:.1} ms, wire {} KB",
        path.display(),
        target.len(),
        dt,
        wire / 1024
    );
    for s in &inv.sections {
        println!(
            "\n== {:?} — lane_total {}, rows {}",
            s.id,
            s.lane_total,
            s.rows.len()
        );
        for r in s.rows.iter().take(20) {
            println!(
                "  {:>8}  {:?}  ({} examples)",
                r.count,
                r.key,
                r.examples.len()
            );
        }
        if s.rows.len() > 20 {
            println!(
                "  … {} more (ascending; tail above is the rare end)",
                s.rows.len() - 20
            );
        }
    }
}

fn census_fleet(dir: &Path) {
    let files = oracle_files(dir, OracleScope::Full);
    let total = files.len();
    let mut rows_per_section: BTreeMap<String, u64> = BTreeMap::new();
    let mut wire_sizes: Vec<usize> = Vec::new();
    let mut census_ms = 0.0f64;
    let mut analyze_ms = 0.0f64;
    let mut worst: (usize, String) = (0, String::new());
    let cfg = Config::v1_defaults();
    for (i, file) in files.iter().enumerate() {
        let id = file.file_stem().unwrap().to_string_lossy().to_string();
        let target = load_corpus(file);
        let t0 = std::time::Instant::now();
        let inv = ssc_core::census(&target, &ssc_core::CensusOptions::default());
        census_ms += t0.elapsed().as_secs_f64() * 1000.0;
        let t1 = std::time::Instant::now();
        let f = analyze_with_config(&target, None, &cfg);
        analyze_ms += t1.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(f);
        let wire = serde_json::to_string(&inv).unwrap().len();
        if wire > worst.0 {
            worst = (wire, id);
        }
        wire_sizes.push(wire);
        for s in &inv.sections {
            *rows_per_section.entry(format!("{:?}", s.id)).or_default() += s.rows.len() as u64;
        }
        if (i + 1) % 200 == 0 {
            eprintln!("{}/{total}", i + 1);
        }
    }
    wire_sizes.sort_unstable();
    let pct = |p: f64| wire_sizes[((wire_sizes.len() - 1) as f64 * p) as usize];
    println!("census fleet dry-run: {total} corpora");
    println!("rows per section (fleet totals):");
    for (k, v) in &rows_per_section {
        println!("  {k}: {v}");
    }
    println!(
        "wire size KB: p50 {} · p90 {} · p99 {} · max {} ({})",
        pct(0.5) / 1024,
        pct(0.9) / 1024,
        pct(0.99) / 1024,
        worst.0 / 1024,
        worst.1
    );
    println!(
        "timing: census total {:.1} s vs default-analyze total {:.1} s (ratio {:.2}x)",
        census_ms / 1000.0,
        analyze_ms / 1000.0,
        census_ms / analyze_ms
    );
}
