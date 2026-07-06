//! Throwaway calibration harness — NOT the library path.
//!
//! ADR 0010 keeps file IO and segmentation out of `core`'s contract; this
//! example exists only to run rules over the `corpora/` USFM trees and
//! report finding volumes for calibration decisions (vision §10). Its
//! naive marker stripping is good enough to measure with, and nothing
//! else. Production consumers get verse text from onion.
//!
//! Usage:
//!   # proportionality (target vs reference):
//!   cargo run --release -p ssc-core --example calibrate -- \
//!       corpora/bem_reg corpora/en_ulb
//!   # per-verse batch (one corpus, default config):
//!   cargo run --release -p ssc-core --example calibrate -- corpora/en_ulb

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::config::{ProportionalityConfig, PunctuationAdjacencyConfig};
use ssc_core::rule::StatefulRule;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::signals::punctuation::PunctuationAdjacencyAnomaly;
use ssc_core::{
    BookId, Config, Finding, FindingArgs, LengthRatioScope, RuleId, VerseMap, analyze,
    analyze_with_config,
};

#[path = "../dev/usfm_naive.rs"]
mod usfm_naive;
use usfm_naive::load_corpus;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (target_dir, source_dir, z_threshold) = match args.as_slice() {
        // Dump a corpus as `{ "GEN 1:1": text, … }` JSON on stdout —
        // input for the wasm-side bench (scripts/bench-wasm.mjs).
        [flag, t] if flag == "--json" => {
            let map: BTreeMap<String, String> = load_corpus(Path::new(t))
                .iter()
                .map(|(sid, text)| (sid.to_string(), text.clone()))
                .collect();
            println!("{}", serde_json::to_string(&map).unwrap());
            return;
        }
        // Corpus-relative ZWSP calibration (ADR 0023): enable the default-off
        // rule at floor 0 to see the full score distribution, and confirm the
        // deterministic hygiene ZWSP storm is gone.
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
        // Repeated-character-run signal exploration: per-finding TSV with the
        // candidate corpus-relative signals (word frequency, run recurrence,
        // corpus base rate) on stdout; per-corpus summary on stderr.
        [flag, t] if flag == "--repeat" => {
            repeat_calib(Path::new(t));
            return;
        }
        [t] => {
            batch(Path::new(t));
            return;
        }
        [t, s] => (t, s, ProportionalityConfig::default().z_threshold),
        [t, s, z] => (t, s, z.parse().expect("z threshold")),
        _ => {
            eprintln!("usage: calibrate <target-corpus-dir> [<source-corpus-dir> [z]]");
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
    let findings = rule.judge(&rule.reduce(&target, Some(&source)), &target);
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

/// Repeated-character-run signal exploration. For every finding the shipped
/// stateless rule produces, emit the candidate corpus-relative signals so we
/// can decide how to score:
///   - the containing word and its corpus frequency (hapax or not),
///   - how many tokens / distinct word types contain the *same* cluster run,
///   - the corpus base rate of 3+ letter runs (any cluster).
fn repeat_calib(dir: &Path) {
    use std::collections::{HashMap, HashSet};

    use ssc_core::grapheme::segment;
    use ssc_core::signals::lexical::scan_repeated_character_run;
    use ssc_core::token::tokenize;

    let corpus = dir.file_name().unwrap().to_string_lossy().to_string();
    let target = load_corpus(dir);

    // Corpus pass: word frequencies (case-folded) and run recurrence.
    let mut word_freq: HashMap<String, usize> = HashMap::new();
    let mut cluster_tokens: HashMap<String, usize> = HashMap::new();
    let mut cluster_types: HashMap<String, HashSet<String>> = HashMap::new();
    let mut total_tokens = 0usize;
    let mut tokens_with_run = 0usize;
    let mut graphemes = Vec::new();

    for text in target.values() {
        for tok in tokenize(text) {
            let word = tok.span.slice(text);
            let folded = word.to_lowercase();
            *word_freq.entry(folded.clone()).or_default() += 1;
            total_tokens += 1;
            graphemes.clear();
            segment(word, &mut graphemes);
            let runs = scan_repeated_character_run(word, &graphemes);
            if runs.is_empty() {
                continue;
            }
            tokens_with_run += 1;
            let mut seen = HashSet::new();
            for r in &runs {
                // Cluster = first grapheme of the run, folded.
                let run_str = r.slice(word);
                let cluster = run_str
                    .graphemes_first()
                    .to_lowercase();
                if seen.insert(cluster.clone()) {
                    *cluster_tokens.entry(cluster.clone()).or_default() += 1;
                    cluster_types.entry(cluster).or_default().insert(folded.clone());
                }
            }
        }
    }

    // Finding pass: the rule as shipped, joined to its containing token.
    let findings = analyze(&target, None);
    let repeat: Vec<_> = findings
        .iter()
        .filter(|f| f.code == RuleId::RepeatedCharacterRun)
        .collect();

    println!("corpus\tsid\tword\tcluster\trun_len\tword_freq\tsame_run_tokens\tsame_run_types\ttokens_with_run\ttotal_tokens");
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
            "{corpus}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            f.sid,
            word,
            cluster,
            run_len,
            word_freq.get(&folded).copied().unwrap_or(0),
            cluster_tokens.get(&cluster).copied().unwrap_or(0),
            cluster_types.get(&cluster).map(|s| s.len()).unwrap_or(0),
            tokens_with_run,
            total_tokens,
        );
    }
    eprintln!(
        "{corpus}: {} verses, {} tokens, {} tokens-with-run ({:.2}/10k), {} findings",
        target.len(),
        total_tokens,
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

/// Punctuation adjacency calibration (ADR 0024) at floor 0.
fn punct_calib(dir: &Path) {
    let target = load_corpus(dir);
    eprintln!("{} verses", target.len());
    let rule = PunctuationAdjacencyAnomaly {
        cfg: PunctuationAdjacencyConfig { emit_score_min: 0.0, ..Default::default() },
    };
    let t0 = std::time::Instant::now();
    let findings = rule.judge(&rule.reduce(&target, None), &target);
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

/// Shared score-distribution report for the two corpus-relative rules: total
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

