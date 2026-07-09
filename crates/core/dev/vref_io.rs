//! Vref → `VerseMap` loader for DEV TOOLING ONLY (the calibrate example and
//! the criterion benches pull this in via `#[path]`).
//!
//! ADR 0010 keeps file IO out of `core`'s contract; ADR 0040 makes the
//! on-disk corpus form a flat, self-describing vref file — one per corpus at
//! `corpora/vref/<id>.txt`, each line `REF\ttext` where `REF` is the
//! `Sid::parse` form (`GEN 1:1`). Onion builds those files (`cargo xtask
//! build-corpus`); this reader is the whole ingest path — no USFM knowledge,
//! no directory descent. It replaces the retired naive USFM loader.

use std::fs;
use std::path::Path;

use ssc_core::{Sid, VerseMap};

/// Load one corpus vref file (`REF\ttext` per line) into a `VerseMap`.
/// Lines without a tab, or whose ref doesn't parse, are skipped — the writer
/// guarantees neither, so a skip means a hand-edited or truncated file.
// This module is shared via `#[path]`; not every includer calls every fn.
#[allow(dead_code)]
pub fn load_corpus(path: &Path) -> VerseMap {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter_map(|line| {
            let (sid, verse) = line.split_once('\t')?;
            Some((Sid::parse(sid)?, verse.to_string()))
        })
        .collect()
}

/// Resolve a corpus by id (e.g. `WA-en-ulb`) to its vref file under
/// `corpora/vref/`, relative to the `ssc-core` crate so cwd doesn't matter.
#[allow(dead_code)]
pub fn corpus_path(id: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpora/vref")
        .join(format!("{id}.txt"))
}
