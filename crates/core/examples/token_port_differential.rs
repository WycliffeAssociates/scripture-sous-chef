//! THROWAWAY verification — NOT part of the crate's shipped surface.
//!
//! Step 4 sanity check for ADR 0064: does the PORTED production
//! `ssc_core::token::tokenize` (not the standalone prototype, which has its
//! own independent copy of the same logic) still match
//! `unicode-segmentation`'s own tokenizer exactly, across the full
//! 1,504-corpus fleet? This calls the real public API directly.
//!
//! Run: cargo run -p ssc-core --release --example token_port_differential

use std::path::Path;

use unicode_segmentation::UnicodeSegmentation;

#[path = "../dev/vref_io.rs"]
mod vref_io;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpora_dir = manifest_dir.join("../../corpora/vref");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&corpora_dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", corpora_dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    eprintln!(
        "scanning {} corpora (full fleet) via the REAL ssc_core::token::tokenize",
        files.len()
    );

    let mut total_verses = 0u64;
    let mut mismatches = 0u64;
    let mut examples: Vec<(String, String, String)> = Vec::new();

    for path in &files {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let corpus = vref_io::load_corpus(path);
        for (key, text) in corpus.keys().iter().zip(corpus.texts()) {
            total_verses += 1;
            let mine: Vec<(u32, u32)> = ssc_core::token::tokenize(text)
                .iter()
                .map(|t| (t.span.start, t.span.end))
                .collect();
            let real: Vec<(u32, u32)> = text
                .unicode_word_indices()
                .map(|(s, w)| (s as u32, (s + w.len()) as u32))
                .collect();
            if mine != real {
                mismatches += 1;
                if examples.len() < 10 {
                    examples.push((id.clone(), key.clone(), text.clone()));
                }
            }
        }
    }

    println!("total verses: {total_verses}");
    println!(
        "mismatches vs unicode-segmentation: {mismatches} ({:.8}%)",
        100.0 * mismatches as f64 / total_verses as f64
    );
    for (id, key, text) in &examples {
        println!("{id} {key}: {text:?}");
    }
}
