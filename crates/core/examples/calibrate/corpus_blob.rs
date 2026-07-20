//! Pre-parsed corpus blob cache.
//!
//! The oracle-gate workflow's dominant per-run cost is opening and parsing
//! up to 1,504 individual `corpora/vref/<id>.txt` files from scratch, every
//! single time — pure repeated overhead, since corpus *content* never
//! changes between spike/gate iterations, only the engine code under test
//! does. A blob is a one-time serialization of already-parsed `(id, keys,
//! texts)` triples for a fixed preset, reloaded with one sequential read
//! instead of N file opens.
//!
//! Blobs are generated artifacts under `target/` (gitignored) — never
//! committed, regenerate on demand via `--build-blob`, and this module
//! never touches `Corpus`'s own type definition: it round-trips through the
//! same public `keys()`/`texts()`/`try_from_parts` surface `load_corpus`
//! itself already uses, so a loaded blob reconstructs a `Corpus` identical
//! to loading the source file directly.

use std::path::{Path, PathBuf};

use ssc_core::Corpus;

use crate::oracle::OracleScope;
use crate::vref_io::load_corpus;

#[derive(serde::Serialize, serde::Deserialize)]
struct BlobEntry {
    id: String,
    keys: Vec<String>,
    texts: Vec<String>,
}

/// The three fixed tiers. `Small` is a hand-picked, script-diverse sample
/// (the same diversity principle as the 2026-07-18 grapheme-interning
/// spike) for the fastest possible sanity-check pass, sitting below `Wa`
/// (the ~251-corpus WA subset, an existing `OracleScope`) and `Full` (the
/// whole ~1,504-corpus fleet — the real before/after gate).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Small,
    Wa,
    Full,
}

impl Preset {
    pub fn parse(name: &str) -> Self {
        match name {
            "small" => Self::Small,
            "wa" => Self::Wa,
            "full" => Self::Full,
            other => panic!("unknown preset {other:?} (want small|wa|full)"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Wa => "wa",
            Self::Full => "full",
        }
    }
}

/// Fixed, versioned ~15-corpus diverse sample, pinned by id so every future
/// small-tier spike gets the exact same, reproducible set: two CJK (Chinese,
/// Japanese), two other non-Devanagari Brahmic/abjad/Ethiopic scripts
/// (Telugu, Arabic, Ge'ez), Cyrillic, Thai, Hebrew (the largest-alphabet
/// outlier per the grapheme-interning survey), Vietnamese, plus the WA
/// percentile anchors already used by the wire-format survey.
pub const SMALL_PRESET_IDS: &[&str] = &[
    "cmn-cu89s",
    "jpn1965",
    "hin2017",
    "tel2017",
    "arb-vd",
    "dwrENT",
    "bel",
    "thaKJV",
    "hboWLC",
    "WA-vi-ulb",
    "WA-en-ulb",
    "WA-auh-reg",
    "WA-knx-x-bajare-reg",
    "WA-gnh-reg",
    "WA-bds-reg",
];

fn preset_files(dir: &Path, preset: Preset) -> Vec<PathBuf> {
    match preset {
        Preset::Small => SMALL_PRESET_IDS
            .iter()
            .map(|id| dir.join(format!("{id}.txt")))
            .filter(|p| {
                let exists = p.exists();
                if !exists {
                    eprintln!("warning: small preset id missing from {}: {}", dir.display(), p.display());
                }
                exists
            })
            .collect(),
        Preset::Wa => crate::oracle::oracle_files(dir, OracleScope::Wa),
        Preset::Full => crate::oracle::oracle_files(dir, OracleScope::Full),
    }
}

/// Build a preset's blob from the real corpora directory. Corpus order in
/// the blob is preserved on load — whatever order downstream processing
/// needs (e.g. the oracle gate's deterministic write) is decided by the
/// caller, not baked in here.
pub fn build_blob(dir: &Path, preset: Preset, out_path: &Path) {
    let files = preset_files(dir, preset);
    let total = files.len();
    let entries: Vec<BlobEntry> = files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let id = file.file_stem().unwrap().to_string_lossy().to_string();
            let corpus = load_corpus(file);
            if (i + 1) % 200 == 0 {
                eprintln!("{}/{total}", i + 1);
            }
            BlobEntry {
                id,
                keys: corpus.keys().to_vec(),
                texts: corpus.texts().to_vec(),
            }
        })
        .collect();
    let bytes = bincode::serialize(&entries).expect("serialize corpus blob");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(out_path, &bytes)
        .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
    eprintln!(
        "built {} preset blob: {total} corpora, {} MB -> {}",
        preset.label(),
        bytes.len() / 1_000_000,
        out_path.display()
    );
}

/// Load every corpus out of a preset blob as `(id, Corpus)` pairs, in the
/// order they were written.
pub fn load_blob(path: &Path) -> Vec<(String, Corpus)> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let entries: Vec<BlobEntry> = bincode::deserialize(&bytes).expect("deserialize corpus blob");
    entries
        .into_iter()
        .map(|e| {
            let corpus = Corpus::try_from_parts(e.keys, e.texts)
                .unwrap_or_else(|err| panic!("{}: {err}", e.id));
            (e.id, corpus)
        })
        .collect()
}

pub fn is_blob_path(path: &Path) -> bool {
    path.extension().is_some_and(|x| x == "blob")
}
