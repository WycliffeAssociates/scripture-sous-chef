//! What a shared per-chapter prep product would actually cost and save.
//!
//! `cold_walk_probe` timed two *standalone* walks (a fresh-`Vec` `tokenize` per
//! verse, and `unicode-segmentation`'s own grapheme iterator). Neither is the
//! production build: the engine's per-verse products come from `tape::build`,
//! `grapheme::segment_tape` (tape-driven) and the buffer-reusing tokenizer, and
//! their costs differ enough to change which product is worth sharing. This
//! probe times the production builds through `ssc_core::bench`-exposed
//! `walk_floor`, reports the product counts, and prices the compact token
//! encoding a shared product would have to use to stay inside the transient
//! memory budget.
//!
//! Usage: shared_prep_probe <vref-file> [trials]

use std::hint::black_box;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().map(PathBuf::from) else {
        eprintln!("usage: shared_prep_probe <vref-file> [trials]");
        std::process::exit(2);
    };
    let trials: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);

    let bible = spike_bench::vref_io::load_corpus(&path);
    let books = ssc_core::corpus::by_book(&bible);
    let bytes: usize = bible.texts().iter().map(String::len).sum();
    eprintln!(
        "loaded {} verses, {:.2} MB",
        bible.texts().len(),
        bytes as f64 / 1e6
    );

    let time = |needs: ssc_core::FloorNeeds| {
        let mut ts = Vec::with_capacity(trials);
        let mut count = 0usize;
        for _ in 0..trials {
            let t0 = std::time::Instant::now();
            count = black_box(ssc_core::walk_floor(&books, needs));
            ts.push(t0.elapsed());
        }
        (spike_bench::median(&mut ts), count)
    };

    let none = ssc_core::FloorNeeds::default();
    let (t_none, _) = time(none);
    let (t_tape, n_tape) = time(ssc_core::FloorNeeds {
        tape: true,
        ..none
    });
    let (t_gr, n_gr) = time(ssc_core::FloorNeeds {
        tape: true,
        graphemes: true,
        ..none
    });
    let (t_tok, n_tok) = time(ssc_core::FloorNeeds {
        tokens: true,
        ..none
    });

    println!("verse-stream floor (no products): {t_none:?}");
    println!("tape only:            {t_tape:?}  ({n_tape} entries)");
    println!("tape + graphemes:     {t_gr:?}  ({} graphemes)", n_gr - n_tape);
    println!("tokens only:          {t_tok:?}  ({n_tok} tokens)");

    // Storage a whole-corpus shared product would retain, per candidate
    // encoding. The engine's live `Token` is two `u32`s.
    let tokens = n_tok as f64;
    println!();
    println!("whole-corpus token product, retained bytes:");
    println!("  Vec<Token> (8 B/token):        {:.2} MB", tokens * 8.0 / 1e6);
    println!("  u16 start+len (4 B/token):     {:.2} MB", tokens * 4.0 / 1e6);

    // The compact encoding: one byte per token when the gap from the previous
    // token's end and the token's length both fit the packed field, an escape
    // byte plus two varints otherwise. Measure the real hit rate rather than
    // assuming scripture is all short words separated by one space.
    let mut packed = 0u64;
    let mut escaped = 0u64;
    let mut buf: Vec<ssc_core::token::Token> = Vec::new();
    for text in bible.texts() {
        buf = ssc_core::token::tokenize(text);
        let mut prev_end = 0u32;
        for t in &buf {
            let gap = t.span.start - prev_end;
            let len = t.span.end - t.span.start;
            if gap <= 3 && (1..=63).contains(&len) {
                packed += 1;
            } else {
                escaped += 1;
            }
            prev_end = t.span.end;
        }
    }
    black_box(&buf);
    let total = (packed + escaped) as f64;
    println!(
        "  packed 1 B + escape 3 B:       {:.2} MB  ({:.3}% escaped)",
        (packed as f64 + escaped as f64 * 3.0) / 1e6,
        escaped as f64 / total * 100.0
    );

    // Encode: tokenize into a reused buffer, then pack. This is what the first
    // consumer of a chapter pays on top of the walk it already did.
    let mut enc = Vec::with_capacity(trials);
    let mut blob: Vec<u8> = Vec::new();
    let mut ends: Vec<u32> = Vec::new();
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        blob.clear();
        ends.clear();
        for text in bible.texts() {
            let toks = ssc_core::token::tokenize(text);
            let mut prev_end = 0u32;
            for t in &toks {
                let gap = t.span.start - prev_end;
                let len = t.span.end - t.span.start;
                if gap <= 3 && (1..=63).contains(&len) {
                    blob.push(((gap as u8) << 6) | len as u8);
                } else {
                    blob.push(0);
                    blob.extend_from_slice(&t.span.start.to_le_bytes());
                    blob.extend_from_slice(&len.to_le_bytes());
                }
                prev_end = t.span.end;
            }
            ends.push(blob.len() as u32);
        }
        enc.push(t0.elapsed());
    }
    println!();
    // The same loop with the packing removed — the difference is the pack cost.
    let mut bare = Vec::with_capacity(trials);
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        let mut n = 0usize;
        for text in bible.texts() {
            n += black_box(ssc_core::token::tokenize(text)).len();
        }
        bare.push(t0.elapsed());
        black_box(n);
    }
    println!(
        "fresh-Vec tokenize alone:              {:?}",
        spike_bench::median(&mut bare)
    );
    println!(
        "encode (tokenize + pack, whole corpus): {:?}   blob {:.2} MB + {} verse ends",
        spike_bench::median(&mut enc),
        blob.len() as f64 / 1e6,
        ends.len()
    );

    // Decode: what every later consumer pays instead of re-tokenizing.
    let mut dec = Vec::with_capacity(trials);
    for _ in 0..trials {
        let t0 = std::time::Instant::now();
        let mut out: Vec<ssc_core::token::Token> = Vec::new();
        let mut start = 0usize;
        let mut n = 0usize;
        for &end in &ends {
            out.clear();
            let mut prev_end = 0u32;
            let mut i = start;
            while i < end as usize {
                let b = blob[i];
                let (s, len) = if b == 0 {
                    let s = u32::from_le_bytes(blob[i + 1..i + 5].try_into().unwrap());
                    let l = u32::from_le_bytes(blob[i + 5..i + 9].try_into().unwrap());
                    i += 9;
                    (s, l)
                } else {
                    let s = prev_end + u32::from(b >> 6);
                    i += 1;
                    (s, u32::from(b & 63))
                };
                prev_end = s + len;
                out.push(ssc_core::token::Token {
                    span: ssc_core::span::Span {
                        start: s,
                        end: prev_end,
                    },
                });
            }
            n += out.len();
            start = end as usize;
        }
        dec.push(t0.elapsed());
        black_box(n);
    }
    println!(
        "decode (whole corpus):                 {:?}",
        spike_bench::median(&mut dec)
    );
}
