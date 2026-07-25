//! Warm incremental analyze profile — decomposes the ADR 0062 warm ladder
//! ("5.2–18.9 ms warm whole-corpus re-analyze", the `cached_edit_*` criterion
//! benches) so samply can show what those milliseconds actually are.
//!
//! Mirrors `crates/core/benches/analyze.rs`'s `galley_warm_edit_*` shape:
//! a resident `Galley` (`Config::v1_defaults()` or all-on) driven through its
//! real steady state — `update_book` + `analyze` on a warm handle. Two
//! deliberate points:
//! - the resident `Galley` chains its prior + prep cache across iterations
//!   automatically (that IS its steady state), and
//! - the edited book's text alternates between two variants, so every
//!   iteration's content hash genuinely mismatches the cached entry and the
//!   edited book re-walks + re-tallies for real (the cache holds one hash per
//!   slug, so the "other" variant is never a stale hit).
//!
//! Usage:
//!   warm_ladder_profile <vref-file> <book-slug> [--config default|all] [--profile <iters>]
//!
//! `--config all` enables every rule (same construction as the calibrate
//! oracle's `all` config: `v1_defaults` + insert true for every `RuleId`);
//! default is `Config::v1_defaults()`.
//!
//! Wall-clock mode runs 200 trials and prints median per-call time (should
//! land on the ADR 0062 ladder: ~5.2 ms for 3JN, ~18.9 ms for PSA). Profile
//! mode runs the same closure `iters` times for samply to attach to — plain
//! CLI args, no env vars (see spike-bench/README.md for why).

use std::path::PathBuf;

