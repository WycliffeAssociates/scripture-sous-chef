//! Calibrate `PARALLEL_MIN_CHAPTER_MAP_BYTES`: the dirty-chapter work below
//! which the direct lane's chapter fan-out costs more than it saves.
//!
//! One-book COLD analyses (a fresh transient cache every iteration, so every
//! chapter is dirty and the map seam sees the whole book), timed with the route
//! forced two ways in one alternating run — the threshold override is a
//! `bench-probes`-only knob, so this needs no rebuild between routes:
//!
//! - `serial`   — threshold `usize::MAX`, so the seam never fans out by chapter
//! - `chapters` — threshold `0`, so it always does
//!
//! Both routes produce byte-identical findings; the harness asserts that on
//! every scenario, so a route can never be "faster" by doing less.
//!
//! Scenarios are the plan's named books (3JN = 1 chapter, MAT = 28, PSA = 150)
//! plus PSA truncated to a ladder of chapter counts, to sample the byte range
//! between them. Configs: `direct` (per-verse rules only — the seam's own work,
//! isolated from the fused walk so the crossover is visible), then `default` and
//! `all` for the honest end-to-end number.
//!
//! usage: chapter_map_threshold <vref-file> [--batches N] [--iters N]

use std::path::PathBuf;
use std::time::Duration;

use ssc_core::{Config, Corpus, RuleId};

/// Per-verse rules only: the direct lane with as little else around it as the
/// engine allows, so the seam's own crossover is not buried under the fused
/// walk. Every stateful/project rule off.
fn direct_only_config() -> Config {
    let mut cfg = Config::v1_defaults();
    for id in [
        RuleId::SentenceInitialLowercase,
        RuleId::InconsistentWordCasing,
        RuleId::RepeatedCharacterRun,
        RuleId::MixedScriptInToken,
        RuleId::NonletterUsageAnomaly,
        RuleId::RareGlyph,
        RuleId::MixedCaseWord,
        RuleId::ProjectLengthRatio,
        RuleId::BracketBalance,
        RuleId::DuplicateWord,
        RuleId::MixedNormalization,
    ] {
        cfg.rules.insert(id, false);
    }
    cfg
}

/// One book's contiguous run, truncated to its first `chapters` chapters.
fn book_corpus(bible: &Corpus, slug: &str, chapters: Option<usize>) -> Corpus {
    let books = ssc_core::corpus::by_book(bible);
    let bg = books
        .iter()
        .find(|g| g.slug == slug)
        .unwrap_or_else(|| panic!("book {slug} not present"));
    let mut keys = Vec::new();
    let mut texts = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (k, t) in bg.keys.iter().zip(bg.texts.iter()) {
        let chapter = k
            .split_once(' ')
            .and_then(|(_, rest)| rest.split_once(':'))
            .map_or("", |(c, _)| c)
            .to_string();
        if !seen.contains(&chapter) {
            if let Some(limit) = chapters {
                if seen.len() == limit {
                    break;
                }
            }
            seen.push(chapter);
        }
        keys.push(k.clone());
        texts.push(t.clone());
    }
    Corpus::try_from_parts(keys, texts).unwrap()
}

