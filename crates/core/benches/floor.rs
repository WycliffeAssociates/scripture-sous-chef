//! Criterion floor benches (`bench-probes` feature only): the fused walk's
//! per-verse substrate cost with zero rule listeners attached — tape build
//! alone, +tape-driven graphemes, +tokens and letter-folds, and all four
//! together (the real ceiling: every currently-shaped rule needs at least
//! one of these). Drives the exact same per-verse build every real listener
//! shares (`stream::drive_book`, via the `walk_floor` bench probe), so a
//! floor number can never silently drift from what `analyze.rs` measures —
//! subtract a tier here from an `analyze/*` number for that rule set's own
//! logic cost, independent of the substrate it rides on.
//!
//! `walk_floor`/`FloorNeeds` are NOT public API: they only exist behind
//! `bench-probes`, a feature no downstream consumer (galley/wasm) enables.
//! It exists solely so this bench binary can reach the walk's private
//! per-verse primitives without exposing them generally.
//!
//! Same corpora as `analyze.rs` (skipped with a notice if `corpora/` is
//! absent — it is gitignored): `full_bible`/`nt` from en_ulb, plus
//! `full_devanagari` (hi_ulb) for the heavier-script case.
//!
//! Run: `cargo bench -p ssc-core --features bench-probes --bench floor`

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ssc_core::corpus::by_book;
use ssc_core::key::parse_key;
use ssc_core::script::is_nt_book;
use ssc_core::{Corpus, FloorNeeds, walk_floor};

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::{corpus_path, load_corpus};

/// Resolve a corpus by id (e.g. `WA-en-ulb`) to its vref file under
/// `corpora/vref/` (ADR 0040). Returns `None` (bench skips) if absent.
fn corpus(id: &str) -> Option<Corpus> {
    let path = corpus_path(id);
    if !path.exists() {
        eprintln!("corpus '{id}' not present under corpora/vref — skipping its floor benches");
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

/// The four substrate tiers, in increasing order of what they force on.
/// `tape_graphemes` and `tape_tokens_folds` mirror the two shapes real rules
/// actually ask for (e.g. spacing needs only graphemes; casing needs only
/// tokens) — `all` is the ceiling no single rule reaches alone, but the
/// whole current rule set does collectively.
const TIERS: &[(&str, FloorNeeds)] = &[
    (
        "tape_only",
        FloorNeeds {
            tape: true,
            graphemes: false,
            tokens: false,
            folds: false,
        },
    ),
    (
        "tape_graphemes",
        FloorNeeds {
            tape: true,
            graphemes: true,
            tokens: false,
            folds: false,
        },
    ),
    (
        "tape_tokens",
        FloorNeeds {
            tape: true,
            graphemes: false,
            tokens: true,
            folds: false,
        },
    ),
    (
        "tape_tokens_folds",
        FloorNeeds {
            tape: true,
            graphemes: false,
            tokens: true,
            folds: true,
        },
    ),
    (
        "all",
        FloorNeeds {
            tape: true,
            graphemes: true,
            tokens: true,
            folds: true,
        },
    ),
];

fn bench_corpus(
    g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    corpus: &Corpus,
) {
    let books = by_book(corpus);
    g.throughput(Throughput::Elements(corpus.len() as u64));
    for (tier, needs) in TIERS.iter().copied() {
        g.bench_function(format!("{name}_{tier}"), |b| {
            b.iter(|| walk_floor(black_box(&books), needs))
        });
    }
}

fn bench_floor(c: &mut Criterion) {
    let mut g = c.benchmark_group("floor");
    g.sample_size(10);

    if let Some(bible) = corpus("WA-en-ulb") {
        let nt = filter_books(&bible, is_nt_book);
        bench_corpus(&mut g, "full_bible", &bible);
        bench_corpus(&mut g, "nt", &nt);
    }

    if let Some(dev) = corpus("WA-hi-ulb") {
        bench_corpus(&mut g, "full_devanagari", &dev);
    }

    g.finish();
}

criterion_group!(benches, bench_floor);
criterion_main!(benches);
