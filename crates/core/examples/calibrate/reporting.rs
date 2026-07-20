//! Adjacent utility reports (timing, census dry-run). These reuse `oracle`'s
//! corpus-loading/config plumbing but aren't part of the byte-identical gate
//! contract — free to change shape without invalidating a pinned baseline.

use std::collections::BTreeMap;
use std::path::Path;

use ssc_core::{Config, analyze_with_config};

use crate::oracle::{OracleScope, oracle_config, oracle_files, oracle_source};
use crate::vref_io::load_corpus;

pub fn time_configs(path: &Path) {
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

pub fn census_single(path: &Path) {
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

pub fn census_fleet(dir: &Path) {
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
