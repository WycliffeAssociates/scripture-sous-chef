//! Vref → `Corpus` loader for DEV TOOLING ONLY (the calibrate example and
//! the criterion benches pull this in via `#[path]`).
//!
//! ADR 0010 keeps file IO out of `core`'s contract; ADR 0040 makes the
//! on-disk corpus form a flat, self-describing vref file — one per corpus at
//! `corpora/vref/<id>.txt`, each line `REF\ttext` where `REF` is the key
//! grammar's form (`GEN 1:1`). Onion builds those files (`cargo xtask
//! build-corpus`); this reader is the whole ingest path — no USFM knowledge,
//! no directory descent. It replaces the retired naive USFM loader.

use std::fs;
use std::path::Path;

use ssc_core::key::parse_key;
use ssc_core::Corpus;

/// Load one corpus vref file (`REF\ttext` per line) into a `Corpus`, in file
/// order — a `Corpus` is duplicate-preserving and order-preserving, so
/// repeated/duplicate refs survive rather than being collapsed by a map.
/// Lines without a tab, or whose ref doesn't parse, are skipped — the writer
/// guarantees neither, so a skip means a hand-edited or truncated file.
/// A text of exactly `<range>` is the BibleNLP vref convention for "this
/// verse is bridged into the previous one" — a placeholder, not verse text —
/// and is skipped as an absent verse (the eBible fleet uses it in ~1,050
/// corpora; fed to the rules it reads as literal markup leftovers).
// This module is shared via `#[path]`; not every includer calls every fn.
#[allow(dead_code)]
pub fn load_corpus(path: &Path) -> Corpus {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut keys = Vec::new();
    let mut texts = Vec::new();
    for line in text.lines() {
        let Some((key, verse)) = line.split_once('\t') else {
            continue;
        };
        if verse == "<range>" || parse_key(key).is_err() {
            continue;
        }
        keys.push(key.to_string());
        texts.push(verse.to_string());
    }
    Corpus::try_from_parts(keys, texts).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Resolve a corpus by id (e.g. `WA-en-ulb`) to its vref file under
/// `corpora/vref/`, relative to the `ssc-core` crate so cwd doesn't matter.
#[allow(dead_code)]
pub fn corpus_path(id: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpora/vref")
        .join(format!("{id}.txt"))
}
