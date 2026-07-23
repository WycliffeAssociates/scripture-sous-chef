//! The resident-`Galley` incremental transcript oracle — a complete-snapshot
//! mutation transcript over the vref fleet (granularity-spine Phase A step 5;
//! the echo/serialized-`Stats` oracle it replaced was retired with echo
//! semantics, plan §2.3/§12.5).
//!
//! It lives in `ssc-galley`'s examples — not `ssc-core`'s — so `ssc-core` does
//! not dev-depend on `ssc-galley` (dependency-direction restore, Phase A step 6
//! closeout). It `#[path]`-includes `ssc-core`'s gate-critical shared oracle
//! helpers verbatim (`OracleScope`, `load_corpora`, `resolve_source`,
//! `write_findings`, `oracle_config`) so the transcript's row bytes are
//! single-sourced through `write_findings` and cannot drift from the
//! `--dump-findings` command that still lives in the calibrate example. Output
//! is byte-identical to the pre-move driver.
//!
//! Run (identical arguments to the old `-p ssc-core --example calibrate`
//! command, only the crate/example name changed):
//! ```text
//!   cargo run --release -p ssc-galley --example transcript_oracle -- \
//!       --dump-incremental corpora/vref /tmp/incremental.tsv default
//!   # WA subset (~32 corpora after subsampling); blobs ignore the scope token:
//!   cargo run --release -p ssc-galley --example transcript_oracle -- \
//!       --dump-incremental oracle-blobs/wa.blob /tmp/inc.wa.tsv default wa
//! ```

// This example includes ssc-core's calibrate modules whole for their shared
// helpers; only the transcript path is exercised here, so some included items
// (e.g. `dump_findings`, `oracle_source`) are unused in this binary.
#![allow(dead_code)]
#![allow(clippy::disallowed_types)]

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use ssc_core::BookBlock;
use ssc_galley::Galley;

// The gate-critical shared helpers, re-used verbatim from ssc-core's calibrate
// example via `#[path]` (rather than duplicated) so `write_findings`' row bytes
// have exactly one definition. `corpus_blob` reaches `crate::oracle::*` and
// `crate::vref_io::*`, and `oracle` reaches `crate::{vref_io, corpus_blob}`, so
// the module names below must match those the two files expect.
#[path = "../../core/dev/vref_io.rs"]
mod vref_io;
#[path = "../../core/examples/calibrate/oracle.rs"]
mod oracle;
#[path = "../../core/examples/calibrate/corpus_blob.rs"]
mod corpus_blob;

use oracle::{OracleScope, load_corpora, oracle_config, resolve_source, write_findings};

/// A fixed, multi-rule-provoking edit applied to the last verse of the first
/// book: doubles punctuation, excess whitespace, a rare glyph, a mixed-case
/// word, a spaced comma, an unbalanced paren. (Moved verbatim from ssc-core's
/// `oracle.rs`; the transcript byte-identity depends on this exact string.)
const EDIT_TEXT: &str = "He fell ,, the  gate stood.. qQx deJésus (broken";

/// The incremental oracle. Per corpus, exactly what the editor's resident steady
/// state does: seed a `Galley` over the **complete** corpus (a cold analyze
/// warming its resident prior + prep), apply the fixed `EDIT_TEXT` mutation to
/// the first book as a complete-book replacement (`update_book` — never an echo
/// subset), analyze again, and dump the post-mutation findings for the whole
/// corpus. The mutated book re-tallies by content hash; clean siblings carry.
///
/// Same parallel-render-then-sequential-write shape as `dump_findings` (each
/// corpus's own resident `Galley` is independent, so `par_iter().map(..)
/// .collect()` preserves file order regardless of completion order — byte-stable
/// across runs and thread counts). Keeps the `wa|full` scope token and the
/// stderr scope print.
fn dump_incremental(path: &Path, out_path: &Path, cfg_name: &str, scope: OracleScope) {
    let cfg = oracle_config(cfg_name);
    let corpora = load_corpora(path, scope);
    let source = resolve_source(path, &corpora);
    // Every 8th corpus (plus the first): the incremental gate needs breadth,
    // not the whole fleet, and this dump runs two analyses per corpus. The
    // WA subset is subsampled the same way (~32 corpora) after scope filtering.
    let corpora: Vec<_> = corpora.into_iter().step_by(8).collect();
    let total = corpora.len();
    let done = AtomicUsize::new(0);
    let buffers: Vec<Vec<u8>> = corpora
        .par_iter()
        .map(|(id, target)| {
            let mut buf = Vec::new();
            if target.is_empty() {
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 20 == 0 {
                    eprintln!("{n}/{total}");
                }
                return buf;
            }

            // Seed a resident Galley over the complete corpus (cold analyze
            // warms its prior + prep), then mutate + re-analyze.
            let mut galley = Galley::new(target.clone(), source.clone(), cfg.clone());
            let _ = galley.analyze();

            // The edit: last verse of the first book, as a complete-book
            // replacement. `by_book` is in presented order, so the first book
            // occupies positions `0..first_len` of the corpus.
            let first_books = ssc_core::corpus::by_book(target);
            let first = first_books.first().unwrap();
            let first_len = first.keys.len();
            let first_slug = first.slug.to_string();
            drop(first_books);
            let keys: Vec<String> = target.keys()[..first_len].to_vec();
            let mut texts: Vec<String> = target.texts()[..first_len].to_vec();
            texts[first_len - 1] = EDIT_TEXT.to_string();
            galley
                .update_book(BookBlock {
                    slug: first_slug.into(),
                    keys,
                    texts,
                })
                .expect("first-book replacement is a valid complete-book update");

            let findings = galley.analyze();
            write_findings(&mut buf, id, "snap", galley.corpus(), &findings);

            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 20 == 0 {
                eprintln!("{n}/{total}");
            }
            buf
        })
        .collect();
    let mut out = std::io::BufWriter::new(std::fs::File::create(out_path).unwrap());
    for buf in buffers {
        std::io::Write::write_all(&mut out, &buf).unwrap();
    }
    eprintln!(
        "dumped {total} corpora incremental ({cfg_name}, scope={}) -> {}",
        scope.label(),
        out_path.display()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        // Trailing `wa`|`full` scopes the fleet; a `.blob` path ignores it (the
        // blob already encodes its preset).
        ["--dump-incremental", path, out, cfg_name, rest @ ..] => {
            dump_incremental(
                Path::new(path),
                Path::new(out),
                cfg_name,
                OracleScope::parse(&rest.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            );
        }
        _ => {
            eprintln!(
                "usage: cargo run --release -p ssc-galley --example transcript_oracle -- \\\n  \
                 --dump-incremental <dir|blob> <out.tsv> <default|all> [wa|full]"
            );
            std::process::exit(2);
        }
    }
}
