//! Fleet field-width probe for WP7a's packed site records.
//!
//! The 6-byte `LowerSite` (`key: u16`, `verse: u16`, `ord: u8`, `pos: u8`) is
//! only sound if the fleet's real segmentation stays inside those ceilings, and
//! no corpus statistic predicts word counts usefully (the 2026-07-24 interner
//! spike measured an 8.7x spread in types-per-verse across two corpora). So the
//! whole vref fleet is measured, per corpus, and the maxima reported.
//!
//! Also reports the widest UAX #29 token count per verse line — the ordinal a
//! word-keyed rule that does NOT hyphen-merge (mixed-case) would index by.
//!
//!   cargo run --release --bin field_extents -- corpora/vref

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: field_extents <corpora/vref>");
        std::process::exit(2);
    });

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read corpora dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    eprintln!("{} corpora", files.len());

    let mut worst_words = (0usize, String::new());
    let mut worst_types = (0usize, String::new());
    let mut worst_classes = (0usize, String::new());
    let mut worst_tokens = (0usize, String::new());
    let mut worst_chapter_tokens = (0usize, String::new());
    let mut worst_shape_count = (0usize, String::new());

    for (i, path) in files.iter().enumerate() {
        if i % 200 == 0 {
            eprintln!("{i}/{}", files.len());
        }
        let name = path.file_stem().unwrap().to_string_lossy().to_string();

        let corpus = spike_bench::vref_io::load_corpus(path);
        let (words, types, classes) = ssc_core::signals::casing::field_extent_probe(&corpus);
        if words > worst_words.0 {
            worst_words = (words, name.clone());
        }
        if types > worst_types.0 {
            worst_types = (types, name.clone());
        }
        if classes > worst_classes.0 {
            worst_classes = (classes, name.clone());
        }

        // Mixed-case's per-chapter shape table (WP7b item 4): how wide one
        // chapter's counters actually have to be.
        let (ch_tokens, shape_count) =
            ssc_core::signals::mixed_case::chapter_extent_probe(&corpus);
        if ch_tokens > worst_chapter_tokens.0 {
            worst_chapter_tokens = (ch_tokens, name.clone());
        }
        if shape_count > worst_shape_count.0 {
            worst_shape_count = (shape_count, name.clone());
        }

        // Plain UAX #29 token counts, straight off the file's verse lines —
        // mixed-case's token unit, which does not hyphen-merge.
        let raw = std::fs::read_to_string(path).expect("read corpus file");
        for line in raw.lines() {
            let n = ssc_core::token::tokenize(line).len();
            if n > worst_tokens.0 {
                worst_tokens = (n, name.clone());
            }
        }
    }

    println!("max compound words per verse : {:>7} ({})", worst_words.0, worst_words.1);
    println!("max distinct types / chapter : {:>7} ({})", worst_types.0, worst_types.1);
    println!("max distinct classes / chap  : {:>7} ({})", worst_classes.0, worst_classes.1);
    println!("max UAX tokens per verse     : {:>7} ({})", worst_tokens.0, worst_tokens.1);
    println!(
        "max letter tokens / chapter  : {:>7} ({})",
        worst_chapter_tokens.0, worst_chapter_tokens.1
    );
    println!(
        "max one-shape count / chapter: {:>7} ({})",
        worst_shape_count.0, worst_shape_count.1
    );
}
