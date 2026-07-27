//! Sanity-check spike: does `dhat` (dhat-rs 0.3, experimental heap profiler)
//! work at all on this codebase's warm resident-analyze path?
//!
//! Mirrors `warm_ladder_profile.rs`'s scenario exactly: load WA-en-ulb via
//! `spike_bench::vref_io::load_corpus`, build `Config::v1_defaults()` or the
//! "all rules on" config, seed a resident `Galley` with a cold `analyze` call,
//! then repeatedly warm-re-analyze with the 3JN book edited via `update_book`
//! (alternating two text variants so the content hash misses every call, same
//! as the samply harness). The only difference: this binary doesn't profile
//! with samply, it profiles with `dhat`'s allocator wrapper, to see whether
//! dhat itself survives this workload (it's documented as liable to crash,
//! hang, or misbehave on some configurations).
//!
//! Usage:
//!   dhat_probe <testing|profile|warm-profile> <default|all|all-no-*> [corpus]
//!
//! `testing` mode: `dhat::Profiler::builder().testing().build()`, seed once,
//! then run N=20 warm iterations, printing per-iteration `HeapStats` deltas
//! (total_blocks/total_bytes) and wall time so the dhat-mode slowdown is
//! visible directly.
//!
//! `warm-profile` mode: as `profile`, but the profiler starts AFTER the cold
//! seed, so the backtraces are the warm analyze's own allocations rather than
//! the seed's (which outnumber them by orders of magnitude).
//!
//! `profile` mode: `dhat::Profiler::new_heap()`, seed + a handful of warm
//! iterations, then let the profiler drop at the end of `main` so it writes
//! `dhat-heap.json` (viewable at https://nnethercote.github.io/dh_view/dh_view.html).

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ssc_core::{BookBlock, Config, Corpus, RuleId};
use ssc_galley::Galley;

/// Default corpus; an optional third argument overrides it (e.g. the hapax-heavy
/// `qub`), so a retained-bytes measurement can be repeated on a second
/// vocabulary regime.
const CORPUS_PATH: &str = "../corpora/vref/WA-en-ulb.txt";
const BOOK_CODE: &str = "3JN";
const WARM_ITERS: usize = 20;

fn build_config(name: &str) -> Config {
    match name {
        "default" => Config::v1_defaults(),
        "all" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg
        }
        // Every rule on EXCEPT punct.spacing-anomaly — its substrate then has no
        // active consumer, so the cache retains none of its products. Paired with
        // "all", the live-bytes difference after the cold seed IS the spacing
        // substrate's retained footprint.
        "all-no-spacing" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg.rules.insert(RuleId::PunctuationSpacingAnomaly, false);
            cfg
        }
        // Same paired-difference trick for the other two migrated substrates:
        // with no active consumer the cache retains none of that substrate's
        // products, so "all" minus this config IS its retained footprint.
        "all-no-adjacency" => {
            let mut cfg = build_config("all");
            cfg.rules.insert(RuleId::PunctuationAdjacencyAnomaly, false);
            cfg
        }
        "all-no-duplicate" => {
            let mut cfg = build_config("all");
            cfg.rules.insert(RuleId::DuplicateWord, false);
            cfg
        }
        "all-no-casing" => {
            let mut cfg = build_config("all");
            cfg.rules.insert(RuleId::SentenceInitialLowercase, false);
            cfg.rules.insert(RuleId::InconsistentWordCasing, false);
            cfg
        }
        "all-no-mixed-case" => {
            let mut cfg = build_config("all");
            cfg.rules.insert(RuleId::MixedCaseWord, false);
            cfg
        }
        other => panic!(
            "unknown config {other:?} (want default|all|all-no-spacing|all-no-adjacency|all-no-duplicate|all-no-casing|all-no-mixed-case)"
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(config_name)) = (args.first().map(String::as_str), args.get(1)) else {
        eprintln!(
            "usage: dhat_probe <testing|profile|warm-profile> <default|all|all-no-spacing|all-no-adjacency|all-no-duplicate|all-no-casing|all-no-mixed-case> [corpus-path]"
        );
        std::process::exit(2);
    };

    let path = args
        .get(2)
        .map_or_else(|| PathBuf::from(CORPUS_PATH), PathBuf::from);
    let bible = spike_bench::vref_io::load_corpus(&path);
    eprintln!("loaded {} verses from {}", bible.len(), path.display());

    let books = ssc_core::corpus::by_book(&bible);
    let bg = books
        .iter()
        .find(|g| g.slug == BOOK_CODE)
        .unwrap_or_else(|| panic!("book {BOOK_CODE} not present in corpus"));
    let base_keys: Vec<String> = bg.keys.iter().map(|k| k.to_string()).collect();
    let base_texts: Vec<String> = bg.texts.iter().map(|t| t.to_string()).collect();
    drop(books);

    let make_block = |suffix: &str| {
        let mut texts = base_texts.clone();
        texts[0].push_str(suffix);
        BookBlock {
            slug: BOOK_CODE.into(),
            keys: base_keys.clone(),
            texts,
        }
    };
    let block_a = make_block(" edited");
    let block_b = make_block(" edited twice");

    let cfg = build_config(config_name);
    eprintln!("config: {config_name}, mode: {mode}");

    match mode {
        "testing" => run_testing(&bible, &cfg, block_a, block_b),
        "profile" => run_profile(&bible, &cfg, block_a, block_b),
        "warm-profile" => run_warm_profile(&bible, &cfg, block_a, block_b),
        other => {
            eprintln!("unknown mode {other:?} (want testing|profile)");
            std::process::exit(2);
        }
    }
}

