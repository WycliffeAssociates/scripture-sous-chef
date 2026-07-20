//! THROWAWAY microbenchmark — NOT part of the crate's shipped surface.
//!
//! Follow-up to `word_break_survey.rs` / `documentation/calibration/
//! 2026-07-17-word-break-fast-path-survey.md`, section (c). That report's
//! perf estimate was algebra, back-solved from two aggregate floor-bench
//! numbers (0.48 µs/verse average tokenize cost, 4.32% gate-trigger rate on
//! `WA-en-ulb`) — this measures the ASCII-gate cost directly instead.
//!
//! `unicode-segmentation`'s word iterator gates on a whole-string
//! `s.is_ascii()` check (`word.rs` ~973-976): a single non-ASCII scalar
//! anywhere routes the ENTIRE string onto the slow, table-driven Unicode
//! word-break path. This program:
//!
//!   1. Pulls real `WA-en-ulb` verses that contain **exactly one** non-ASCII
//!      scalar (909 such verses exist; overwhelmingly the em dash U+2014,
//!      which appears 1,795 times fleet-wide in this corpus but as the SOLE
//!      non-ASCII scalar in 909 individual verses).
//!   2. Builds an all-ASCII control for each by substituting that one scalar
//!      with its plain-ASCII typographic equivalent (em/en dash -> `-`,
//!      curly quotes -> `'`/`"`) — same verse, same word content, same length
//!      to within 1-2 bytes, differing only in whether the whole-string
//!      ASCII gate trips.
//!   3. Times `text.unicode_word_indices().count()` on both variants of each
//!      pair with a hand-rolled repeated-call microbenchmark (warmup + 5
//!      timed trials, median reported — no criterion harness/Cargo.toml
//!      change needed for a throwaway timing loop).
//!
//! Run: cargo run -p ssc-core --release --example word_break_ascii_gate_bench

use std::hint::black_box;
use std::time::Instant;

use unicode_segmentation::UnicodeSegmentation;

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::corpus_path;

/// Plain-ASCII typographic equivalent for the non-ASCII scalars actually
/// observed as the SOLE non-ASCII char in a `WA-en-ulb` verse (per the
/// companion survey's frequency table for this corpus: em dash 1795x, en
/// dash 1x, curly quotes a handful). Anything else is skipped rather than
/// guessed at — this bench only needs a representative sample, not every
/// verse.
fn ascii_equivalent(c: char) -> Option<char> {
    match c {
        '\u{2013}' | '\u{2014}' => Some('-'),  // en dash, em dash
        '\u{2018}' | '\u{2019}' => Some('\''), // curly single quotes
        '\u{201C}' | '\u{201D}' => Some('"'),  // curly double quotes
        _ => None,
    }
}

/// Fully drive the word iterator (the actual cost we're measuring) without
/// materializing tokens — matches what `token::tokenize_into` does, minus
/// the `Vec` push, which is identical overhead on both variants of a pair and
/// so doesn't matter for a differential measurement.
#[inline(never)]
fn drive(text: &str) -> usize {
    text.unicode_word_indices().count()
}

/// Median of 5 timed trials, `reps` calls each, nanoseconds per call.
/// `black_box` on both the input reference and the per-call result inside
/// the loop (the standard criterion-internal pattern) prevents the optimizer
/// from hoisting the loop-invariant computation out of the repeat loop.
fn median_ns_per_call(text: &str, reps: u32, warmup: u32) -> f64 {
    for _ in 0..warmup {
        black_box(drive(black_box(text)));
    }
    let mut trials = [0f64; 5];
    for t in trials.iter_mut() {
        let t0 = Instant::now();
        for _ in 0..reps {
            black_box(drive(black_box(text)));
        }
        let elapsed = t0.elapsed();
        *t = elapsed.as_nanos() as f64 / reps as f64;
    }
    trials.sort_by(|a, b| a.partial_cmp(b).unwrap());
    trials[2] // median of 5
}

struct Pair {
    key: String,
    len_bytes: usize,
    nonascii_char: char,
    ascii_text: String,
    nonascii_text: String,
}

fn main() {
    let path = corpus_path("WA-en-ulb");
    if !path.exists() {
        eprintln!("WA-en-ulb corpus not present under corpora/vref — nothing to bench");
        return;
    }
    let corpus = vref_io::load_corpus(&path);

    // Collect every verse with exactly one non-ASCII scalar and a known
    // ASCII equivalent for it.
    let mut candidates: Vec<Pair> = Vec::new();
    for (key, text) in corpus.keys().iter().zip(corpus.texts()) {
        let nonascii: Vec<char> = text.chars().filter(|c| !c.is_ascii()).collect();
        if nonascii.len() != 1 {
            continue;
        }
        let c = nonascii[0];
        let Some(repl) = ascii_equivalent(c) else {
            continue;
        };
        let ascii_text = text.replace(c, &repl.to_string());
        candidates.push(Pair {
            key: key.clone(),
            len_bytes: text.len(),
            nonascii_char: c,
            ascii_text,
            nonascii_text: text.clone(),
        });
    }
    eprintln!(
        "found {} WA-en-ulb verses with exactly 1 non-ASCII scalar + known ASCII equivalent",
        candidates.len()
    );

    // Spread the sample across the length distribution (short/medium/long)
    // rather than just taking the first N alphabetically — 30 verses, evenly
    // strided across the length-sorted list.
    candidates.sort_by_key(|p| p.len_bytes);
    let sample_n = 30usize.min(candidates.len());
    let stride = (candidates.len() / sample_n).max(1);
    let sample: Vec<&Pair> = candidates.iter().step_by(stride).take(sample_n).collect();

    const REPS: u32 = 20_000;
    const WARMUP: u32 = 2_000;

    println!(
        "{:<10} {:>6} {:>10} {:>10} {:>10} {:>8}  char",
        "verse", "bytes", "ascii_ns", "nonascii_ns", "delta_ns", "ratio"
    );
    let mut deltas = Vec::with_capacity(sample.len());
    let mut ratios = Vec::with_capacity(sample.len());
    for p in &sample {
        let ascii_ns = median_ns_per_call(&p.ascii_text, REPS, WARMUP);
        let nonascii_ns = median_ns_per_call(&p.nonascii_text, REPS, WARMUP);
        let delta = nonascii_ns - ascii_ns;
        let ratio = nonascii_ns / ascii_ns;
        deltas.push(delta);
        ratios.push(ratio);
        println!(
            "{:<10} {:>6} {:>10.1} {:>10.1} {:>10.1} {:>7.2}x  U+{:04X}",
            p.key, p.len_bytes, ascii_ns, nonascii_ns, delta, ratio, p.nonascii_char as u32
        );
    }

    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_delta: f64 = deltas.iter().sum::<f64>() / deltas.len() as f64;
    let mean_ratio: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let median_delta = deltas[deltas.len() / 2];
    let median_ratio = ratios[ratios.len() / 2];

    println!("\n--- summary across {} verse pairs ---", sample.len());
    println!("mean   delta_ns={mean_delta:.1}  mean   ratio={mean_ratio:.2}x");
    println!("median delta_ns={median_delta:.1}  median ratio={median_ratio:.2}x");
    println!(
        "min delta_ns={:.1}  max delta_ns={:.1}",
        deltas.first().unwrap(),
        deltas.last().unwrap()
    );
}
