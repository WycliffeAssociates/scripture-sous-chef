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
//! - `analyze/full_devanagari`— hi_ulb, the expensive-script case
//!
//! The editor's warm steady-state bench (`galley_warm_edit_{3JN,MAT,PSA}`:
//! resident `Galley` + `update_book` + `analyze`) lives in ssc-galley's own
//! bench — `cargo bench -p ssc-galley --bench warm_edit` — so ssc-core no longer
//! dev-depends on ssc-galley. The plan §13 warm ladder (spike-bench
//! `warm_ladder_profile`) remains the cross-packet warm-path referee.
//! - `proportionality/nt_vs_bible` — bem_reg vs en_ulb through the rule
//!
//! All serial under default features; rerun with `--features parallel` for
//! the native fan-out (ADR 0018/0042) — same benches, no mirror code.
//!
//! Run: `cargo bench -p ssc-core`
//! The wasm-side equivalent is `npm run bench:wasm` (same NT through
//! `analyze_vref`, marshaling included).
//! Baseline numbers: `documentation/calibration/2026-06-09-perf-baseline.md`
//!
//! For "what's the unavoidable substrate cost before any rule's own logic
//! runs" (as opposed to the collective cost of the shipped rules), see the
//! sibling `floor.rs` bench (`cargo bench -p ssc-core --features
//! bench-probes --bench floor`) — same corpora, zero rule listeners.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::config::ProportionalityConfig;
use ssc_core::key::parse_key;
use ssc_core::script::is_nt_book;
use ssc_core::{Config, Corpus, analyze};

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::{corpus_path, load_corpus};

/// Resolve a corpus by id (e.g. `WA-en-ulb`) to its vref file under
/// `corpora/vref/` (ADR 0040). Returns `None` (bench skips) if absent.
fn corpus(id: &str) -> Option<Corpus> {
    let path = corpus_path(id);
    if !path.exists() {
        eprintln!("corpus '{id}' not present under corpora/vref — skipping its benches");
        return None;
    }
    Some(load_corpus(&path))
}

/// Filter a corpus down to the verses whose book slug passes `keep`,
/// preserving presented order.
fn filter_books(corpus: &Corpus, mut keep: impl FnMut(&str) -> bool) -> Corpus {
    let mut keys = Vec::new();
    let mut texts = Vec::new();
    for (key, text) in corpus.keys().iter().zip(corpus.texts()) {
        let book = parse_key(key).expect("vref key").book;
        if keep(book) {
            keys.push(key.clone());
            texts.push(text.clone());
        }
    }
    Corpus::try_from_parts(keys, texts).unwrap()
}

fn bench_analyze(c: &mut Criterion) {
    let mut g = c.benchmark_group("analyze");
    // A full-Bible pass is ~1s; keep wall time sane without losing signal.
    g.sample_size(10);

    if let Some(bible) = corpus("WA-en-ulb") {
        let nt = filter_books(&bible, is_nt_book);

        g.throughput(Throughput::Elements(bible.len() as u64));
        g.bench_function("full_bible", |b| {
            b.iter(|| analyze(black_box(&bible), None))
        });

        g.throughput(Throughput::Elements(nt.len() as u64));
        g.bench_function("nt", |b| b.iter(|| analyze(black_box(&nt), None)));

        // The editor's warm steady state (`galley_warm_edit_{3JN,MAT,PSA}`:
        // resident `Galley` + `update_book` + `analyze`) moved to ssc-galley's
        // own bench so ssc-core no longer dev-depends on ssc-galley
        // (dependency-direction restore). Run it with
        // `cargo bench -p ssc-galley --bench warm_edit`. This core bench keeps
        // the whole-corpus one-shot passes below (`full_bible`, `nt`,
        // `full_devanagari`), which need no resident shell.
    }

    if let Some(dev) = corpus("WA-hi-ulb") {
        g.throughput(Throughput::Elements(dev.len() as u64));
        g.bench_function("full_devanagari", |b| {
            b.iter(|| analyze(black_box(&dev), None))
        });
    }

    g.finish();
}

// The old `nt_rayon` bench — a bench-local mirror of the per-verse rayon
// fan-out — is retired: the library now parallelizes both phases for real
// behind the `parallel` feature (ADR 0018/0042), so the parallel numbers
// come from `cargo bench -p ssc-core --features parallel` with no mirror
// to drift.

/// The counting-vs-emission split: how much of the stateful phase is `reduce`
/// (invalidated only by text edits, book-granular) vs `judge` (re-paid by any
/// complete-emission call). This is the number behind hash-derived counting's
/// payoff — a whole-corpus call that re-counts only the edited book saves ~the
/// reduce line and still pays the judge line. Tokens are `None` here (no shared
/// cache), so repeated-run tokenizes in both phases — a slight overcount of
/// each, same direction.
fn bench_phases(c: &mut Criterion) {
    let Some(bible) = corpus("WA-en-ulb") else {
        return;
    };
    let cfg = Config::v1_defaults();
    let books = ssc_core::corpus::by_book(&bible);
    let rules: Vec<_> = ssc_core::rule::stateful_rules(&cfg)
        .into_iter()
        .filter(|r| cfg.is_enabled(r.id()))
        .collect();

    let mut g = c.benchmark_group("phases");
    g.sample_size(10);
    g.bench_function("reduce_full", |b| {
        b.iter(|| {
            rules
                .iter()
                .map(|r| r.reduce(black_box(&books), None, None).0)
                .collect::<Vec<_>>()
        })
    });
    let merged: Vec<_> = rules
        .iter()
        .map(|r| r.reduce(&books, None, None).0)
        .collect();
    g.bench_function("judge_full", |b| {
        b.iter(|| {
            rules
                .iter()
                .zip(&merged)
                .map(|(r, m)| r.judge(black_box(m), black_box(&books), None, None))
                .collect::<Vec<_>>()
        })
    });
    g.finish();
}

fn bench_proportionality(c: &mut Criterion) {
    let (Some(target), Some(source)) = (corpus("WA-bem-reg"), corpus("WA-en-ulb")) else {
        return;
    };
    let cfg = ProportionalityConfig::default();

    let mut g = c.benchmark_group("proportionality");
    g.throughput(Throughput::Elements(target.len() as u64));
    g.bench_function("nt_vs_bible", |b| {
        b.iter(|| {
            ssc_core::signals::proportionality::length_ratio_findings(
                black_box(&target),
                Some(black_box(&source)),
                &cfg,
            )
        })
    });
    g.finish();
}

criterion_group!(benches, bench_analyze, bench_phases, bench_proportionality);
criterion_main!(benches);
