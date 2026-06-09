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
//! - `analyze/nt_devanagari`— bap-x-rai_reg, the expensive-script case
//! - `proportionality/nt_vs_bible` — bem_reg vs en_ulb through the rule
//!
//! Run: `cargo bench -p ssc-core`
//! Baseline numbers: `documentation/calibration/2026-06-09-perf-baseline.md`

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::config::ProportionalityConfig;
use ssc_core::rule::ProjectRule;
use ssc_core::script::is_nt_book;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::{VerseMap, analyze};

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
    }

    if let Some(nt_dev) = corpus("bap-x-rai_reg") {
        g.throughput(Throughput::Elements(nt_dev.len() as u64));
        g.bench_function("nt_devanagari", |b| {
            b.iter(|| analyze(black_box(&nt_dev), None))
        });
    }

    g.finish();
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
