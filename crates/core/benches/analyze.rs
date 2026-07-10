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
//! - `analyze/incremental_edit_{3JN,MAT,PSA}` — the local-echo call: cached
//!   corpus `Stats` as prior, only the edited book supplied (ADR 0017),
//!   across the book-size spread (floor / large / largest)
//! - `analyze/changed_edit_{3JN,MAT,PSA}` — the complete-snapshot call
//!   (ADR 0043): whole corpus + prior + `changed=[book]`; counting is
//!   book-scoped, emission is global
//! - `analyze/full_devanagari`— hi_ulb, the expensive-script case
//! - `proportionality/nt_vs_bible` — bem_reg vs en_ulb through the rule
//!
//! All serial under default features; rerun with `--features parallel` for
//! the native fan-out (ADR 0018/0042) — same benches, no mirror code.
//!
//! Run: `cargo bench -p ssc-core`
//! The wasm-side equivalent is `npm run bench:wasm` (same NT through
//! `analyze_vref`, marshaling included).
//! Baseline numbers: `documentation/calibration/2026-06-09-perf-baseline.md`

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::config::ProportionalityConfig;
use ssc_core::rule::StatefulRule;
use ssc_core::script::is_nt_book;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::{Config, VerseMap, analyze, analyze_stateful};

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::{corpus_path, load_corpus};

/// Resolve a corpus by id (e.g. `WA-en-ulb`) to its vref file under
/// `corpora/vref/` (ADR 0040). Returns `None` (bench skips) if absent.
fn corpus(id: &str) -> Option<VerseMap> {
    let path = corpus_path(id);
    if !path.exists() {
        eprintln!("corpus '{id}' not present under corpora/vref — skipping its benches");
        return None;
    }
    Some(load_corpus(&path))
}

fn bench_analyze(c: &mut Criterion) {
    let mut g = c.benchmark_group("analyze");
    // A full-Bible pass is ~1s; keep wall time sane without losing signal.
    g.sample_size(10);

    if let Some(bible) = corpus("WA-en-ulb") {
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

        // The editor's steady state (ADR 0017): the full corpus was analyzed
        // once and its `Stats` cached; a chapter edit re-supplies its whole
        // book (book granularity is the supersede unit) with that prior. This
        // is the incremental cost a keystroke-adjacent consumer pays —
        // measured without the prior clone (`iter_batched` setup), since the
        // shell hands the value back rather than rebuilding it. The spread of
        // book sizes bounds the range: 3JN (~15 verses, floor), MAT (large),
        // PSA (~2.5k verses, the worst case).
        let cfg = Config::v1_defaults();
        let (_, cached) = analyze_stateful(&bible, None, &cfg, None, None);
        for code in ["3JN", "MAT", "PSA"] {
            let mut book: VerseMap = bible
                .iter()
                .filter(|(sid, _)| sid.book.as_str() == code)
                .map(|(s, t)| (*s, t.clone()))
                .collect();
            let Some(&first) = book.keys().next() else {
                eprintln!("{code} not present in en_ulb — skipping its bench");
                continue;
            };
            book.get_mut(&first).unwrap().push_str(" edited");
            g.throughput(Throughput::Elements(book.len() as u64));
            g.bench_function(format!("incremental_edit_{code}"), |b| {
                b.iter_batched(
                    || cached.clone(),
                    |prior| {
                        analyze_stateful(black_box(&book), None, black_box(&cfg), Some(prior), None)
                    },
                    BatchSize::LargeInput,
                )
            });

            // The complete-snapshot call (ADR 0043): whole corpus supplied,
            // `changed` names the edited book — only it re-counts, findings
            // cover everything (a tipped convention re-emits in every book,
            // this same call). The payoff vs `full_bible` is the counting
            // saved; vs `incremental_edit_*` it buys global consistency.
            let mut edited = bible.clone();
            edited.get_mut(&first).unwrap().push_str(" edited");
            let changed = [ssc_core::BookId::from_str(code).unwrap()];
            g.throughput(Throughput::Elements(edited.len() as u64));
            g.bench_function(format!("changed_edit_{code}"), |b| {
                b.iter_batched(
                    || cached.clone(),
                    |prior| {
                        analyze_stateful(
                            black_box(&edited),
                            None,
                            black_box(&cfg),
                            Some(prior),
                            Some(black_box(&changed)),
                        )
                    },
                    BatchSize::LargeInput,
                )
            });
        }
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

/// The counting-vs-emission split (ADR 0043 territory): how much of the
/// stateful phase is `reduce` (invalidated only by text edits, book-granular)
/// vs `judge` (re-paid by any complete-emission call). This is the number
/// that prices a hypothetical `changed: &[BookId]` argument — a whole-corpus
/// call that re-counts one book saves ~the reduce line and still pays the
/// judge line. Tokens are `None` here (no shared cache), so repeated-run
/// tokenizes in both phases — a slight overcount of each, same direction.
fn bench_phases(c: &mut Criterion) {
    let Some(bible) = corpus("WA-en-ulb") else {
        return;
    };
    let cfg = Config::v1_defaults();
    let books = ssc_core::verse::by_book(&bible);
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
    let rule = ProjectLengthRatio {
        cfg: ProportionalityConfig::default(),
    };

    let mut g = c.benchmark_group("proportionality");
    g.throughput(Throughput::Elements(target.len() as u64));
    let books = ssc_core::verse::by_book(&target);
    g.bench_function("nt_vs_bible", |b| {
        b.iter(|| {
            rule.judge(
                &rule
                    .reduce(black_box(&books), Some(black_box(&source)), None)
                    .0,
                black_box(&books),
                None,
                None,
            )
        })
    });
    g.finish();
}

criterion_group!(benches, bench_analyze, bench_phases, bench_proportionality);
criterion_main!(benches);
