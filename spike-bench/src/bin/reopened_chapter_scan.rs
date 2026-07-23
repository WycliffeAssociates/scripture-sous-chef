//! Gate-0 fleet scan for the granularity-spine plan (§2 item 2): does any
//! corpus in the VREF fleet REOPEN a book or a chapter?
//!
//! The plan's Bible-shaped structural constraint (§1 owner decision 5) is that
//! books are contiguous and may not reopen, and within a book an opaque
//! chapter token is one contiguous run that may not reopen. `Corpus` already
//! enforces the book half; the chapter half is what Phase A adds and what this
//! scan must find zero counterexamples for before any engine code changes.
//! A single "mover" (a chapter token reappearing after another token closed
//! it, in the caller's presented order) is a hard stop for owner adjudication.
//!
//! Deliberately opaque: chapter tokens are compared only by exact string
//! equality in presented order. NOTHING here parses a chapter or verse token
//! as a number or sorts them — that is the whole point of "opaque token".
//!
//! Reads each `corpora/vref/<id>.txt` directly (the same `REF\ttext` per-line
//! form `dev/vref_io.rs` ingests), replicating its skip rules exactly so the
//! scanned key stream is byte-for-byte what the engine would see: a line with
//! no tab is skipped, a `<range>` placeholder verse is skipped, and a key that
//! fails `parse_key` is skipped. Building a `Corpus` is deliberately avoided:
//! `Corpus::try_from_parts` PANICS on a reopened book, which would mask the
//! very book-contiguity result this scan is meant to report cleanly.
//!
//! Usage:
//!   reopened_chapter_scan <vref-dir>
//! e.g. reopened_chapter_scan ../corpora/vref

use std::fs;
use std::path::{Path, PathBuf};

use ssc_core::key::parse_key;

/// One recorded structural violation, with a corpus/key sample.
struct Violation {
    corpus: String,
    kind: &'static str, // "book-reopen" | "chapter-reopen"
    token: String,      // the reopened book slug or chapter token
    /// The key at which the reopen was observed (the first key of the
    /// offending run), plus the key of the run that closed the token.
    reopen_at: String,
    closed_by: String,
}

fn scan_corpus(id: &str, keys: &[String], out: &mut Vec<Violation>) {
    // Book-level state (across the whole corpus).
    let mut current_book: Option<String> = None;
    let mut closed_books: Vec<(String, String)> = Vec::new(); // (slug, key that closed it)

    // Chapter-level state (reset at every book boundary — a chapter token is
    // only required to be contiguous WITHIN its book).
    let mut current_chapter: Option<String> = None;
    let mut closed_chapters: Vec<(String, String)> = Vec::new(); // (token, key that closed it)

    for key in keys {
        // Keys were already filtered through parse_key by the caller, so this
        // cannot fail; re-parse to borrow the opaque slices.
        let parts = parse_key(key).expect("caller pre-filtered parseable keys");
        let book = parts.book;
        let chapter = parts.chapter;

        let book_changed = current_book.as_deref() != Some(book);
        if book_changed {
            // Closing the previous book also closes its final chapter.
            if let Some(prev_book) = current_book.take() {
                if let Some(prev_ch) = current_chapter.take() {
                    closed_chapters.push((prev_ch, key.clone()));
                }
                closed_books.push((prev_book, key.clone()));
            }
            // Reopened book? (present order — a slug we already closed.)
            if let Some((_, closed_key)) = closed_books.iter().find(|(s, _)| s == book) {
                out.push(Violation {
                    corpus: id.to_string(),
                    kind: "book-reopen",
                    token: book.to_string(),
                    reopen_at: key.clone(),
                    closed_by: closed_key.clone(),
                });
            }
            current_book = Some(book.to_string());
            // Fresh chapter scope for the new book.
            closed_chapters.clear();
            current_chapter = Some(chapter.to_string());
            continue;
        }

        // Same book: check for a chapter transition.
        if current_chapter.as_deref() != Some(chapter) {
            if let Some(prev_ch) = current_chapter.take() {
                closed_chapters.push((prev_ch, key.clone()));
            }
            if let Some((_, closed_key)) = closed_chapters.iter().find(|(t, _)| t == chapter) {
                out.push(Violation {
                    corpus: id.to_string(),
                    kind: "chapter-reopen",
                    token: chapter.to_string(),
                    reopen_at: key.clone(),
                    closed_by: closed_key.clone(),
                });
            }
            current_chapter = Some(chapter.to_string());
        }
    }
}

/// Read one vref file into the ordered, engine-faithful key stream (see the
/// module docs for the skip rules).
fn load_keys(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut keys = Vec::new();
    for line in text.lines() {
        let Some((key, verse)) = line.split_once('\t') else {
            continue;
        };
        if verse == "<range>" || parse_key(key).is_err() {
            continue;
        }
        keys.push(key.to_string());
    }
    keys
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("usage: reopened_chapter_scan <vref-dir>");
            std::process::exit(2);
        });

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();

    let mut violations: Vec<Violation> = Vec::new();
    let mut scanned = 0usize;
    let mut total_keys = 0usize;
    for file in &files {
        let id = file.file_stem().unwrap().to_string_lossy().to_string();
        let keys = load_keys(file);
        total_keys += keys.len();
        scan_corpus(&id, &keys, &mut violations);
        scanned += 1;
    }

    let book_movers = violations.iter().filter(|v| v.kind == "book-reopen").count();
    let chapter_movers = violations
        .iter()
        .filter(|v| v.kind == "chapter-reopen")
        .count();

    println!("=== reopened-chapter fleet scan (Gate 0, granularity-spine §2 item 2) ===");
    println!("vref dir:        {}", dir.display());
    println!("corpora scanned: {scanned}");
    println!("keys scanned:    {total_keys}");
    println!("book reopens:    {book_movers}");
    println!("chapter reopens: {chapter_movers}");
    if violations.is_empty() {
        println!("\nRESULT: ZERO movers — the no-reopened-chapter invariant holds across the fleet.");
    } else {
        println!("\nRESULT: MOVERS FOUND — STOP. Samples (up to 50):");
        for v in violations.iter().take(50) {
            println!(
                "  [{}] corpus={} token={:?} reopened_at={:?} closed_by={:?}",
                v.kind, v.corpus, v.token, v.reopen_at, v.closed_by
            );
        }
        // Nonzero exit so the gate is unmistakable in automation.
        std::process::exit(1);
    }
}
