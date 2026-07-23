//! The editor's warm steady-state benchmark: a resident [`Galley`] holding the
//! whole corpus + prior + warm prep cache, then `update_book` + `analyze` — the
//! real keystroke path (ADR 0062; granularity-spine Phase A step 5).
//!
//! Moved here from `crates/core/benches/analyze.rs` so `ssc-core` no longer
//! dev-depends on `ssc-galley` (dependency-direction restore, Phase A step 6
//! closeout). The whole-corpus one-shot passes (`full_bible`, `nt`,
//! `full_devanagari`) stay in the core bench; only the resident-shell benches
//! live here.
//!
//! Serial under default features; rerun with `--features parallel` for the
//! native fan-out (ADR 0018/0042). Skipped with a notice if `corpora/` is
//! absent (it is gitignored).
//!
//! Run: `cargo bench -p ssc-galley --bench warm_edit`
//!
//! NOTE (criterion baseline continuity): this replaced the former
//! `analyze_stateful`-based `snapshot_edit_*`/`cached_edit_*` benches with the
//! resident `Galley` API; the `pre-spine` baselines for those names no longer
//! compare. The plan §13 warm ladder (spike-bench `warm_ladder_profile`) is the
//! cross-packet warm-path referee.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::{BookBlock, Config, Corpus};
use ssc_galley::Galley;

#[path = "../../core/dev/vref_io.rs"]
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

fn bench_warm_edit(c: &mut Criterion) {
    let Some(bible) = corpus("WA-en-ulb") else {
        return;
    };

    let mut g = c.benchmark_group("analyze");
    g.sample_size(10);

    // The editor's shipped steady state: a resident `Galley` holds the whole
    // corpus + prior + warm prep cache, and every edit runs the *complete*
    // whole-corpus call (no book-scoped echo). A warm `Galley` (seeded by one
    // analyze in setup, excluded from the measurement), then `update_book` +
    // `analyze` — the real keystroke path. The book spread bounds the range:
    // 3JN (~15 verses, floor), MAT (large), PSA (~2.5k verses, worst case).
    let cfg = Config::v1_defaults();
    let books = ssc_core::corpus::by_book(&bible);
    for code in ["3JN", "MAT", "PSA"] {
        let Some(bg) = books.iter().find(|g| g.slug == code) else {
            eprintln!("{code} not present in en_ulb — skipping its bench");
            continue;
        };
        // The edited replacement book: its first verse gets a suffix, supplied
        // as a complete-book `update_book` — the resident mutation verb.
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
                    // A warm resident Galley: one cold analyze warms both cache
                    // lanes + the prior. Excluded from the measurement.
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
    g.finish();
}

criterion_group!(benches, bench_warm_edit);
criterion_main!(benches);
