//! Criterion benchmarks for the collective cost of the shipped rule set.
//!
//! Serial, single-thread, real corpora: the point is to know what every
//! enabled rule costs together at editor-relevant scales *before* the
//! rule set grows, and to give ADR 0011's "escalate only on measurement"
//! something to measure against. (Parallelism would only divide these
//! numbers; the per-verse loop is embarrassingly parallel.)
//!
//! Benches (skipped with a notice if `corpora/` is absent — it is
//! gitignored):
//! - `analyze/full_bible`   — en_ulb, ~31k verses, `Config::v1_defaults()`
//! - `analyze/nt`           — en_ulb NT subset, ~7.9k verses
//! - `analyze/nt_rayon`     — same NT, per-verse loop fanned out with
//!   rayon **in the bench only** — what a native (non-wasm) consumer
//!   could buy by parallelising around the library; core stays serial
//! - `analyze/nt_devanagari`— bap-x-rai_reg, the expensive-script case
//! - `proportionality/nt_vs_bible` — bem_reg vs en_ulb through the rule
//!
//! Run: `cargo bench -p ssc-core`
//! The wasm-side equivalent is `npm run bench:wasm` (same NT through
//! `analyze_vref`, marshaling included).
//! Baseline numbers: `documentation/calibration/2026-06-09-perf-baseline.md`

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rayon::prelude::*;
use ssc_core::config::ProportionalityConfig;
use ssc_core::rule::{ProjectRule, per_verse_rules};
use ssc_core::script::is_nt_book;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::{Config, Finding, VerseMap, analyze};

#[path = "../dev/usfm_naive.rs"]
mod usfm_naive;
use usfm_naive::load_corpus;

fn corpus(name: &str) -> Option<VerseMap> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpora")
        .join(name);
    if !dir.is_dir() {
        eprintln!("corpora/{name} not present — skipping its benches");
        return None;
    }
    Some(load_corpus(&dir))
}

fn bench_analyze(c: &mut Criterion) {
    let mut g = c.benchmark_group("analyze");
    // A full-Bible pass is ~1s; keep wall time sane without losing signal.
    g.sample_size(10);

    if let Some(bible) = corpus("en_ulb") {
        let nt: VerseMap = bible
            .iter()
            .filter(|(sid, _)| is_nt_book(sid.book.as_str()))
            .map(|(s, t)| (*s, t.clone()))
            .collect();

        g.throughput(Throughput::Elements(bible.len() as u64));
        g.bench_function("full_bible", |b| {
            b.iter(|| analyze(black_box(&bible), None))
        });

        g.throughput(Throughput::Elements(nt.len() as u64));
        g.bench_function("nt", |b| b.iter(|| analyze(black_box(&nt), None)));

        g.throughput(Throughput::Elements(nt.len() as u64));
        g.bench_function("nt_rayon", |b| {
            b.iter(|| analyze_par(black_box(&nt), &Config::v1_defaults()))
        });
    }

    if let Some(nt_dev) = corpus("bap-x-rai_reg") {
        g.throughput(Throughput::Elements(nt_dev.len() as u64));
        g.bench_function("nt_devanagari", |b| {
            b.iter(|| analyze(black_box(&nt_dev), None))
        });
    }

    g.finish();
}

/// What `analyze` would look like with the per-verse loop fanned out
/// over rayon. Lives in the bench, not the library: the editor's wasm
/// target is single-threaded, and serial Mode A is already inside every
/// budget — this exists purely to quantify the native headroom (finding
/// order differs; per-verse rules are `Sync` by contract).
fn analyze_par(target: &VerseMap, config: &Config) -> Vec<Finding> {
    let rules: Vec<_> = per_verse_rules()
        .into_iter()
        .filter(|r| config.is_enabled(r.id()))
        .collect();
    target
        .par_iter()
        .flat_map_iter(|(&sid, text)| {
            rules.iter().flat_map(move |r| {
                let code = r.id();
                let severity = r.severity();
                r.check(text).into_iter().map(move |range| Finding {
                    sid,
                    code,
                    severity,
                    range,
                    score: None,
                    args: None,
                })
            })
        })
        .collect()
}

fn bench_proportionality(c: &mut Criterion) {
    let (Some(target), Some(source)) = (corpus("bem_reg"), corpus("en_ulb")) else {
        return;
    };
    let rule = ProjectLengthRatio {
        cfg: ProportionalityConfig::default(),
    };

    let mut g = c.benchmark_group("proportionality");
    g.throughput(Throughput::Elements(target.len() as u64));
    g.bench_function("nt_vs_bible", |b| {
        b.iter(|| rule.check(black_box(&target), Some(black_box(&source))))
    });
    g.finish();
}

criterion_group!(benches, bench_analyze, bench_proportionality);
criterion_main!(benches);
