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
//!   # incremental oracle (resident-Galley complete-snapshot mutation
//!   # transcript) now lives in ssc-galley's own example (dependency-direction
//!   # restore — ssc-core no longer dev-depends on ssc-galley):
//!   cargo run --release -p ssc-galley --example transcript_oracle -- \
//!       --dump-incremental corpora/vref /tmp/incremental.tsv default
//!   # fast inner-loop oracle: WA subset only (~251 corpora, ~6x quicker) —
//!   # trailing `wa` scopes any dump command; omit (or `full`) for the whole
//!   # fleet. A `wa` dump only ever diffs against another `wa` dump.
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       --dump-findings corpora/vref /tmp/findings.wa.tsv default wa
//!   # source-paired tier plan Phase A: paired survey (per-book fraction/
//!   # median/MAD/z=3.5 boundaries + versification quarantine) over a pairs
//!   # manifest, writing per-pair TSVs and a self-contained HTML report:
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       --paired-survey documentation/calibration/corpora-pairs.tsv /tmp/paired
//!   # same plan, deterministic fixed-seed fault injection + catch-rate join:
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       --seed-faults documentation/calibration/corpora-pairs.tsv /tmp/paired

// Spike/survey/dev code — std collections are fine here; the workspace
// disallowed-types ban targets shipped engine code.
#![allow(clippy::disallowed_types)]
use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::config::{ProportionalityConfig, RepeatedCharacterRunConfig};
use ssc_core::{Corpus, FindingArgs, LengthRatioScope};

#[path = "../../dev/vref_io.rs"]
mod vref_io;
use vref_io::load_corpus;

mod corpus_blob;
mod oracle;
mod reporting;
mod survey;
use corpus_blob::{Preset, build_blob};
use oracle::{OracleScope, dump_findings};
use reporting::{census_fleet, census_single, time_configs};
use survey::casing::{analyze_casing, casing_fleet, casing_single_report};
use survey::glyphs::{analyze_glyphs, glyph_fleet, glyph_single_report};
use survey::misc::{
    batch, bracket_calib, fleet, punct_calib, punct_only_calib, repeat_calib,
    spacing_fleet_sweep, zwsp_calib,
};
use survey::mixedcase::{analyze_mixedcase, mixedcase_fleet, mixedcase_single_report};
use survey::paired::{paired_survey, seed_faults, uw_calibrate, uw_case_shape_simulate};
use survey::pooled::{analyze_pooled, pooled_fleet, pooled_single_report};
use survey::review_depth_candidates::{review_depth_path_survey, review_depth_survey};
use survey::signatures::{analyze_signatures, signature_fleet, signature_single_report};
use survey::terminal::{terminal_fleet, terminal_single};

