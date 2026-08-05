//! THROWAWAY measurement — NOT part of the crate's shipped surface.
//!
//! Follow-up to the ASCII-gate work in `documentation/calibration/
//! 2026-07-17-word-break-fast-path-survey.md`. That gate currently checks
//! `str::is_ascii()` PER VERSE. The next refinement is per-BOOK (the real
//! processing unit — `walk_book` is the parallel-fan-out unit, ADR 0042):
//! sample the first N verses of a book, decide once whether to delegate to
//! `unicode_word_indices()` for the whole book or run the hand-rolled walker
//! for the whole book, then stop checking `is_ascii()` per verse for the
//! rest of that book.
//!
//! This measures, fleet-wide (all 1,504 corpora), the one empirical question
//! that decision needs answered: **how many verses of prefix sampling
//! reliably predict a book's true non-ASCII codepoint density?**
//!
//! For every book of every corpus: compute the TRUE density (total
//! non-ASCII codepoints ÷ total codepoints, summed over every verse) and the
//! density estimated from just the first N verses, for N in {1,2,3,4,5,10,
//! 20,50} (skipping N >= the book's own verse count — that's not a genuine
//! partial sample, it's the whole book). Reports:
//!   1. How the estimate's absolute error shrinks as N grows.
//!   2. Directional agreement against candidate crossover thresholds
//!      (15%/25%/40%/50%) — does the N-verse estimate land on the same side
//!      of a candidate threshold as the true density? — since the real
//!      crossover where the hand-rolled walker starts winning isn't pinned
//!      down yet (only ~0%/~10%/~50%+ are measured so far).
//!   3. Books where even N=50 disagrees with the true value against some
//!      candidate threshold — flagged for direct inspection.
//!
//! Run: cargo run -p ssc-core --release --example ascii_gate_book_sampling

use std::path::Path;

use ssc_core::corpus::by_book;

#[path = "../dev/vref_io.rs"]
mod vref_io;

const CANDIDATE_N: &[usize] = &[1, 2, 3, 4, 5, 10, 20, 50];
const CANDIDATE_THRESHOLDS: &[f64] = &[0.15, 0.25, 0.40, 0.50];

/// `(non_ascii_codepoints, total_codepoints)` for one verse.
fn verse_ascii_counts(text: &str) -> (u64, u64) {
    let mut na = 0u64;
    let mut tot = 0u64;
    for c in text.chars() {
        tot += 1;
        if !c.is_ascii() {
            na += 1;
        }
    }
    (na, tot)
}

/// Density (non-ASCII fraction) over the first `n` verses' worth of
/// `(na, tot)` pairs. `None` if `n` is 0 or the prefix has zero codepoints
/// (an all-empty-text edge case).
fn density_over(per_verse: &[(u64, u64)], n: usize) -> Option<f64> {
    let n = n.min(per_verse.len());
    if n == 0 {
        return None;
    }
    let (na, tot) = per_verse[..n]
        .iter()
        .fold((0u64, 0u64), |(a, b), &(x, y)| (a + x, b + y));
    if tot == 0 {
        None
    } else {
        Some(na as f64 / tot as f64)
    }
}