fn run_testing(bible: &Corpus, cfg: &Config, block_a: BookBlock, block_b: BookBlock) {
    let _profiler = dhat::Profiler::builder().testing().build();

    let mut galley = Galley::new(bible.clone(), None, cfg.clone());
    let seed_start = std::time::Instant::now();
    let _ = galley.analyze();
    eprintln!("cold seed: {:?}", seed_start.elapsed());

    let seed_heap = dhat::HeapStats::get();
    eprintln!(
        "after seed: total_blocks={} total_bytes={} curr_blocks={} curr_bytes={} max_blocks={} max_bytes={}",
        seed_heap.total_blocks,
        seed_heap.total_bytes,
        seed_heap.curr_blocks,
        seed_heap.curr_bytes,
        seed_heap.max_blocks,
        seed_heap.max_bytes,
    );

    let mut flip = false;
    let mut prev = seed_heap;

    for i in 0..WARM_ITERS {
        let block = if flip { block_a.clone() } else { block_b.clone() };
        flip = !flip;

        let iter_start = std::time::Instant::now();
        galley
            .update_book(block)
            .expect("valid complete-book replacement");
        let findings = galley.analyze();
        let elapsed = iter_start.elapsed();

        let now = dhat::HeapStats::get();
        eprintln!(
            "iter {i:2}: {elapsed:>10?}  d_blocks={:>8}  d_bytes={:>10}  curr_blocks={:>7} curr_bytes={:>10} max_blocks={:>7} max_bytes={:>10}  findings={}",
            now.total_blocks as i64 - prev.total_blocks as i64,
            now.total_bytes as i64 - prev.total_bytes as i64,
            now.curr_blocks,
            now.curr_bytes,
            now.max_blocks,
            now.max_bytes,
            findings.len(),
        );
        prev = now;
    }
}

/// Warm-only allocation attribution: seed the resident handle with the profiler
/// OFF, start it, then run a few warm iterations — so `dhat-heap.json`'s
/// backtraces are the warm analyze's own allocations, not the cold seed's
/// (which outnumber them by orders of magnitude and would bury them).
fn run_warm_profile(bible: &Corpus, cfg: &Config, block_a: BookBlock, block_b: BookBlock) {
    let mut galley = Galley::new(bible.clone(), None, cfg.clone());
    let seed_start = std::time::Instant::now();
    let _ = galley.analyze();
    eprintln!("cold seed (profiler off): {:?}", seed_start.elapsed());

    let _profiler = dhat::Profiler::new_heap();
    let mut flip = false;
    const PROFILE_ITERS: usize = 5;
    for _ in 0..PROFILE_ITERS {
        let block = if flip { block_a.clone() } else { block_b.clone() };
        flip = !flip;
        galley
            .update_book(block)
            .expect("valid complete-book replacement");
        std::hint::black_box(galley.analyze().len());
    }
    eprintln!("warm profile: {PROFILE_ITERS} iterations recorded");
}

fn run_profile(bible: &Corpus, cfg: &Config, block_a: BookBlock, block_b: BookBlock) {
    let _profiler = dhat::Profiler::new_heap();

    let mut galley = Galley::new(bible.clone(), None, cfg.clone());
    let seed_start = std::time::Instant::now();
    let _ = galley.analyze();
    eprintln!("cold seed: {:?}", seed_start.elapsed());

    let mut flip = false;
    // Small iteration count deliberately — dhat's per-allocation backtrace
    // capture is much slower than the plain allocator, and this is a
    // does-it-work probe, not a throughput measurement.
    const PROFILE_ITERS: usize = 5;
    let loop_start = std::time::Instant::now();
    for _ in 0..PROFILE_ITERS {
        let block = if flip { block_a.clone() } else { block_b.clone() };
        flip = !flip;
        galley
            .update_book(block)
            .expect("valid complete-book replacement");
        std::hint::black_box(galley.analyze().len());
    }
    let elapsed = loop_start.elapsed();
    eprintln!(
        "profile loop: {elapsed:?} total ({:?}/iter over {PROFILE_ITERS} iters, under dhat)",
        elapsed / PROFILE_ITERS as u32
    );
    // `_profiler` drops here at end of main, writing dhat-heap.json.
}