// terminal_strength SPIKE (shortlist 2/3) — dev-only sweep harness. The trust
// model itself now ships in `signals::casing` (ADR 0052); this spike retains
// the multiplier-vs-gate sweep reporting the calibration doc was built from,
// reading the graduated `analysis::association`.
#[path = "../../dev/terminal.rs"]
mod terminal;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (target_dir, source_dir, z) = match args.as_slice() {
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
        // Review Depth pilot survey: compact per-corpus TSV rows over the
        // committed five-anchor grids. Use a small/WA pass before full fleet.
        [flag, path, out, tier] if flag == "--review-depth-survey" => {
            review_depth_survey(Path::new(path), Path::new(out), tier);
            return;
        }
        // Selected Review Depth path audit: measure the production interpolation
        // at its two interior checkpoints after the 0/50/100 owner decision.
        [flag, path, out, tier] if flag == "--review-depth-path-survey" => {
            review_depth_path_survey(Path::new(path), Path::new(out), tier);
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
        // `uni.nonletter-usage-anomaly` PROBE (epic plan §9) — dev-only, no live
        // rule. A file prints its per-corpus channel detail; a vref directory
        // runs the fleet sweep. A trailing `overlap` adds the old-rule ledger,
        // which costs three extra rule passes per corpus.
        [flag, path, rest @ ..] if flag == "--nonletter" && rest.len() <= 1 => {
            let p = Path::new(path);
            if p.is_dir() {
                let overlap = rest.first().is_some_and(|r| r == "overlap");
                survey::nonletter::nonletter_fleet(p, overlap);
            } else {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                survey::nonletter::nonletter_single_report(&id, &load_corpus(p));
            }
            return;
        }
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
        // Build a pre-parsed corpus blob for one of the three fixed presets
        // (`small` ~15 script-diverse corpora, `wa` ~251, `full` ~1,504) from
        // a vref directory. Regenerate only when corpora/vref itself changes;
        // the output is a gitignored build artifact (put it under target/),
        // never committed. Pass the resulting `.blob` file wherever a
        // dump-findings/dump-incremental `<path>` is expected — it's a
        // drop-in replacement for the directory, and needs no trailing
        // wa|full scope token of its own (the blob's preset already implies it).
        [flag, dir, preset, out] if flag == "--build-blob" => {
            build_blob(Path::new(dir), Preset::parse(preset), Path::new(out));
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
        // dump — never a `full` one. `<path>` may also be a `.blob` file
        // built via `--build-blob`, in which case the scope token is ignored
        // (the blob already encodes its preset).
        [flag, path, out, cfg_name, rest @ ..] if flag == "--dump-findings" => {
            dump_findings(
                Path::new(path),
                Path::new(out),
                cfg_name,
                OracleScope::parse(rest),
            );
            return;
        }
        // The incremental oracle (`--dump-incremental`) moved to ssc-galley's
        // own example so ssc-core no longer dev-depends on ssc-galley:
        //   cargo run --release -p ssc-galley --example transcript_oracle -- \
        //       --dump-incremental <dir|blob> <out> <default|all> [wa|full]
        [flag, ..] if flag == "--dump-incremental" => {
            eprintln!(
                "--dump-incremental moved to ssc-galley: cargo run --release -p ssc-galley \
                 --example transcript_oracle -- --dump-incremental <dir|blob> <out> \
                 <default|all> [wa|full]"
            );
            std::process::exit(2);
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
        // Source-paired tier plan, Phase A (2026-07-30): per-book paired
        // survey (fraction, median, MAD, z=3.5 flag boundaries, the
        // versification quarantine guard) over every loadable row of a
        // pairs manifest, plus a self-contained HTML report.
        [flag, pairs, out] if flag == "--paired-survey" => {
            paired_survey(Path::new(pairs), Path::new(out));
            return;
        }
        // Same plan, Phase A step 3: deterministic fixed-seed fault
        // injection (tail-chop 10/20/30/50%, whole-verse delete,
        // source-verse paste) over every loadable pairs-manifest row, with a
        // ground-truth manifest and a catch-rate/clean-flag-rate join at
        // every swept z.
        [flag, pairs, out] if flag == "--seed-faults" => {
            seed_faults(Path::new(pairs), Path::new(out));
            return;
        }
        // Phase D calibration packet for lex.untranslated-word: baseline
        // findings (genealogy/false-positive read), seeded source-paste
        // recall, and a judging-only knob sweep (emit_score_min,
        // word_recurrence_k, run_bonus) — the shipped substrate/knobs as-is,
        // no observation-schema change.
        [flag, pairs, out] if flag == "--uw-calibrate" => {
            uw_calibrate(Path::new(pairs), Path::new(out));
            return;
        }
        // Harness-side simulation of the proposed case-shape (proper-noun)
        // excusal gate, against every real finding the shipped rule
        // currently produces — does NOT change the substrate. Estimates the
        // observation-schema-change proposal's effect before it is made.
        [flag, pairs, out] if flag == "--uw-case-shape-simulate" => {
            uw_case_shape_simulate(Path::new(pairs), Path::new(out));
            return;
        }
        [t] => {
            batch(Path::new(t));
            return;
        }
        [t, s] => (t, s, ProportionalityConfig::default().z_long),
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

    let t0 = std::time::Instant::now();
    let findings = ssc_core::signals::proportionality::length_ratio_findings(
        &target,
        Some(&source),
        // Ad-hoc single-pair path: one CLI knob drives both sides —
        // documented, not a design claim (the paired harness's
        // `--paired-survey`/`--seed-faults` is where z_long/z_short are
        // actually swept independently).
        &ProportionalityConfig {
            z_long: z,
            z_short: z,
            ..Default::default()
        },
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
