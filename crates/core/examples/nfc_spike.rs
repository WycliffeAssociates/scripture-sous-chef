//! THROWAWAY SPIKE — measure the cost of `unicode_normalization::is_nfc`
//! over real corpora, to decide crate-vs-vendored-table for the proposed
//! `uni.mixed-normalization` deterministic check. Delete after adjudication.
//!
//! Run: cargo run --release -p ssc-core --example nfc_spike -- WA-en-ulb WA-hi-ulb ...

use std::hint::black_box;
use std::time::Instant;

use unicode_normalization::is_nfc;

#[path = "../dev/vref_io.rs"]
mod vref_io;
use vref_io::{corpus_path, load_corpus};

fn main() {
    let ids: Vec<String> = {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.is_empty() {
            // Spread of scripts: ASCII / Devanagari / Ge'ez / Bengali /
            // Assamese / an African "reg" corpus.
            ["WA-en-ulb", "WA-hi-ulb", "WA-am-ulb", "WA-bn-ulb", "WA-as-ulb", "WA-bem-reg"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            args
        }
    };

    println!(
        "{:<16} {:>7} {:>12} {:>10} {:>9} {:>9} {:>10}",
        "corpus", "verses", "chars", "non-nfc", "ms/pass", "ns/verse", "ns/char"
    );

    for id in &ids {
        let path = corpus_path(id);
        if !path.exists() {
            println!("{id:<16} (absent — skipped)");
            continue;
        }
        let map = load_corpus(&path);
        let verses = map.len();
        let chars: usize = map.texts().iter().map(|t| t.chars().count()).sum();

        // How many verses are NOT already NFC (the check firing).
        let non_nfc = map.texts().iter().filter(|t| !is_nfc(t)).count();

        // Warm up, then time enough passes to total ~0.5s of work.
        let one = |map: &ssc_core::Corpus| {
            let mut acc = 0usize;
            for t in map.texts() {
                if is_nfc(black_box(t)) {
                    acc += 1;
                }
            }
            black_box(acc)
        };
        let _ = one(&map);

        let mut passes = 0u32;
        let start = Instant::now();
        while start.elapsed().as_millis() < 500 {
            black_box(one(&map));
            passes += 1;
        }
        let elapsed = start.elapsed();
        let per_pass_ns = elapsed.as_nanos() as f64 / passes as f64;

        println!(
            "{id:<16} {verses:>7} {chars:>12} {non_nfc:>10} {:>9.3} {:>9.1} {:>10.2}",
            per_pass_ns / 1e6,
            per_pass_ns / verses as f64,
            per_pass_ns / chars.max(1) as f64,
        );
    }
}
