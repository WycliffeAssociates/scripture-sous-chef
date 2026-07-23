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
//! - `analyze/galley_warm_edit_{3JN,MAT,PSA}` — the editor's steady state: a
//!   warm resident `Galley` (seeded by one analyze in setup), then
//!   `update_book` + `analyze` — a complete-book edit + whole-corpus warm
//!   re-analyze. All books hash, the edited one re-counts, clean books reuse
//!   both cache lanes, emission is global. (Replaced the former
//!   `snapshot_edit_*`/`cached_edit_*` `analyze_stateful` benches at
//!   granularity-spine Phase A step 5; §13 warm ladder is the referee.)
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
//!
//! For "what's the unavoidable substrate cost before any rule's own logic
//! runs" (as opposed to the collective cost of the shipped rules), see the
//! sibling `floor.rs` bench (`cargo bench -p ssc-core --features
//! bench-probes --bench floor`) — same corpora, zero rule listeners.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::config::ProportionalityConfig;
use ssc_core::key::parse_key;
use ssc_core::rule::StatefulRule;
use ssc_core::script::is_nt_book;
use ssc_core::signals::proportionality::ProjectLengthRatio;
use ssc_core::{BookBlock, Config, Corpus, analyze};
use ssc_galley::Galley;

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

        // The editor's shipped steady state (ADR 0062; granularity-spine Phase A
        // step 5): a resident `Galley` holds the whole corpus + prior + warm
        // prep cache, and every edit runs the *complete* whole-corpus call —
        // there is no book-scoped "echo" any more. So the bench models exactly
        // that: a warm `Galley` (seeded by one analyze in setup, excluded from
        // the measurement), then `update_book` + `analyze` — the real keystroke
        // path. The book spread bounds the range: 3JN (~15 verses, floor), MAT
        // (large), PSA (~2.5k verses, worst case).
        //
        // NOTE (criterion baseline continuity): this replaces the former
        // `analyze_stateful`-based `snapshot_edit_*`/`cached_edit_*` benches
        // with the resident `Galley` API. The `pre-spine` criterion baselines
        // for those names no longer compare; the plan §13 warm ladder
        // (spike-bench `warm_ladder_profile`) is the cross-packet referee.
        let cfg = Config::v1_defaults();
        let books = ssc_core::corpus::by_book(&bible);
        for code in ["3JN", "MAT", "PSA"] {
            let Some(bg) = books.iter().find(|g| g.slug == code) else {
                eprintln!("{code} not present in en_ulb — skipping its bench");
                continue;
            };
            // The edited replacement book: its first verse gets a suffix (the
            // same one-verse edit shape the old benches used), supplied as a
            // complete-book `update_book` — the resident mutation verb.
            let keys: Vec<String> = bg.keys.iter().map(|k| k.to_string()).collect();
            let mut texts: Vec<String> = bg.texts.iter().map(|t| t.to_string()).collect();
            texts[0].push_str(" edited");
            let edited_block = BookBlock {
                slug: code.into(),
                keys,
                texts,
            };

            g.throughput(Throughput::Elements(bible.len() as u64));
            g.bench_function(format!("galley_warm_edit_{code}"), |b| {
                b.iter_batched(
                    || {
                        // A warm resident Galley: one cold analyze warms both
                        // cache lanes + the prior. Excluded from the measurement.
                        let mut galley = Galley::new(bible.clone(), None, cfg.clone());
                        let _ = galley.analyze();
                        galley
                    },
                    |mut galley| {
                        galley
                            .update_book(black_box(edited_block.clone()))
                            .expect("valid complete-book replacement");
                        black_box(galley.analyze())
                    },
                    BatchSize::LargeInput,
                )
            });
        }
        drop(books);
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
    let rule = ProjectLengthRatio {
        cfg: ProportionalityConfig::default(),
    };

    let mut g = c.benchmark_group("proportionality");
    g.throughput(Throughput::Elements(target.len() as u64));
    let books = ssc_core::corpus::by_book(&target);
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
