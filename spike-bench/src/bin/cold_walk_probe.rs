//! Cold-map decomposition: how much of a substrate's cold `map` column is the
//! SHARED raw walk (tokenization / grapheme segmentation over every verse)
//! versus the substrate's own per-token compute?
//!
//! Each observation substrate maps its chapters independently, so a cold seed
//! re-walks the corpus once per enabled substrate. This probe times the raw
//! walks alone — one full-corpus tokenize pass and one full-corpus grapheme
//! pass — giving the per-walk unit cost. `(walkers − 1) × unit` is the ceiling
//! a shared-prep product could recover from the cold drive table; everything a
//! substrate's map costs beyond the unit is its own compute, which sharing
//! cannot remove.
//!
//! Usage: cold_walk_probe <vref-file> [trials]

use std::hint::black_box;
use std::path::PathBuf;

use unicode_segmentation::UnicodeSegmentation;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().map(PathBuf::from) else {
        eprintln!("usage: cold_walk_probe <vref-file> [trials]");
        std::process::exit(2);
    };
    let trials: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);

    let bible = spike_bench::vref_io::load_corpus(&path);
    let texts: Vec<&str> = bible.texts().iter().map(String::as_str).collect();
    let bytes: usize = texts.iter().map(|t| t.len()).sum();
    eprintln!(
        "loaded {} verses, {:.1} MB from {}",
        texts.len(),
        bytes as f64 / 1e6,
        path.display()
    );

    // Token walk: the tokenizer every token-consuming substrate re-runs.
    let mut tok = Vec::with_capacity(trials);
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        let mut n = 0usize;
        for text in &texts {
            n += black_box(ssc_core::token::tokenize(text)).len();
        }
        tok.push(t0.elapsed());
        black_box(n);
    }

    // Grapheme walk: the segmentation pass grapheme-consuming substrates re-run.
    let mut gr = Vec::with_capacity(trials);
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        let mut n = 0usize;
        for text in &texts {
            n += black_box(text.graphemes(true).count());
        }
        gr.push(t0.elapsed());
        black_box(n);
    }

    // Plain byte scan: the floor — what any walk pays just to stream the text.
    let mut scan = Vec::with_capacity(trials);
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        let mut n = 0usize;
        for text in &texts {
            n += black_box(text.bytes().filter(|b| *b == b' ').count());
        }
        scan.push(t0.elapsed());
        black_box(n);
    }

    println!("one full-corpus token walk:    {:?} (median of {trials})", spike_bench::median(&mut tok));
    println!("one full-corpus grapheme walk: {:?} (median of {trials})", spike_bench::median(&mut gr));
    println!("one full-corpus byte scan:     {:?} (median of {trials})", spike_bench::median(&mut scan));
}