use spike_bench::{profile_loop, variance_note};
use ssc_core::{BookBlock, Config, RuleId};
use ssc_galley::Galley;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(code)) = (args.first().map(PathBuf::from), args.get(1)) else {
        eprintln!("usage: warm_ladder_profile <vref-file> <book-slug> [--profile <iters>]");
        std::process::exit(2);
    };
    let profile_iters: Option<usize> = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());
    let config_name = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map_or("default", String::as_str);

    let bible = spike_bench::vref_io::load_corpus(&path);
    eprintln!("loaded {} verses from {}", bible.len(), path.display());

    // The edited book's own keys/texts (its contiguous run in the corpus).
    let books = ssc_core::corpus::by_book(&bible);
    let bg = books
        .iter()
        .find(|g| g.slug == *code)
        .unwrap_or_else(|| panic!("book {code} not present in corpus"));
    let base_keys: Vec<String> = bg.keys.iter().map(|k| k.to_string()).collect();
    let base_texts: Vec<String> = bg.texts.iter().map(|t| t.to_string()).collect();
    drop(books);

    // Two complete-book replacements differing only in the first verse — same
    // edit shape as the bench (`push_str` on one verse), two variants so
    // alternating them forces a fresh content hash every call.
    let make_block = |suffix: &str| {
        let mut texts = base_texts.clone();
        texts[0].push_str(suffix);
        BookBlock {
            slug: code.as_str().into(),
            keys: base_keys.clone(),
            texts,
        }
    };
    let variants: usize = args
        .iter()
        .position(|a| a == "--variants")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let distinct = args.iter().any(|a| a == "--distinct-variants");
    let blocks: Vec<BookBlock> = (0..variants.max(2))
        .map(|i| {
            if distinct {
                // Each variant introduces a DIFFERENT word type, so every
                // iteration presents a genuinely different word aggregate. The
                // default "!"-repeat variants differ only in trailing
                // punctuation, and for a word-tallying rule that makes variants
                // 2 and 3 the SAME aggregate — enough for a two-entry
                // content-keyed model memo to hit on every warm iteration.
                make_block(&format!(" editedx{i}zz"))
            } else {
                make_block(&format!(" edited{}", "!".repeat(i)))
            }
        })
        .collect();
    let block_a = blocks[0].clone();
    let block_b = blocks[1].clone();

    // Same construction as the calibrate oracle's configs (`oracle_config` in
    // crates/core/examples/calibrate/oracle.rs): "all" = v1 defaults with
    // every rule switched on.
    let cfg = match config_name {
        "default" => Config::v1_defaults(),
        "all" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg
        }
        // Every rule on with only ONE casing consumer. Paired with "all", this
        // isolates the cost of a second per-site emit pass: the pre-substrate
        // engine judged and walked the sites once PER RULE, so dropping one rule
        // drops one whole pass; the substrate engine emits both consumers in a
        // single pass, so dropping one changes almost nothing. The difference
        // between those two differences is the decomposition.
        "all-pos-only" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg.rules.insert(RuleId::InconsistentWordCasing, false);
            cfg
        }
        // Every rule on EXCEPT the two casing consumers — paired with "all", the
        // difference is casing's whole warm contribution.
        "all-no-casing" => {
            let mut cfg = Config::v1_defaults();
            for &id in RuleId::ALL {
                cfg.rules.insert(id, true);
            }
            cfg.rules.insert(RuleId::SentenceInitialLowercase, false);
            cfg.rules.insert(RuleId::InconsistentWordCasing, false);
            cfg
        }
        other => panic!("unknown config {other:?} (want default|all|all-pos-only|all-no-casing)"),
    };
    eprintln!("config: {config_name}");
    // Cold seed: the resident Galley's first analyze warms the prior + both
    // cache lanes for every book. Excluded from the profiled/timed loop.
    let mut galley = Galley::new(bible.clone(), None, cfg.clone());
    let seed_start = std::time::Instant::now();
    let _ = galley.analyze();
    eprintln!("cold seed: {:?}", seed_start.elapsed());

    let mut flip = false;
    let do_work = || {
        flip = !flip;
        let block = if flip { block_a.clone() } else { block_b.clone() };
        galley
            .update_book(block)
            .expect("valid complete-book replacement");
        galley.analyze().len()
    };

    if let Some(iters) = profile_iters {
        eprintln!("profile mode: {iters} warm iterations of edited book {code} (attach samply)");
        let loop_start = std::time::Instant::now();
        profile_loop(iters, do_work);
        let elapsed = loop_start.elapsed();
        eprintln!(
            "loop wall time: {elapsed:?} ({:?}/iter over {iters} iters)",
            elapsed / iters as u32
        );
        return;
    }

    // Batch count for the §13 protocol (default 1; the gate script passes 5).
    let batches: usize = args
        .iter()
        .position(|a| a == "--batches")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let trials: usize = args
        .iter()
        .position(|a| a == "--trials")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    // Decompose each warm iteration into update_book (whole-corpus layout
    // rebuild + prior/prep bookkeeping) and analyze, and analyze further into
    // the map/reduce/judge phase split (`ssc_core::bench`, bench-probes only).
    let mut rot = 0usize;
    let mut batch_totals: Vec<std::time::Duration> = Vec::new();
    for b in 0..batches {
        let mut total = Vec::with_capacity(trials);
        let mut upd = Vec::with_capacity(trials);
        let mut ana = Vec::with_capacity(trials);
        let mut map = Vec::with_capacity(trials);
        let mut red = Vec::with_capacity(trials);
        let mut jud = Vec::with_capacity(trials);
        let mut findings = 0usize;
        for _ in 0..trials {
            rot = (rot + 1) % blocks.len();
            let block = blocks[rot].clone();
            let t0 = std::time::Instant::now();
            galley
                .update_book(block)
                .expect("valid complete-book replacement");
            let t1 = std::time::Instant::now();
            findings = galley.analyze().len();
            let t2 = std::time::Instant::now();
            let ph = ssc_core::bench::last();
            total.push(t2 - t0);
            upd.push(t1 - t0);
            ana.push(t2 - t1);
            map.push(ph.map);
            red.push(ph.reduce);
            jud.push(ph.judge);
        }
        let med = |v: &mut Vec<std::time::Duration>| spike_bench::median(v);
        let bt = med(&mut total);
        batch_totals.push(bt);
        println!(
            "batch {b}/{batches} {code} {config_name}: total {:?} | update_book {:?} | analyze {:?} \
             (map {:?} reduce {:?} judge {:?}) | {findings} findings ({})",
            bt,
            med(&mut upd),
            med(&mut ana),
            med(&mut map),
            med(&mut red),
            med(&mut jud),
            variance_note(&total),
        );
    }
    if batches > 1 {
        batch_totals.sort();
        println!(
            "median-of-medians {code} {config_name} over {batches} batches: {:?}",
            batch_totals[batches / 2]
        );
    }
}