struct FlaggedBook {
    corpus_id: String,
    slug: String,
    verse_count: usize,
    true_density: f64,
    est50_density: f64,
    disagreed_thresholds: Vec<f64>,
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpora_dir = manifest_dir.join("../../corpora/vref");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&corpora_dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", corpora_dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".txt"))
        })
        .collect();
    files.sort();
    eprintln!("scanning {} corpora (full fleet)", files.len());

    // Per-N: all per-book absolute errors (for percentile stats), across the
    // fleet, restricted to books whose verse_count > N (a genuine partial
    // sample, not "N happens to be >= the whole book").
    let mut errors_by_n: Vec<Vec<f64>> = CANDIDATE_N.iter().map(|_| Vec::new()).collect();
    // Per (N, threshold): (agree_count, total_count).
    let mut agree_by_n_t: Vec<Vec<(u64, u64)>> = CANDIDATE_N
        .iter()
        .map(|_| CANDIDATE_THRESHOLDS.iter().map(|_| (0u64, 0u64)).collect())
        .collect();

    let mut total_books = 0u64;
    let mut eligible_books_by_n = vec![0u64; CANDIDATE_N.len()];
    let mut flagged: Vec<FlaggedBook> = Vec::new();
    let mut book_lengths: Vec<usize> = Vec::new();

    for path in &files {
        let corpus_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let corpus = vref_io::load_corpus(path);
        for group in by_book(&corpus) {
            total_books += 1;
            let per_verse: Vec<(u64, u64)> =
                group.texts.iter().map(|t| verse_ascii_counts(t)).collect();
            let verse_count = per_verse.len();
            book_lengths.push(verse_count);
            let Some(true_density) = density_over(&per_verse, verse_count) else {
                continue; // an all-empty book; skip (nothing to estimate)
            };

            for (ni, &n) in CANDIDATE_N.iter().enumerate() {
                if n >= verse_count {
                    continue; // not a genuine partial sample for this book
                }
                eligible_books_by_n[ni] += 1;
                let Some(est) = density_over(&per_verse, n) else {
                    continue;
                };
                errors_by_n[ni].push((est - true_density).abs());
                for (ti, &t) in CANDIDATE_THRESHOLDS.iter().enumerate() {
                    let agree = (est >= t) == (true_density >= t);
                    let (a, b) = &mut agree_by_n_t[ni][ti];
                    if agree {
                        *a += 1;
                    }
                    *b += 1;
                }

                // Flag disagreements specifically at N=50.
                if n == 50 {
                    let disagreed: Vec<f64> = CANDIDATE_THRESHOLDS
                        .iter()
                        .copied()
                        .filter(|&t| (est >= t) != (true_density >= t))
                        .collect();
                    if !disagreed.is_empty() {
                        flagged.push(FlaggedBook {
                            corpus_id: corpus_id.clone(),
                            slug: group.slug.to_string(),
                            verse_count,
                            true_density,
                            est50_density: est,
                            disagreed_thresholds: disagreed,
                        });
                    }
                }
            }
        }
    }

    println!("=== Book-level ASCII-density prefix-sampling survey (full fleet) ===");
    println!("total books scanned: {total_books}");

    println!(
        "\n--- error shrinkage: |estimate(N) - true density|, percentiles (books w/ verse_count > N only) ---"
    );
    println!(
        "{:>4} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "N", "n_books", "mean", "median", "p90", "p99", "max"
    );
    for (ni, &n) in CANDIDATE_N.iter().enumerate() {
        let mut e = errors_by_n[ni].clone();
        if e.is_empty() {
            println!("{n:>4} {:>10} (no eligible books)", 0);
            continue;
        }
        e.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean: f64 = e.iter().sum::<f64>() / e.len() as f64;
        let pct = |p: f64| e[((e.len() as f64 - 1.0) * p).round() as usize];
        println!(
            "{n:>4} {:>10} {:>9.4}% {:>9.4}% {:>9.4}% {:>9.4}% {:>9.4}%",
            e.len(),
            mean * 100.0,
            pct(0.50) * 100.0,
            pct(0.90) * 100.0,
            pct(0.99) * 100.0,
            e.last().unwrap() * 100.0
        );
    }

    println!(
        "\n--- directional agreement: does estimate(N) land on the same side of threshold T as the true density? ---"
    );
    println!(
        "{:>4} {:>8}{}",
        "N",
        "n_books",
        CANDIDATE_THRESHOLDS
            .iter()
            .map(|t| format!(" {:>10}", format!("T={:.0}%", t * 100.0)))
            .collect::<String>()
    );
    for (ni, &n) in CANDIDATE_N.iter().enumerate() {
        let row: String = agree_by_n_t[ni]
            .iter()
            .map(|&(a, b)| {
                if b == 0 {
                    " (n/a)".to_string()
                } else {
                    format!(" {:>9.4}%", 100.0 * a as f64 / b as f64)
                }
            })
            .collect();
        println!("{n:>4} {:>8}{row}", eligible_books_by_n[ni]);
    }

    println!(
        "\n--- smallest N (from the candidate set) reaching >=99.9% agreement, per threshold ---"
    );
    for (ti, &t) in CANDIDATE_THRESHOLDS.iter().enumerate() {
        let mut found = None;
        for (ni, &n) in CANDIDATE_N.iter().enumerate() {
            let (a, b) = agree_by_n_t[ni][ti];
            if b > 0 && 100.0 * a as f64 / b as f64 >= 99.9 {
                found = Some(n);
                break;
            }
        }
        match found {
            Some(n) => println!("T={:.0}%: N={n}", t * 100.0),
            None => println!(
                "T={:.0}%: not reached by N=50 (highest candidate)",
                t * 100.0
            ),
        }
    }

    println!(
        "\n--- books where N=50 disagrees with the true value on >=1 threshold ({} flagged) ---",
        flagged.len()
    );
    for f in &flagged {
        let ts: Vec<String> = f
            .disagreed_thresholds
            .iter()
            .map(|t| format!("{:.0}%", t * 100.0))
            .collect();
        println!(
            "{} {} (verses={}) true={:.2}% est50={:.2}% disagreed_at=[{}]",
            f.corpus_id,
            f.slug,
            f.verse_count,
            f.true_density * 100.0,
            f.est50_density * 100.0,
            ts.join(",")
        );
    }

    // Real book-length distribution — the ~360 verses/book figure floating
    // around elsewhere is a back-calculated aggregate mean (total verses /
    // total books), which is misleading given how skewed book length is
    // (Psalms ~2,500 verses vs. 3 John ~15). This reports the real shape.
    book_lengths.sort_unstable();
    let bl = &book_lengths;
    let n = bl.len();
    let pct = |p: f64| bl[((n as f64 - 1.0) * p).round() as usize];
    let mean: f64 = bl.iter().sum::<usize>() as f64 / n as f64;
    println!("\n=== Book-length distribution (verses per book, fleet-wide, n={n}) ===");
    println!(
        "min={} p10={} p25={} median={} mean={:.1} p75={} p90={} max={}",
        bl[0],
        pct(0.10),
        pct(0.25),
        pct(0.50),
        mean,
        pct(0.75),
        pct(0.90),
        bl[n - 1]
    );
}