fn median(mut d: Vec<Duration>) -> Duration {
    d.sort();
    d[d.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().map(PathBuf::from) else {
        eprintln!("usage: chapter_map_threshold <vref-file> [--batches N] [--iters N]");
        std::process::exit(2);
    };
    let flag = |name: &str, default: usize| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };
    let only = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let batches = flag("--batches", 5);
    let iters = flag("--iters", 20);

    let bible = spike_bench::vref_io::load_corpus(&path);
    eprintln!(
        "loaded {} verses; shipped threshold = {} bytes; rayon threads = {}",
        bible.len(),
        ssc_core::bench::PARALLEL_MIN_CHAPTER_MAP_BYTES,
        rayon_threads(),
    );

    let scenarios: Vec<(String, Corpus)> = [
        ("3JN".to_string(), book_corpus(&bible, "3JN", None)),
        ("PSA/2".to_string(), book_corpus(&bible, "PSA", Some(2))),
        ("PSA/5".to_string(), book_corpus(&bible, "PSA", Some(5))),
        ("PSA/10".to_string(), book_corpus(&bible, "PSA", Some(10))),
        ("PSA/12".to_string(), book_corpus(&bible, "PSA", Some(12))),
        ("PSA/15".to_string(), book_corpus(&bible, "PSA", Some(15))),
        ("PSA/18".to_string(), book_corpus(&bible, "PSA", Some(18))),
        ("PSA/20".to_string(), book_corpus(&bible, "PSA", Some(20))),
        ("PSA/25".to_string(), book_corpus(&bible, "PSA", Some(25))),
        ("PSA/30".to_string(), book_corpus(&bible, "PSA", Some(30))),
        ("PSA/50".to_string(), book_corpus(&bible, "PSA", Some(50))),
        ("MAT".to_string(), book_corpus(&bible, "MAT", None)),
        ("PSA".to_string(), book_corpus(&bible, "PSA", None)),
    ]
    .into_iter()
    .collect();

    println!(
        "{:<10} {:<8} {:>4} {:>9} {:>12} {:>12} {:>9}",
        "scenario", "config", "ch", "bytes", "serial(mom)", "chapters(mom)", "speedup"
    );
    for (config_name, cfg) in [
        ("direct", direct_only_config()),
        ("default", Config::v1_defaults()),
        ("all", Config::all()),
    ] {
        if only.as_deref().is_some_and(|o| o != config_name) {
            continue;
        }
        for (name, corpus) in &scenarios {
            let chapters = chapter_count(corpus);
            let bytes: usize = corpus.texts().iter().map(String::len).sum();

            // Correctness first: the two routes must agree exactly.
            ssc_core::bench::set_chapter_map_min_bytes(usize::MAX);
            let a = ssc_core::analyze_with_config(corpus, None, &cfg);
            ssc_core::bench::set_chapter_map_min_bytes(0);
            let b = ssc_core::analyze_with_config(corpus, None, &cfg);
            assert_eq!(a, b, "{name}/{config_name}: routes disagree");

            let mut serial_meds = Vec::new();
            let mut chapter_meds = Vec::new();
            for _ in 0..batches {
                for (threshold, out) in [
                    (usize::MAX, &mut serial_meds),
                    (0usize, &mut chapter_meds),
                ] {
                    ssc_core::bench::set_chapter_map_min_bytes(threshold);
                    let (durations, _) = spike_bench::time_trials(iters, || {
                        std::hint::black_box(ssc_core::analyze_with_config(corpus, None, &cfg))
                    });
                    out.push(median(durations));
                }
            }
            let s = median(serial_meds);
            let c = median(chapter_meds);
            println!(
                "{:<10} {:<8} {:>4} {:>9} {:>12?} {:>12?} {:>8.2}x",
                name,
                config_name,
                chapters,
                bytes,
                s,
                c,
                s.as_secs_f64() / c.as_secs_f64()
            );
        }
    }
    // Leave the shipped value in force, in case anything runs after this.
    ssc_core::bench::set_chapter_map_min_bytes(
        ssc_core::bench::PARALLEL_MIN_CHAPTER_MAP_BYTES,
    );
}

fn chapter_count(corpus: &Corpus) -> usize {
    let mut seen: Vec<String> = Vec::new();
    for k in corpus.keys() {
        let chapter = k
            .split_once(' ')
            .and_then(|(_, rest)| rest.split_once(':'))
            .map_or("", |(c, _)| c)
            .to_string();
        if !seen.contains(&chapter) {
            seen.push(chapter);
        }
    }
    seen.len()
}

#[cfg(feature = "parallel")]
fn rayon_threads() -> usize {
    rayon::current_num_threads()
}

#[cfg(not(feature = "parallel"))]
fn rayon_threads() -> usize {
    1
}
